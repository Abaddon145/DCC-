use rusqlite::Connection;

pub fn migrate(db: &Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("intelligence_schema.sql"))
        .map_err(|e| e.to_string())?;
    // The final schema version advances only when all v0.23 migrations are installed.
    tx.commit().map_err(|e| e.to_string())
}
