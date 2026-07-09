pub mod apply;
pub mod inspect;
pub mod map;
pub mod prompt;

use crate::migrate::apply::{apply_schema, copy_rows};
use crate::migrate::inspect::{inspect_table, list_tables};
use crate::migrate::prompt::{confirm_data_migration, confirm_plan, plan_table, select_tables};
use anyhow::{Context, Result};
use clap::Args;
use ferrum_schema::{SchemaRegistry, SchemaService, MIGRATOR};
use sqlx::postgres::PgPoolOptions;

#[derive(Debug, Args)]
pub struct MigrateArgs {
    /// Source Postgres connection string.
    #[arg(long)]
    pub source: String,

    /// Target Ferrum Postgres connection string. Defaults to FERRUM_DATABASE_URL.
    #[arg(long, env = "FERRUM_DATABASE_URL")]
    pub target: String,

    /// Print the migration plan and exit without writing anything.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

pub async fn run(args: MigrateArgs) -> Result<()> {
    let source = PgPoolOptions::new()
        .max_connections(5)
        .connect(&args.source)
        .await
        .context("connect to source database")?;

    let target = PgPoolOptions::new()
        .max_connections(5)
        .connect(&args.target)
        .await
        .context("connect to target database")?;

    MIGRATOR
        .run(&target)
        .await
        .context("apply Ferrum internal migrations to target")?;

    let registry = SchemaRegistry::new();
    registry
        .reload_from_db(&target)
        .await
        .context("hydrate schema registry")?;
    let schemas = SchemaService::new(target.clone(), registry.clone());

    let all_tables = list_tables(&source).await.context("list source tables")?;
    let selected = select_tables(&all_tables);
    if selected.is_empty() {
        println!("No tables selected. Exiting.");
        return Ok(());
    }

    let existing_type_names: Vec<String> = registry
        .list()
        .await
        .iter()
        .map(|ct| ct.name.clone())
        .collect();
    let mut plans = Vec::new();
    for table_name in &selected {
        let source_table = inspect_table(&source, table_name)
            .await
            .with_context(|| format!("inspect table {table_name}"))?;
        if let Some(plan) = plan_table(&source_table, &existing_type_names) {
            plans.push(plan);
        }
    }

    if plans.is_empty() {
        println!("No tables to migrate after mapping. Exiting.");
        return Ok(());
    }

    if !confirm_plan(&plans) || args.dry_run {
        println!("Migration cancelled.");
        return Ok(());
    }

    apply_schema(&schemas, &plans)
        .await
        .context("apply schema")?;

    if confirm_data_migration() {
        copy_rows(&source, &target, &plans)
            .await
            .context("copy rows")?;
    }

    println!("\nMigration complete.");
    Ok(())
}
