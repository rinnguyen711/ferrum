//! Declarative schema sync: load content types from TOML file(s) and reconcile
//! the database to match on startup. See
//! docs/superpowers/specs/2026-06-14-schema-as-code-toml-sync-design.md.

use std::path::Path;

use rustapi_core::{ContentType, Error, Field, NewContentType, ValidationErrors};
use serde::Deserialize;

/// How aggressively sync reconciles the DB toward the TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Create missing types, add missing fields. Never drop. (default)
    #[default]
    Additive,
    /// Also drop types/fields absent from the TOML.
    Full,
}

impl SyncMode {
    /// Parse from the `RUSTAPI_SCHEMA_SYNC` env value. Unknown/empty → Additive.
    pub fn from_env_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "full" => SyncMode::Full,
            _ => SyncMode::Additive,
        }
    }
}

/// One TOML file's worth of content types.
#[derive(Debug, Deserialize)]
struct SchemaFile {
    #[serde(default, rename = "content_type")]
    content_types: Vec<TomlContentType>,
}

/// A content type as declared in TOML. Maps onto `NewContentType`; `field` is
/// renamed so the TOML key is `[[content_type.field]]`.
#[derive(Debug, Deserialize)]
struct TomlContentType {
    name: String,
    display_name: String,
    #[serde(default)]
    kind: rustapi_core::ContentTypeKind,
    #[serde(default)]
    options: serde_json::Value,
    #[serde(default, rename = "field")]
    fields: Vec<Field>,
}

impl From<TomlContentType> for NewContentType {
    fn from(t: TomlContentType) -> Self {
        NewContentType {
            name: t.name,
            display_name: t.display_name,
            fields: t.fields,
            options: t.options,
            kind: t.kind,
        }
    }
}

/// One reconciliation step computed by `plan_sync`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SyncAction {
    /// Type in TOML, absent from DB.
    Create(NewContentType),
    /// Type in both: add these fields, drop these field names (drops only in Full).
    Patch {
        name: String,
        add_fields: Vec<Field>,
        drop_fields: Vec<String>,
        options: serde_json::Value,
    },
    /// Type in DB, absent from TOML (Full mode only).
    DropType(String),
    /// Type in DB, absent from TOML (Additive): clear its `managed` flag.
    Unmanage(String),
}

/// Compute the reconciliation plan. Pure: no DB. `desired` is the TOML set,
/// `current` the live registry list. Returns actions in no particular order;
/// the apply loop orders creates by relation dependency.
pub(crate) fn plan_sync(
    desired: &[NewContentType],
    current: &[ContentType],
    mode: SyncMode,
) -> Result<Vec<SyncAction>, Error> {
    use std::collections::HashMap;
    let cur: HashMap<&str, &ContentType> = current.iter().map(|c| (c.name.as_str(), c)).collect();
    let des: HashMap<&str, &NewContentType> =
        desired.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut actions = Vec::new();

    for d in desired {
        match cur.get(d.name.as_str()) {
            None => actions.push(SyncAction::Create(d.clone())),
            Some(existing) => {
                let cur_fields: HashMap<&str, &Field> =
                    existing.fields.iter().map(|f| (f.name.as_str(), f)).collect();
                let des_fields: HashMap<&str, &Field> =
                    d.fields.iter().map(|f| (f.name.as_str(), f)).collect();

                let mut add_fields = Vec::new();
                for f in &d.fields {
                    match cur_fields.get(f.name.as_str()) {
                        None => add_fields.push(f.clone()),
                        Some(cf) => {
                            if cf.kind != f.kind || cf.kind_meta != f.kind_meta {
                                return Err(Error::Validation(ValidationErrors::field(
                                    &f.name,
                                    format!(
                                        "field `{}` on `{}` changed kind/meta; not supported \
                                         (drop+add in full mode, or edit in UI)",
                                        f.name, d.name
                                    ),
                                )));
                            }
                        }
                    }
                }

                let mut drop_fields = Vec::new();
                if mode == SyncMode::Full {
                    for f in &existing.fields {
                        if !des_fields.contains_key(f.name.as_str())
                            && !rustapi_core::is_system_column(&f.name)
                        {
                            drop_fields.push(f.name.clone());
                        }
                    }
                }

                actions.push(SyncAction::Patch {
                    name: d.name.clone(),
                    add_fields,
                    drop_fields,
                    options: managed_options(&d.options),
                });
            }
        }
    }

    for c in current {
        if !des.contains_key(c.name.as_str()) {
            match mode {
                SyncMode::Full => actions.push(SyncAction::DropType(c.name.clone())),
                SyncMode::Additive => {
                    if c.managed() {
                        actions.push(SyncAction::Unmanage(c.name.clone()));
                    }
                }
            }
        }
    }

    Ok(actions)
}

/// Merge `managed = true` into a type's declared options.
fn managed_options(declared: &serde_json::Value) -> serde_json::Value {
    let mut obj = declared.as_object().cloned().unwrap_or_default();
    obj.insert("managed".into(), serde_json::Value::Bool(true));
    serde_json::Value::Object(obj)
}

/// Parse a single TOML document into content types.
pub(crate) fn parse_toml(doc: &str) -> Result<Vec<NewContentType>, Error> {
    let parsed: SchemaFile = toml::from_str(doc)
        .map_err(|e| Error::Validation(ValidationErrors::single(format!("schema TOML parse: {e}"))))?;
    Ok(parsed.content_types.into_iter().map(Into::into).collect())
}

/// Load + merge all content types from a path. If `path` is a directory, every
/// `*.toml` file in it (non-recursive) is parsed and merged. If a file, that one
/// file is parsed. Duplicate type names across files are rejected.
pub(crate) fn load_desired(path: &Path) -> Result<Vec<NewContentType>, Error> {
    let mut docs: Vec<(String, String)> = Vec::new();
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("read schema dir {path:?}: {e}")))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
            .collect();
        entries.sort();
        for p in entries {
            let body = std::fs::read_to_string(&p)
                .map_err(|e| Error::Internal(anyhow::anyhow!("read {p:?}: {e}")))?;
            docs.push((p.display().to_string(), body));
        }
    } else {
        let body = std::fs::read_to_string(path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("read {path:?}: {e}")))?;
        docs.push((path.display().to_string(), body));
    }

    let mut merged: Vec<NewContentType> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (label, body) in docs {
        for ct in parse_toml(&body)? {
            if !seen.insert(ct.name.clone()) {
                return Err(Error::Validation(ValidationErrors::single(format!(
                    "duplicate content type `{}` (in {label})",
                    ct.name
                ))));
            }
            merged.push(ct);
        }
    }
    Ok(merged)
}

/// Order creates so a relation's target is created before the dependent type.
/// Stable topological sort by relation targets; self-references and cycles fall
/// back to declaration order (DB-level checks still apply at apply time).
fn order_creates(mut creates: Vec<NewContentType>) -> Vec<NewContentType> {
    use std::collections::HashSet;
    let names: HashSet<String> = creates.iter().map(|c| c.name.clone()).collect();
    let mut ordered: Vec<NewContentType> = Vec::with_capacity(creates.len());
    let mut placed: HashSet<String> = HashSet::new();

    while !creates.is_empty() {
        let idx = creates.iter().position(|c| {
            c.fields.iter().all(|f| match f.relation_meta() {
                Some(m) => {
                    m.target == c.name
                        || !names.contains(&m.target)
                        || placed.contains(&m.target)
                }
                None => true,
            })
        });
        match idx {
            Some(i) => {
                let c = creates.remove(i);
                placed.insert(c.name.clone());
                ordered.push(c);
            }
            None => {
                ordered.append(&mut creates);
                break;
            }
        }
    }
    ordered
}

/// Entry point called at boot. Loads TOML from `path`, diffs against the live
/// registry, and applies the plan through `SchemaService`. Fail-fast: the first
/// error aborts (and propagates so the server refuses to boot).
pub async fn sync_from_path(
    schemas: &crate::SchemaService,
    path: &str,
    mode: SyncMode,
) -> Result<(), Error> {
    let path = Path::new(path);
    let desired = load_desired(path)?;
    for ct in &desired {
        ct.validate().map_err(Error::from)?;
    }

    let current = schemas.registry().list().await;
    let actions = plan_sync(&desired, &current, mode)?;

    let (creates, others): (Vec<_>, Vec<_>) = actions
        .into_iter()
        .partition(|a| matches!(a, SyncAction::Create(_)));
    let create_cts: Vec<NewContentType> = creates
        .into_iter()
        .map(|a| match a {
            SyncAction::Create(c) => c,
            _ => unreachable!(),
        })
        .collect();

    let mut created = 0usize;
    let mut patched = 0usize;
    let mut dropped = 0usize;
    let mut unmanaged = 0usize;

    for mut nct in order_creates(create_cts) {
        nct.options = managed_options(&nct.options);
        schemas.create(nct).await?;
        created += 1;
    }

    for action in others {
        match action {
            SyncAction::Patch { name, add_fields, drop_fields, options } => {
                let existing = schemas.registry().get(&name).await;
                let options_changed = existing
                    .as_ref()
                    .map(|e| e.options != options)
                    .unwrap_or(true);
                if add_fields.is_empty() && drop_fields.is_empty() && !options_changed {
                    continue;
                }
                let patch = rustapi_core::PatchContentType {
                    display_name: None,
                    add_fields,
                    drop_fields,
                    extend_enum_values: vec![],
                    options: Some(options),
                };
                schemas.patch(&name, patch).await?;
                patched += 1;
            }
            SyncAction::DropType(name) => {
                schemas.delete(&name).await?;
                dropped += 1;
            }
            SyncAction::Unmanage(name) => {
                if let Some(existing) = schemas.registry().get(&name).await {
                    let mut obj = existing.options.as_object().cloned().unwrap_or_default();
                    obj.remove("managed");
                    let patch = rustapi_core::PatchContentType {
                        display_name: None,
                        add_fields: vec![],
                        drop_fields: vec![],
                        extend_enum_values: vec![],
                        options: Some(serde_json::Value::Object(obj)),
                    };
                    schemas.patch(&name, patch).await?;
                    unmanaged += 1;
                }
            }
            SyncAction::Create(_) => unreachable!("creates handled above"),
        }
    }

    tracing::info!(created, patched, dropped, unmanaged, ?mode, "schema sync complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustapi_core::FieldKind;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn fld(name: &str) -> Field {
        Field {
            name: name.into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        }
    }

    fn nct(name: &str, fields: Vec<Field>) -> NewContentType {
        NewContentType {
            name: name.into(),
            display_name: name.into(),
            fields,
            options: json!({}),
            kind: rustapi_core::ContentTypeKind::Collection,
        }
    }

    fn ct(name: &str, fields: Vec<Field>, managed: bool) -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: name.into(),
            display_name: name.into(),
            fields,
            options: if managed { json!({ "managed": true }) } else { json!({}) },
            kind: rustapi_core::ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn diff_creates_missing_type() {
        let desired = vec![nct("post", vec![fld("title")])];
        let actions = plan_sync(&desired, &[], SyncMode::Additive).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], SyncAction::Create(c) if c.name == "post"));
    }

    #[test]
    fn diff_adds_missing_field() {
        let desired = vec![nct("post", vec![fld("title"), fld("body")])];
        let current = vec![ct("post", vec![fld("title")], true)];
        let actions = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &actions[0] {
            SyncAction::Patch { add_fields, drop_fields, .. } => {
                assert_eq!(add_fields.len(), 1);
                assert_eq!(add_fields[0].name, "body");
                assert!(drop_fields.is_empty());
            }
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn diff_drop_field_only_in_full() {
        let desired = vec![nct("post", vec![fld("title")])];
        let current = vec![ct("post", vec![fld("title"), fld("body")], true)];

        let add = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &add[0] {
            SyncAction::Patch { drop_fields, .. } => assert!(drop_fields.is_empty()),
            other => panic!("expected Patch, got {other:?}"),
        }

        let full = plan_sync(&desired, &current, SyncMode::Full).unwrap();
        match &full[0] {
            SyncAction::Patch { drop_fields, .. } => assert_eq!(drop_fields, &vec!["body".to_string()]),
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn diff_drop_type_full_unmanage_additive() {
        let current = vec![ct("legacy", vec![fld("x")], true)];
        let full = plan_sync(&[], &current, SyncMode::Full).unwrap();
        assert_eq!(full, vec![SyncAction::DropType("legacy".into())]);

        let add = plan_sync(&[], &current, SyncMode::Additive).unwrap();
        assert_eq!(add, vec![SyncAction::Unmanage("legacy".into())]);
    }

    #[test]
    fn diff_unmanaged_db_only_type_left_alone_additive() {
        let current = vec![ct("uionly", vec![fld("x")], false)];
        let add = plan_sync(&[], &current, SyncMode::Additive).unwrap();
        assert!(add.is_empty());
    }

    #[test]
    fn diff_field_kind_change_errors() {
        let mut changed = fld("title");
        changed.kind = FieldKind::Integer;
        let desired = vec![nct("post", vec![changed])];
        let current = vec![ct("post", vec![fld("title")], true)];
        let err = plan_sync(&desired, &current, SyncMode::Full).unwrap_err();
        assert!(format!("{err:?}").contains("changed kind/meta"));
    }

    #[test]
    fn diff_patch_sets_managed_option() {
        let desired = vec![nct("post", vec![fld("title")])];
        let current = vec![ct("post", vec![fld("title")], false)];
        let actions = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &actions[0] {
            SyncAction::Patch { options, .. } => {
                assert_eq!(options.get("managed").and_then(|v| v.as_bool()), Some(true));
            }
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn parses_type_with_fields() {
        let doc = r#"
[[content_type]]
name = "post"
display_name = "Post"
kind = "collection"
options = { draft_publish = true }

  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true
"#;
        let cts = parse_toml(doc).expect("parse");
        assert_eq!(cts.len(), 1);
        assert_eq!(cts[0].name, "post");
        assert_eq!(cts[0].fields.len(), 1);
        assert_eq!(cts[0].fields[0].name, "title");
        assert_eq!(cts[0].fields[0].kind, FieldKind::String);
        assert!(cts[0].fields[0].required);
    }

    #[test]
    fn sync_mode_from_env() {
        assert_eq!(SyncMode::from_env_str("full"), SyncMode::Full);
        assert_eq!(SyncMode::from_env_str("FULL"), SyncMode::Full);
        assert_eq!(SyncMode::from_env_str("additive"), SyncMode::Additive);
        assert_eq!(SyncMode::from_env_str("garbage"), SyncMode::Additive);
    }

    #[test]
    fn order_creates_places_target_before_dependent() {
        let mut post = nct("post", vec![fld("title")]);
        let mut rel = fld("author");
        rel.kind = FieldKind::Relation;
        rel.kind_meta = json!({"target": "author", "cardinality": "many_to_one"});
        post.fields.push(rel);
        let author = nct("author", vec![fld("name")]);

        // Declared post-first; ordering must move author ahead.
        let ordered = super::order_creates(vec![post, author]);
        assert_eq!(ordered[0].name, "author");
        assert_eq!(ordered[1].name, "post");
    }
}
