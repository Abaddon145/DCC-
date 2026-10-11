pub mod analysis;
pub mod commands;
pub mod entities;
pub mod intelligence_migration;
pub mod intelligence_models;
#[cfg(test)]
mod intelligence_tests;
pub mod metadata;
pub mod models;
pub mod prompt;
pub mod relations;
pub mod repository;
pub mod scanner;
pub mod service;
pub mod shot;
#[cfg(test)]
mod tests;

pub fn migrate(db: &rusqlite::Connection) -> Result<(), String> {
    intelligence_migration::migrate(db)
}

pub(crate) fn migrate_origins(tx: &rusqlite::Connection) -> Result<(), String> {
    for table in ["tags", "localized_tags"] {
        let mut stmt = tx
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| e.to_string())?;
        let columns = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (name, definition) in [
            ("color", "TEXT NOT NULL DEFAULT ''"),
            ("created_at", "TEXT NOT NULL DEFAULT ''"),
        ] {
            if !columns.iter().any(|c| c == name) {
                tx.execute_batch(&format!(
                    "ALTER TABLE {table} ADD COLUMN {name} {definition}"
                ))
                .map_err(|e| e.to_string())?;
            }
        }
    }
    for (kind, table) in [("legacy", "assets"), ("media", "media_entries")] {
        let mut statement = tx
            .prepare(&format!("SELECT id FROM {table}"))
            .map_err(|e| e.to_string())?;
        // Registration does no filesystem I/O; source IDs and managed paths stay untouched.
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let id = repository::register_origin(
                tx,
                &models::CoreOrigin {
                    kind: kind.into(),
                    id: row.map_err(|e| e.to_string())?,
                },
            )?;
            repository::register_files(tx, &id)?;
        }
    }
    tx.execute("UPDATE schema_meta SET value='21' WHERE key='schema_version' AND CAST(value AS INTEGER)<21", []).map_err(|e| e.to_string())?;
    Ok(())
}
