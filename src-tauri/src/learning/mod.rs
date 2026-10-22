pub fn migrate(db: &rusqlite::Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v23.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v24.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE schema_meta SET value='24' WHERE key='schema_version' AND CAST(value AS INTEGER)<24",[]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub mod commands;
pub mod files;
pub mod models;
#[cfg(windows)]
mod native_ipc;
pub mod native_player;
#[cfg(all(windows, debug_assertions))]
pub mod native_qa;
#[cfg(windows)]
pub mod native_window;
pub mod store;
