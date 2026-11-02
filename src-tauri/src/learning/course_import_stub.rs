//! Course selection, preflight, and commit commands.
//!
//! Arguments are library, selection, explicit media UUIDs, and preview ids.
//! There is no path parameter. The marked synthetic session refuses an unmarked
//! database before SQLite is opened. Windows and non-Windows use this module.
use super::import_access::SqliteSourceBinding;
use super::import_course_preview::CourseMediaBinding;
use crate::state::AppState;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

fn course_binding(
    binding: super::course_source_readonly::MarkedCourseBinding,
) -> SqliteSourceBinding {
    SqliteSourceBinding {
        selection_id: binding.selection_id,
        library_id: binding.library_id,
        profile_id: binding.profile_id,
        epoch: binding.epoch,
        file_name: binding.file_name,
    }
}

async fn job<T, F>(app: AppHandle, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        work(&app.state::<AppState>())
    })
    .await
    .map_err(|_| "学习资料迁入操作中断，请重新核对来源与批次".to_owned())?
}

#[tauri::command]
pub async fn select_learning_course_source(
    app: AppHandle,
    expected_library_id: String,
) -> Result<Option<SqliteSourceBinding>, String> {
    let picker_app = app.clone();
    job(app, move |state| {
        super::course_source_readonly::select_marked_course_with_picker(
            state,
            &expected_library_id,
            || {
                picker_app
                    .dialog()
                    .file()
                    .set_title("选择带合成标记的课程数据库副本")
                    .add_filter("SQLite 数据库", &["sqlite", "sqlite3", "db"])
                    .blocking_pick_file()
                    .and_then(|path| path.into_path().ok())
            },
        )
        .map(|picked| picked.map(course_binding))
    })
    .await
}

#[tauri::command]
pub async fn current_learning_course_source(
    app: AppHandle,
    expected_library_id: String,
) -> Result<Option<SqliteSourceBinding>, String> {
    job(app, move |state| {
        super::course_source_readonly::current_marked_course_source(state, &expected_library_id)
            .map(|picked| picked.map(course_binding))
    })
    .await
}

#[tauri::command]
pub async fn release_learning_course_source(
    app: AppHandle,
    expected_library_id: String,
    selection_id: String,
) -> Result<bool, String> {
    job(app, move |state| {
        super::course_source_readonly::release_synthetic_course_source(
            state,
            &expected_library_id,
            &selection_id,
        )
    })
    .await
}

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
