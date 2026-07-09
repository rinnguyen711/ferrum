use crate::migrate::inspect::{SourceColumn, SourceTable};
use crate::migrate::map::Mapping;
use dialoguer::{Confirm, Input, MultiSelect, Select};
use ferrum_core::field::FieldKind;

/// User-confirmed mapping for one column.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ColumnDecision {
    pub source_name: String,
    pub field_name: String,
    pub mapping: Mapping,
    pub relation_target: Option<String>,
    pub required: bool,
    pub enum_values: Vec<String>,
}

/// User-confirmed plan for one table.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TablePlan {
    pub source_table: String,
    pub content_type_name: String,
    pub display_name: String,
    pub columns: Vec<ColumnDecision>,
}

/// Phase 1: let user pick which tables to migrate.
#[allow(dead_code)]
pub fn select_tables(tables: &[String]) -> Vec<String> {
    if tables.is_empty() {
        println!("No user tables found in source database.");
        return vec![];
    }
    let defaults = vec![true; tables.len()];
    let chosen = MultiSelect::new()
        .with_prompt("Select tables to migrate (space to toggle, enter to confirm)")
        .items(tables)
        .defaults(&defaults)
        .interact()
        .unwrap_or_default();
    chosen.into_iter().map(|i| tables[i].clone()).collect()
}

/// Phase 2: confirm/edit mapping for one table.
#[allow(dead_code)]
pub fn plan_table(table: &SourceTable, existing_type_names: &[String]) -> Option<TablePlan> {
    println!("\n── Table: {} ──", table.table_name);

    let proposed_name = table.table_name.to_lowercase();
    let ct_name: String = Input::new()
        .with_prompt("Content type name (API ID)")
        .default(proposed_name.clone())
        .interact_text()
        .unwrap_or(proposed_name.clone());

    let proposed_display = ct_name
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let display_name: String = Input::new()
        .with_prompt("Display name")
        .default(proposed_display.clone())
        .interact_text()
        .unwrap_or(proposed_display);

    let mut columns = Vec::new();
    for col in &table.columns {
        if let Some(decision) = plan_column(col, &ct_name, existing_type_names) {
            columns.push(decision);
        }
    }

    if columns.is_empty() {
        println!("  ⚠ No fields selected for {ct_name}. Skipping.");
        return None;
    }

    Some(TablePlan {
        source_table: table.table_name.clone(),
        content_type_name: ct_name,
        display_name,
        columns,
    })
}

#[allow(dead_code)]
fn plan_column(
    col: &SourceColumn,
    _ct_name: &str,
    existing_type_names: &[String],
) -> Option<ColumnDecision> {
    let mapping_label = describe_mapping(&col.mapping);
    println!(
        "  Column: {} ({}) → {}",
        col.column_name, col.pg_type, mapping_label
    );

    if col.mapping == Mapping::Skip {
        println!("    (auto-skipped)");
        return None;
    }

    let choices = &["Accept", "Change kind", "Rename field", "Skip"];
    let choice = Select::new()
        .with_prompt("    Action")
        .items(choices)
        .default(0)
        .interact()
        .unwrap_or(0);

    if choice == 3 {
        return None;
    }

    let field_name = if choice == 2 {
        Input::new()
            .with_prompt("    New field name")
            .default(col.column_name.clone())
            .interact_text()
            .unwrap_or_else(|_| col.column_name.clone())
    } else {
        col.column_name.clone()
    };

    let mapping = if choice == 1 {
        pick_kind()
    } else {
        col.mapping.clone()
    };

    let relation_target = if mapping == Mapping::Relation {
        Some(pick_relation_target(existing_type_names))
    } else {
        None
    };

    let required = !col.is_nullable
        && Confirm::new()
            .with_prompt(format!("    Mark '{field_name}' as required?"))
            .default(false)
            .interact()
            .unwrap_or(false);

    Some(ColumnDecision {
        source_name: col.column_name.clone(),
        field_name,
        mapping,
        relation_target,
        required,
        enum_values: col.enum_values.clone(),
    })
}

#[allow(dead_code)]
fn pick_kind() -> Mapping {
    let kinds = &[
        ("String (short text)", Mapping::Field(FieldKind::String)),
        ("Text (long text)", Mapping::Field(FieldKind::Text)),
        ("Integer", Mapping::Field(FieldKind::Integer)),
        ("Float", Mapping::Field(FieldKind::Float)),
        ("Boolean", Mapping::Field(FieldKind::Boolean)),
        ("Datetime", Mapping::Field(FieldKind::Datetime)),
        ("Json", Mapping::Field(FieldKind::Json)),
        ("Enum", Mapping::Field(FieldKind::Enum)),
        ("Relation", Mapping::Relation),
        ("Skip", Mapping::Skip),
    ];
    let labels: Vec<&str> = kinds.iter().map(|(l, _)| *l).collect();
    let i = Select::new()
        .with_prompt("    Choose field kind")
        .items(&labels)
        .default(0)
        .interact()
        .unwrap_or(0);
    kinds[i].1.clone()
}

#[allow(dead_code)]
fn pick_relation_target(existing: &[String]) -> String {
    if existing.is_empty() {
        return Input::new()
            .with_prompt("    Target content type name")
            .interact_text()
            .unwrap_or_default();
    }
    let i = Select::new()
        .with_prompt("    Target content type")
        .items(existing)
        .default(0)
        .interact()
        .unwrap_or(0);
    existing[i].clone()
}

#[allow(dead_code)]
fn describe_mapping(m: &Mapping) -> &'static str {
    match m {
        Mapping::Field(FieldKind::String) => "String",
        Mapping::Field(FieldKind::Text) => "Text",
        Mapping::Field(FieldKind::Integer) => "Integer",
        Mapping::Field(FieldKind::Float) => "Float",
        Mapping::Field(FieldKind::Boolean) => "Boolean",
        Mapping::Field(FieldKind::Datetime) => "Datetime",
        Mapping::Field(FieldKind::Json) => "Json",
        Mapping::Field(FieldKind::Enum) => "Enum",
        Mapping::Relation => "Relation (FK)",
        Mapping::Skip => "Skip",
        _ => "Unknown",
    }
}

/// Phase 3: print summary and ask for final confirmation.
#[allow(dead_code)]
pub fn confirm_plan(plans: &[TablePlan]) -> bool {
    println!("\n── Migration Plan ──");
    for p in plans {
        println!(
            "  {} → content type '{}' ({} fields)",
            p.source_table,
            p.content_type_name,
            p.columns.len()
        );
        for c in &p.columns {
            println!("    {} → {} ({:?})", c.source_name, c.field_name, c.mapping);
        }
    }
    Confirm::new()
        .with_prompt("\nApply this schema to the target database?")
        .default(false)
        .interact()
        .unwrap_or(false)
}

/// Phase 5: ask whether to copy rows.
#[allow(dead_code)]
pub fn confirm_data_migration() -> bool {
    Confirm::new()
        .with_prompt("Migrate existing rows?")
        .default(false)
        .interact()
        .unwrap_or(false)
}
