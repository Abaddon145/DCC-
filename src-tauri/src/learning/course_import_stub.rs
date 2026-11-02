use super::import_course_preview::CourseMediaBinding;
use serde_json::Value;
#[tauri::command]
pub async fn list_learning_course_source_series(
    _app: tauri::AppHandle,
    expected_library_id: String,
    selection_id: String,
) -> Result<Value, String> {
    let _ = (expected_library_id, selection_id);
    Err("课程来源迁入当前仅支持 Windows".into())
}
#[tauri::command]
pub async fn preview_learning_course_import(
    _app: tauri::AppHandle,
    expected_library_id: String,
    selection_id: String,
    bindings: Vec<CourseMediaBinding>,
    series_ids: Option<Vec<String>>,
) -> Result<Value, String> {
    let _ = (expected_library_id, selection_id, bindings, series_ids);
    Err("课程来源迁入当前仅支持 Windows".into())
}
#[tauri::command]
pub fn queue_learning_course_import(
    _app: tauri::AppHandle,
    _state: tauri::State<crate::state::AppState>,
    expected_library_id: String,
    selection_id: String,
    preview_id: String,
) -> Result<crate::tasks::LibraryTask, String> {
    let _ = (expected_library_id, selection_id, preview_id);
    Err("课程来源迁入当前仅支持 Windows".into())
}
