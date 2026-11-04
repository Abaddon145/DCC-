use super::native_player::{self, NativeViewport, PlaybackAction, PlaybackSnapshot};
use super::{models::*, store};
use crate::{models::Page, state::AppState};
use tauri::State;
#[tauri::command]
pub fn get_course_analysis_default(
    state: State<AppState>,
    expected_library_id: String,
) -> Result<String, String> {
    state
        .with_library_if_id(&expected_library_id, |db, _| {
            Ok(crate::organization::load_library_preferences(db)?.default_analysis_template_id)
        })?
        .ok_or_else(|| "素材库已切换".into())
}
#[tauri::command]
pub fn list_course_analysis_templates(
    state: State<AppState>,
    expected_library_id: String,
) -> Result<Vec<super::analysis_templates::CustomTemplate>, String> {
    super::analysis_templates::list(&state, &expected_library_id)
}

#[tauri::command]
pub fn save_course_analysis_template(
    state: State<AppState>,
    expected_library_id: String,
    id: Option<String>,
    version: Option<i64>,
    name: String,
    focus: String,
) -> Result<super::analysis_templates::CustomTemplate, String> {
    super::analysis_templates::save(
        &state,
        &expected_library_id,
        id.as_deref(),
        version,
        &name,
        &focus,
    )
}

#[tauri::command]
pub fn delete_course_analysis_template(
    state: State<AppState>,
    expected_library_id: String,
    id: String,
    version: i64,
) -> Result<(), String> {
    super::analysis_templates::remove(&state, &expected_library_id, &id, version)
}
#[tauri::command]
pub async fn prepare_learning_note_jump(
    app: tauri::AppHandle,
    expected_library_id: String,
    reference: super::note_nodes::LearningReference,
    range_index: usize,
) -> Result<super::note_nodes::NoteJump, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        super::note_nodes::prepare_jump(
            &app.state::<AppState>(),
            &expected_library_id,
            &reference,
            range_index,
        )
    })
    .await
    .map_err(|_| "课程来源核对已中断，请重试".to_owned())?
}
async fn component_job<F>(
    app: tauri::AppHandle,
    expected: String,
    work: F,
) -> Result<super::transcription_install::ComponentStatus, String>
where
    F: FnOnce(&std::path::Path) -> Result<(), String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        if state.active_library_id()? != expected {
            return Err("素材库已切换，未处理组件".into());
        }
        let config = state.config_directory();
        work(&config)?;
        let result = super::transcription_install::inspect_components(&config)?;
        if state.active_library_id()? != expected {
            return Err("素材库已切换；组件为全局数据，请在当前库刷新状态".into());
        }
        Ok(result)
    })
    .await
    .map_err(|_| "语音组件处理已中断，请重新检查状态".to_owned())?
}
#[tauri::command]
pub async fn get_learning_transcription_components(
    app: tauri::AppHandle,
    expected_library_id: String,
) -> Result<super::transcription_install::ComponentStatus, String> {
    component_job(app, expected_library_id, |_| Ok(())).await
}
#[tauri::command]
pub async fn import_learning_transcription_model(
    app: tauri::AppHandle,
    expected_library_id: String,
    model_id: String,
    selected_path: String,
) -> Result<super::transcription_install::ComponentStatus, String> {
    component_job(app, expected_library_id, move |config| {
        super::transcription_install::import_model(
            config,
            &model_id,
            std::path::Path::new(&selected_path),
        )
    })
    .await
}
#[tauri::command]
pub async fn install_learning_transcription_tools(
    app: tauri::AppHandle,
    expected_library_id: String,
    selected_resource_root: String,
) -> Result<super::transcription_install::ComponentStatus, String> {
    component_job(app, expected_library_id, move |config| {
        super::transcription_install::install_tools(
            config,
            std::path::Path::new(&selected_resource_root),
        )
    })
    .await
}
#[tauri::command]
pub fn list_course_analyses(
    state: State<AppState>,
    expected_library_id: String,
    course_id: String,
    offset: i64,
    limit: i64,
) -> Result<Page<super::analysis_models::CourseAnalysisHistoryItem>, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        super::analysis_store::list_history(db, &course_id, offset, limit)
    }))
}
#[tauri::command]
pub async fn prepare_course_summary(
    app: tauri::AppHandle,
    expected_library_id: String,
    course_id: String,
    parent_analysis_id: String,
) -> Result<super::analysis_models::CourseAnalysisPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        super::analysis::prepare_summary(
            &app.state::<AppState>(),
            &expected_library_id,
            &course_id,
            &parent_analysis_id,
        )
    })
    .await
    .map_err(|_| "课程汇总预览准备中断，请重试".to_string())?
}
#[tauri::command]
pub async fn get_course_analysis(
    app: tauri::AppHandle,
    expected_library_id: String,
    course_id: String,
    analysis_id: String,
    stage_key: Option<String>,
    offset: i64,
    limit: i64,
) -> Result<super::analysis_models::CourseAnalysisDetail, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        super::analysis::get(
            &app.state::<AppState>(),
            &expected_library_id,
            &course_id,
            &analysis_id,
            stage_key.as_deref(),
            offset,
            limit,
        )
    })
    .await
    .map_err(|_| "课程分析历史读取中断".to_string())?
}
#[tauri::command]
pub async fn prepare_course_analysis(
    app: tauri::AppHandle,
    expected_library_id: String,
    course_id: String,
    options: super::analysis_models::CourseAnalysisOptions,
) -> Result<super::analysis_models::CourseAnalysisPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let resources = app.path().resource_dir().ok();
        super::analysis::prepare(
            &state,
            &expected_library_id,
            &course_id,
            &options,
            resources.as_deref(),
        )
    })
    .await
    .map_err(|_| "课程预览准备中断，请重试".to_string())?
}
#[tauri::command]
pub async fn start_learning_playback(
    app: tauri::AppHandle,
    expected_library_id: String,
    course_id: String,
    expected_fingerprint: Option<String>,
) -> Result<PlaybackSnapshot, String> {
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || {
        native_player::start_playback_with_fingerprint(
            &app,
            &expected_library_id,
            &course_id,
            expected_fingerprint,
        )
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    Err("当前平台不支持原生学习播放".into())
}
#[tauri::command]
pub async fn control_learning_playback(
    app: tauri::AppHandle,
    expected_library_id: String,
    session_id: String,
    action: PlaybackAction,
) -> Result<PlaybackSnapshot, String> {
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || {
        native_player::control(&app, &expected_library_id, &session_id, action)
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    Err("当前平台不支持原生学习播放".into())
}
#[tauri::command]
pub async fn set_learning_viewport(
    app: tauri::AppHandle,
    expected_library_id: String,
    session_id: String,
    viewport: NativeViewport,
) -> Result<(), String> {
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        native_player::set_viewport(
            &app.state::<AppState>(),
            &expected_library_id,
            &session_id,
            viewport,
        )
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    Err("当前平台不支持原生学习播放".into())
}
#[tauri::command]
pub async fn stop_learning_playback(
    app: tauri::AppHandle,
    expected_library_id: String,
    session_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        native_player::stop_learning_session(
            &app.state::<AppState>(),
            &expected_library_id,
            &session_id,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn get_learning_playback(
    state: State<AppState>,
    expected_library_id: String,
    session_id: String,
) -> Result<PlaybackSnapshot, String> {
    native_player::snapshot(&state, &expected_library_id, &session_id)
}
fn current<T>(result: Result<Option<T>, String>) -> Result<T, String> {
    result?.ok_or_else(|| "素材库已切换，未读取或修改旧库".into())
}
#[tauri::command]
pub async fn import_learning_subtitle(
    app: tauri::AppHandle,
    expected_library_id: String,
    course_id: String,
    selected_path: String,
) -> Result<LearningSubtitle, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        current(
            app.state::<AppState>()
                .with_library_if_id(&expected_library_id, |db, base| {
                    super::subtitles::import(
                        db,
                        base,
                        &course_id,
                        std::path::Path::new(&selected_path),
                    )
                }),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn list_learning_subtitles(
    state: State<AppState>,
    expected_library_id: String,
    course_id: String,
) -> Result<Vec<LearningSubtitle>, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        super::subtitles::list(db, &course_id)
    }))
}
#[tauri::command]
pub fn transcript_page(
    state: State<AppState>,
    expected_library_id: String,
    course_id: String,
    subtitle_id: String,
    offset: i64,
    limit: i64,
) -> Result<Page<TranscriptCue>, String> {
    current(state.with_library_if_id(&expected_library_id, |db, base| {
        super::subtitles::page(db, base, &course_id, &subtitle_id, offset, limit)
    }))
}
#[tauri::command]
pub async fn load_learning_subtitle(
    app: tauri::AppHandle,
    expected_library_id: String,
    session_id: String,
    subtitle_id: String,
) -> Result<(), String> {
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || {
        native_player::load_subtitle(&app, &expected_library_id, &session_id, &subtitle_id)
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    Err("当前平台不支持原生字幕".into())
}
#[tauri::command]
pub fn list_learning_unassigned_videos(
    state: State<AppState>,
    expected_library_id: String,
    offset: i64,
    limit: i64,
) -> Result<Page<super::membership::UnassignedLearningVideo>, String> {
    super::membership::list_unassigned_for_library(&state, &expected_library_id, offset, limit)
}
#[tauri::command]
pub fn list_learning_membership_preview(
    state: State<AppState>,
    expected_library_id: String,
    offset: i64,
    limit: i64,
) -> Result<Page<super::membership::MembershipPreview>, String> {
    super::membership::list_for_library(&state, &expected_library_id, offset, limit)
}
#[tauri::command]
pub fn apply_learning_membership_changes(
    state: State<AppState>,
    expected_library_id: String,
    changes: Vec<super::membership::MembershipChange>,
) -> Result<(), String> {
    super::membership::apply_for_library(&state, &expected_library_id, &changes)
}
#[tauri::command]
pub fn ensure_learning_course(
    state: State<AppState>,
    expected_library_id: String,
    media_id: String,
) -> Result<Course, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        store::ensure_course(db, &media_id)
    }))
}
#[tauri::command]
pub fn get_learning_course(
    state: State<AppState>,
    expected_library_id: String,
    id: String,
) -> Result<Course, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| store::course(db, &id)))
}
#[tauri::command]
pub fn list_learning_courses(
    state: State<AppState>,
    expected_library_id: String,
    offset: i64,
    limit: i64,
) -> Result<Page<Course>, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        store::list_courses(db, offset, limit)
    }))
}
#[tauri::command]
pub fn search_learning_courses(
    state: State<AppState>,
    expected_library_id: String,
    query: String,
    series_id: Option<String>,
    offset: i64,
    limit: i64,
) -> Result<Page<Course>, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        store::search_courses(db, &query, series_id.as_deref(), offset, limit)
    }))
}
#[tauri::command]
pub fn list_learning_series(
    state: State<AppState>,
    expected_library_id: String,
) -> Result<Vec<Series>, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| store::list_series(db)))
}
#[tauri::command]
pub fn list_learning_categories(
    state: State<AppState>,
    expected_library_id: String,
) -> Result<Vec<LearningCategory>, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        store::list_learning_categories(db)
    }))
}
#[tauri::command]
pub fn create_learning_category(
    state: State<AppState>,
    expected_library_id: String,
    name: String,
) -> Result<LearningCategory, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        store::create_learning_category(db, &name)
    }))
}
#[tauri::command]
pub fn rename_learning_category(
    state: State<AppState>,
    expected_library_id: String,
    id: String,
    revision: i64,
    name: String,
) -> Result<LearningCategory, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        store::rename_learning_category(db, &id, revision, &name)
    }))
}
#[tauri::command]
pub fn delete_learning_category(
    state: State<AppState>,
    expected_library_id: String,
    id: String,
    revision: i64,
) -> Result<(), String> {
    current(state.with_library_mut_if_id(&expected_library_id, |db, _| {
        store::delete_learning_category(db, &id, revision)
    }))
}
#[tauri::command]
pub fn assign_learning_series_category(
    state: State<AppState>,
    expected_library_id: String,
    id: String,
    revision: i64,
    category_id: Option<String>,
) -> Result<Series, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        store::assign_series_category(db, &id, revision, category_id.as_deref())
    }))
}
#[tauri::command]
pub fn upsert_learning_series(
    state: State<AppState>,
    expected_library_id: String,
    input: SeriesInput,
) -> Result<Series, String> {
    current(state.with_library_mut_if_id(&expected_library_id, |db, _| {
        store::upsert_series(
            db,
            input.id.as_deref(),
            &input.name,
            &input.description,
            input.expected_revision,
        )
    }))
}
#[tauri::command]
pub fn set_learning_series_members(
    state: State<AppState>,
    expected_library_id: String,
    id: String,
    expected_revision: i64,
    course_ids: Vec<String>,
) -> Result<Series, String> {
    current(state.with_library_mut_if_id(&expected_library_id, |db, _| {
        store::set_series_members(db, &id, expected_revision, &course_ids)
    }))
}
#[tauri::command]
pub fn delete_learning_series(
    state: State<AppState>,
    expected_library_id: String,
    id: String,
    expected_revision: i64,
) -> Result<(), String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        store::delete_series(db, &id, expected_revision)
    }))
}

#[cfg(test)]
mod learning_store_commands_tests {
    use super::*;
    #[test]
    fn learning_store_scope_never_falls_back_to_current_library() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(dir.path());
        let foreign = uuid::Uuid::new_v4().to_string();
        let result = current(state.with_library_mut_if_id(&foreign, |db, _| {
            store::upsert_series(db, None, "foreign", "", None)
        }));
        assert!(result.unwrap_err().contains("素材库已切换"));
        assert_eq!(
            state
                .with_library(|db, _| Ok(store::list_series(db)?.len()))
                .unwrap(),
            0
        );
    }
}
