pub fn migrate(db: &rusqlite::Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v23.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v24.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v25.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v26.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v27.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute_batch(include_str!("schema-v28.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE schema_meta SET value='28' WHERE key='schema_version' AND CAST(value AS INTEGER)<28",[]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub mod analysis;
pub mod analysis_models;
pub mod analysis_result;
pub mod analysis_runner;
#[cfg(test)]
mod analysis_runner_tests;
pub mod analysis_store;
pub mod analysis_summary;
#[cfg(test)]
mod analysis_summary_tests;
#[cfg(test)]
mod analysis_tests;
pub mod commands;
#[cfg(test)]
mod excerpt_tests;
pub mod excerpts;
pub mod files;
pub mod models;
#[cfg(windows)]
mod native_ipc;
pub mod native_player;
#[cfg(all(windows, debug_assertions))]
pub mod native_qa;
#[cfg(windows)]
pub mod native_window;
pub mod note_archive;
pub mod note_archive_browser;
#[cfg(test)]
mod note_archive_browser_tests;
#[cfg(test)]
mod note_archive_tests;
pub mod note_conversion;
#[cfg(test)]
mod note_conversion_tests;
pub mod note_nodes;
#[cfg(test)]
mod note_nodes_tests;
pub mod note_preview;
pub mod note_preview_runtime;
#[cfg(test)]
mod note_preview_runtime_tests;
#[cfg(test)]
mod note_preview_tests;
pub mod store;
pub mod subtitles;
pub mod transcription;
pub mod transcription_install;
pub mod transcription_runner;
#[cfg(test)]
mod transcription_runner_tests;
#[cfg(test)]
mod transcription_tests;
