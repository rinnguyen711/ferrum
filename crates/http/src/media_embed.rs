//! Always-on media embed pass. Runs after `row_to_json` on every entry read
//! (single GET and list), replacing bare media ids with full asset objects.
//! Not gated by `?populate`. Single media -> object or null; multiple media ->
//! ordered array of asset objects.

use crate::media::store;
use crate::routes::media::AssetView;
use rustapi_core::{ContentType, Error, FieldKind};
use serde_json::{Map, Value};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

/// Group ordered (parent, asset_id) join rows into per-parent id lists, preserving
/// the order they arrive in (caller SELECTs `ORDER BY <ct>_id, position`).
pub fn group_gallery_ids(parents: &[Uuid], fetched: Vec<(Uuid, Uuid)>) -> HashMap<Uuid, Vec<Uuid>> {
    let mut out: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for p in parents {
        out.insert(*p, Vec::new());
    }
    for (p, asset) in fetched {
        out.entry(p).or_default().push(asset);
    }
    out
}

fn internal(e: impl Into<anyhow::Error>) -> Error {
    Error::Internal(e.into())
}

/// Embed all media fields on `rows` in place.
pub async fn apply_media_embed(
    pool: &PgPool,
    ct: &ContentType,
    rows: &mut [Map<String, Value>],
) -> Result<(), Error> {
    let media_fields: Vec<&rustapi_core::Field> = ct
        .fields
        .iter()
        .filter(|f| f.kind == FieldKind::Media)
        .collect();
    if media_fields.is_empty() {
        return Ok(());
    }

    let parent_ids: Vec<Uuid> = rows
        .iter()
        .filter_map(|r| {
            r.get("id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
        })
        .collect();

    let mut galleries: HashMap<String, HashMap<Uuid, Vec<Uuid>>> = HashMap::new();
    let mut all_asset_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    for f in &media_fields {
        let multiple = f.media_meta().map(|m| m.multiple).unwrap_or(false);
        if multiple {
            if parent_ids.is_empty() {
                galleries.insert(f.name.clone(), HashMap::new());
                continue;
            }
            let jt = rustapi_sql::media_join_table_name(&ct.name, &f.name)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            let owner_col = format!("{}_id", ct.name);
            let owner_q = rustapi_sql::quote_ident(&owner_col)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            let sql = format!(
                "SELECT {owner_q} AS parent, \"asset_id\" FROM {jt} WHERE {owner_q} = ANY($1) ORDER BY {owner_q}, \"position\""
            );
            let fetched = sqlx::query(&sql)
                .bind(&parent_ids)
                .fetch_all(pool)
                .await
                .map_err(internal)?;
            let mut pairs: Vec<(Uuid, Uuid)> = Vec::with_capacity(fetched.len());
            for row in &fetched {
                let parent: Uuid = row.try_get("parent").map_err(internal)?;
                let asset: Uuid = row.try_get("asset_id").map_err(internal)?;
                all_asset_ids.insert(asset);
                pairs.push((parent, asset));
            }
            galleries.insert(f.name.clone(), group_gallery_ids(&parent_ids, pairs));
        } else {
            for r in rows.iter() {
                if let Some(Value::String(s)) = r.get(&f.name) {
                    if let Ok(u) = Uuid::parse_str(s) {
                        all_asset_ids.insert(u);
                    }
                }
            }
        }
    }

    let mut by_id: HashMap<Uuid, Value> = HashMap::new();
    if !all_asset_ids.is_empty() {
        let ids: Vec<Uuid> = all_asset_ids.into_iter().collect();
        let asset_rows = store::get_assets_by_ids(pool, &ids)
            .await
            .map_err(internal)?;
        for row in asset_rows {
            let id = row.id;
            let obj = serde_json::to_value(AssetView::from(row))
                .map_err(|e| internal(anyhow::anyhow!(e)))?;
            by_id.insert(id, obj);
        }
    }

    for r in rows.iter_mut() {
        let pid = r
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        for f in &media_fields {
            let multiple = f.media_meta().map(|m| m.multiple).unwrap_or(false);
            if multiple {
                let list = pid
                    .and_then(|p| galleries.get(&f.name).and_then(|g| g.get(&p)))
                    .cloned()
                    .unwrap_or_default();
                let arr: Vec<Value> = list
                    .iter()
                    .filter_map(|id| by_id.get(id).cloned())
                    .collect();
                r.insert(f.name.clone(), Value::Array(arr));
            } else {
                let resolved = match r.get(&f.name) {
                    Some(Value::String(s)) => {
                        Uuid::parse_str(s).ok().and_then(|u| by_id.get(&u).cloned())
                    }
                    _ => None,
                };
                r.insert(f.name.clone(), resolved.unwrap_or(Value::Null));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_gallery_preserves_order_and_seeds_empty() {
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let grouped = group_gallery_ids(&[p1, p2], vec![(p1, a), (p1, b)]);
        assert_eq!(grouped.get(&p1).unwrap(), &vec![a, b]);
        assert!(grouped.get(&p2).unwrap().is_empty());
    }
}
