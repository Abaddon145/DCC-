//! Non-Windows stub: Markdown vault leases are Windows-only in this slice.
use crate::{learning::import_runner::ImportPrepareNote, state::AppState, tasks::LibraryTask};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceBinding {
    pub profile_id: String,
    pub epoch: String,
    pub vault: String,
    pub root: String,
    pub library_id: String,
    pub boundary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqliteSourceBinding {
    pub selection_id: String,
    pub library_id: String,
    pub profile_id: String,
    pub epoch: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportNotePreview {
    pub profile_id: String,
    pub epoch: String,
    pub vault: String,
    pub relative_path: String,
    pub source_sha256: String,
    pub selection_id: String,
    pub target_token: String,
    pub conflict: String,
    pub target_id: Option<String>,
    pub title: String,
    pub pending_conversion: bool,
    pub warnings: Vec<String>,
    pub can_prepare: bool,
    pub will_overwrite: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchView {
    pub id: String,
    pub status: String,
    pub request_sha256: String,
    pub source_profile_id: String,
    pub source_epoch: String,
    pub current_source_epoch: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub receipt: Option<Value>,
    pub revert_report: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemView {
    pub id: String,
    pub batch_id: String,
    pub status: String,
    pub conflict_code: Option<String>,
    pub target_id: Option<String>,
    pub source_sha256: String,
    pub stage_relative: Option<String>,
    pub relative_path: String,
}

fn unsupported() -> String {
    "学习资料迁入目前仅支持 Windows 来源目录绑定".into()
}

#[tauri::command]
pub async fn select_learning_course_source(
    _app: AppHandle,
    _expected_library_id: String,
) -> Result<Option<SqliteSourceBinding>, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn current_learning_course_source(
    _app: AppHandle,
    _expected_library_id: String,
) -> Result<Option<SqliteSourceBinding>, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn release_learning_course_source(
    _app: AppHandle,
    _expected_library_id: String,
    _selection_id: String,
) -> Result<bool, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn bind_learning_import_source(
    _app: AppHandle,
    _expected_library_id: String,
    _vault: String,
) -> Result<Option<ImportSourceBinding>, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn pick_learning_import_note(
    _app: AppHandle,
    _expected_library_id: String,
    _profile_id: String,
    _epoch: String,
    _vault: String,
    _selected_root: String,
) -> Result<Option<ImportNotePreview>, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn preview_learning_import_note(
    _app: AppHandle,
    _expected_library_id: String,
    _profile_id: String,
    _epoch: String,
    _vault: String,
    _selected_root: String,
    _relative_path: String,
) -> Result<ImportNotePreview, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn pick_learning_import_notes(
    _app: AppHandle,
    _expected_library_id: String,
    _profile_id: String,
    _epoch: String,
    _vault: String,
    _selected_root: String,
) -> Result<Option<Vec<ImportNotePreview>>, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn queue_learning_import_prepare(
    _app: AppHandle,
    _state: State<'_, AppState>,
    _expected_library_id: String,
    _profile_id: String,
    _epoch: String,
    _vault: String,
    _selected_root: String,
    _relative_path: String,
    _expected_source_sha256: String,
    _expected_selection_id: String,
    _expected_target_token: String,
) -> Result<LibraryTask, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn queue_learning_import_prepare_batch(
    _app: AppHandle,
    _state: State<'_, AppState>,
    _expected_library_id: String,
    _profile_id: String,
    _epoch: String,
    _vault: String,
    _selected_root: String,
    _notes: Vec<ImportPrepareNote>,
    _expected_selection_id: String,
) -> Result<LibraryTask, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn list_learning_import_batches(
    _app: AppHandle,
    _expected_library_id: String,
) -> Result<Vec<ImportBatchView>, String> {
    Err(unsupported())
}

#[tauri::command]
pub async fn list_learning_import_items(
    _app: AppHandle,
    _expected_library_id: String,
    _batch_id: String,
) -> Result<Vec<ImportItemView>, String> {
    Err(unsupported())
}
