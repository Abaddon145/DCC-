pub fn migrate(db: &rusqlite::Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v23.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE schema_meta SET value='23' WHERE key='schema_version' AND CAST(value AS INTEGER)<23",[]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
