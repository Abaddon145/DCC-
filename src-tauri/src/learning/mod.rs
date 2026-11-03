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
    tx.execute_batch(include_str!("schema-v29.sql"))
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE schema_meta SET value='29' WHERE key='schema_version' AND CAST(value AS INTEGER)<29",[]).map_err(|e|e.to_string())?;
    let version: i64 = tx
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if version < 30 {
        tx.execute_batch(include_str!("schema-v30.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='30' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 31 {
        tx.execute_batch(include_str!("schema-v31.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='31' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 32 {
        tx.execute_batch(include_str!("schema-v32.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='32' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 33 {
        tx.execute_batch(include_str!("schema-v33.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='33' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 34 {
        tx.execute_batch(include_str!("schema-v34.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='34' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 35 {
        tx.execute_batch(include_str!("schema-v35.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='35' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 36 {
        let present: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('learning_import_batches') WHERE name='batch_kind')", [], |row| row.get(0)).map_err(|e| e.to_string())?;
        if !present {
            tx.execute_batch(include_str!("schema-v36.sql"))
                .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "UPDATE schema_meta SET value='36' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 37 {
        tx.execute_batch(include_str!("schema-v37.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='37' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 38 {
        tx.execute_batch(include_str!("schema-v38.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='38' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 39 {
        tx.execute_batch(include_str!("schema-v39.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='39' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if version < 40 {
        tx.execute_batch(include_str!("schema-v40.sql"))
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE schema_meta SET value='40' WHERE key='schema_version'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
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
#[allow(dead_code)]
mod course_apply;
#[cfg(test)]
mod course_apply_tests;
#[allow(dead_code)]
pub mod course_batch;
#[cfg(test)]
mod course_batch_tests;
mod course_directory_watch;
#[path = "course_import_stub.rs"]
pub mod course_import;
#[cfg(all(test, windows))]
#[allow(dead_code)]
mod course_import_legacy;
#[cfg(all(test, windows))]
mod course_import_tests;
#[allow(dead_code)]
mod course_origins;
#[cfg(test)]
mod course_origins_tests;
#[cfg(test)]
mod course_revert_tests;
mod course_snapshot;
#[allow(dead_code)]
pub(crate) mod course_source_readonly;
#[cfg(test)]
mod course_source_readonly_tests;
pub mod excerpt_access;
#[cfg(test)]
mod excerpt_access_tests;
pub mod excerpt_board_source;
#[cfg(test)]
mod excerpt_board_source_tests;
#[cfg(test)]
mod excerpt_file_tests;
pub mod excerpt_files;
#[cfg(test)]
mod excerpt_image_publish_tests;
pub mod excerpt_publish;
#[cfg(test)]
mod excerpt_publish_tests;
pub mod excerpt_runner;
#[cfg(test)]
mod excerpt_runner_tests;
pub mod excerpt_runtime;
#[cfg(test)]
mod excerpt_runtime_tests;
pub mod excerpt_store;
#[cfg(test)]
mod excerpt_store_tests;
#[cfg(test)]
mod excerpt_tests;
pub mod excerpts;
pub mod files;
#[cfg(windows)]
pub mod import_access;
#[cfg(not(windows))]
#[path = "import_access_stub.rs"]
pub mod import_access;
#[cfg(all(test, windows))]
mod import_access_tests;
#[allow(dead_code)]
mod import_course_preview;
#[cfg(test)]
mod import_course_preview_tests;
#[cfg(test)]
mod import_file_check;
#[cfg(test)]
mod import_file_check_tests;
#[cfg(test)]
mod import_manifest;
#[cfg(test)]
mod import_manifest_tests;
mod import_ops;
#[cfg(test)]
mod import_ops_tests;
#[cfg(test)]
mod import_revert_tests;
pub mod import_runner;
#[cfg(test)]
mod import_runner_tests;
#[cfg(test)]
mod import_snapshot;
#[cfg(test)]
mod import_snapshot_tests;
#[cfg(test)]
mod import_table_check;
#[cfg(test)]
mod import_table_check_tests;
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
mod note_origins;
#[cfg(test)]
mod note_origins_tests;
pub mod note_preview;
pub mod note_preview_runtime;
#[cfg(test)]
mod note_preview_runtime_tests;
#[cfg(test)]
mod note_preview_tests;
pub mod project_notes;
#[cfg(test)]
mod project_notes_tests;
#[cfg(windows)]
mod source_markdown;
#[cfg(all(test, windows))]
mod source_markdown_tests;
pub mod source_profiles;
#[cfg(test)]
mod source_profiles_tests;
#[cfg(windows)]
#[allow(dead_code)]
mod source_sqlite;
pub mod store;
pub mod subtitles;
pub mod transcription;
pub mod transcription_install;
pub mod transcription_runner;
#[cfg(test)]
mod transcription_runner_tests;
#[cfg(test)]
mod transcription_tests;
