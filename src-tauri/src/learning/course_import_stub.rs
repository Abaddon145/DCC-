//! Non-Windows course preflight and commit commands.
//!
//! Arguments are library, selection, explicit media UUIDs, and preview ids.
//! There is no path parameter. The marked synthetic session refuses an unmarked
//! database before SQLite is opened.
use super::import_course_preview::CourseMediaBinding;
use crate::state::AppState;
use serde_json::Value;

#[tauri::command]
pub async fn list_learning_course_source_series(
    app: tauri::AppHandle,
    expected_library_id: String,
    selection_id: String,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        super::course_source_readonly::list_marked_course_series(
            &app.state::<AppState>(),
            &expected_library_id,
            &selection_id,
        )
    })
    .await
    .map_err(|_| "读取来源系列已中断".to_string())?
}

#[tauri::command]
pub async fn preview_learning_course_import(
    app: tauri::AppHandle,
    expected_library_id: String,
    selection_id: String,
    bindings: Vec<CourseMediaBinding>,
    series_ids: Option<Vec<String>>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        super::course_source_readonly::approve_marked_course_preflight(
            &app.state::<AppState>(),
            &expected_library_id,
            &selection_id,
            bindings,
            series_ids,
        )
    })
    .await
    .map_err(|_| "课程预检已中断".to_string())?
}

#[tauri::command]
pub fn queue_learning_course_import(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    expected_library_id: String,
    selection_id: String,
    preview_id: String,
) -> Result<crate::tasks::LibraryTask, String> {
    let (task, launch) = super::course_source_readonly::queue_synthetic_course_commit(
        &state,
        &expected_library_id,
        &selection_id,
        &preview_id,
    )?;
    if launch {
        tauri::async_runtime::spawn(crate::tasks::dispatch(app));
    }
    Ok(task)
}
