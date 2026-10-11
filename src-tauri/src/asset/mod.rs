pub mod models;
pub mod repository;
pub mod scanner;
pub mod service;
#[cfg(test)]
mod tests;

pub fn migrate(db: &rusqlite::Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema.sql"))
        .map_err(|e| e.to_string())?;
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
                &tx,
                &models::CoreOrigin {
                    kind: kind.into(),
                    id: row.map_err(|e| e.to_string())?,
                },
            )?;
            repository::register_files(&tx, &id)?;
        }
    }
    tx.execute("UPDATE schema_meta SET value='21' WHERE key='schema_version' AND CAST(value AS INTEGER)<21", []).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
