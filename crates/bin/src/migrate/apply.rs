use crate::migrate::map::Mapping;
use crate::migrate::prompt::{ColumnDecision, TablePlan};
use indicatif::{ProgressBar, ProgressStyle};
use ferrum_core::content_type::{ContentTypeKind, NewContentType};
use ferrum_core::field::{Field, FieldKind};
use ferrum_schema::SchemaService;
use sqlx::PgPool;

#[allow(dead_code)]
fn decision_to_field(d: &ColumnDecision) -> Option<Field> {
    let kind = match &d.mapping {
        Mapping::Field(k) => *k,
        Mapping::Relation => FieldKind::Relation,
        Mapping::Skip => return None,
    };

    let kind_meta = match kind {
        FieldKind::Relation => {
            let target = d.relation_target.as_deref().unwrap_or("");
            serde_json::json!({
                "target": target,
                "cardinality": "many_to_one"
            })
        }
        FieldKind::Enum => {
            serde_json::json!({ "values": d.enum_values })
        }
        _ => serde_json::json!({}),
    };

    Some(Field {
        name: d.field_name.clone(),
        kind,
        required: d.required,
        unique: false,
        default: serde_json::Value::Null,
        max_length: None,
        kind_meta,
    })
}

#[allow(dead_code)]
pub async fn apply_schema(
    schemas: &SchemaService,
    plans: &[TablePlan],
) -> Result<(), anyhow::Error> {
    for plan in plans {
        let fields: Vec<Field> = plan.columns.iter().filter_map(decision_to_field).collect();

        if fields.is_empty() {
            println!(
                "  ⚠ Skipping '{}': no valid fields.",
                plan.content_type_name
            );
            continue;
        }

        let payload = NewContentType {
            name: plan.content_type_name.clone(),
            display_name: plan.display_name.clone(),
            fields,
            options: serde_json::json!({}),
            kind: ContentTypeKind::Collection,
        };

        match schemas.create(payload).await {
            Ok(ct) => println!("  ✓ Created content type '{}'", ct.name),
            Err(e) => println!("  ✗ Failed to create '{}': {}", plan.content_type_name, e),
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn copy_rows(
    source: &PgPool,
    target: &PgPool,
    plans: &[TablePlan],
) -> Result<(), anyhow::Error> {
    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}",
    )
    .unwrap();

    for plan in plans {
        let copy_cols: Vec<&ColumnDecision> = plan
            .columns
            .iter()
            .filter(|c| matches!(&c.mapping, Mapping::Field(_)))
            .collect();

        if copy_cols.is_empty() {
            continue;
        }

        let source_cols: Vec<String> = copy_cols.iter().map(|c| c.source_name.clone()).collect();
        let target_cols: Vec<String> = copy_cols.iter().map(|c| c.field_name.clone()).collect();

        let select_sql = format!(
            "SELECT {} FROM {}",
            source_cols
                .iter()
                .map(|c| format!("\"{c}\""))
                .collect::<Vec<_>>()
                .join(", "),
            plan.source_table
        );

        let rows = sqlx::query(&select_sql).fetch_all(source).await?;
        let total = rows.len() as u64;

        let pb = ProgressBar::new(total);
        pb.set_style(style.clone());
        pb.set_message(format!("Copying {}", plan.source_table));

        let target_table = format!("ct_{}", plan.content_type_name);
        let mut ok = 0u64;
        let mut fail = 0u64;

        for row in &rows {
            let placeholders: Vec<String> =
                (1..=target_cols.len()).map(|i| format!("${i}")).collect();
            let insert_sql = format!(
                "INSERT INTO \"{}\" ({}) VALUES ({})",
                target_table,
                target_cols
                    .iter()
                    .map(|c| format!("\"{c}\""))
                    .collect::<Vec<_>>()
                    .join(", "),
                placeholders.join(", ")
            );

            let mut q = sqlx::query(&insert_sql);
            for col in &source_cols {
                let val: Option<String> = sqlx::Row::try_get(row, col.as_str()).ok().flatten();
                q = q.bind(val);
            }

            let mut tx = target.begin().await?;
            match q.execute(&mut *tx).await {
                Ok(_) => {
                    tx.commit().await?;
                    ok += 1;
                }
                Err(_) => {
                    let _ = tx.rollback().await;
                    fail += 1;
                }
            }
            pb.inc(1);
        }

        pb.finish_with_message(format!(
            "{}: {} migrated, {} failed",
            plan.source_table, ok, fail
        ));
    }

    Ok(())
}
