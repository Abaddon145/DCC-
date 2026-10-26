//! Excerpts use the shared task lifecycle; no separate queue or paid operation.
use super::{excerpt_publish::CommitRequest, excerpts::ExcerptSelection};
use crate::{state::AppState, tasks::LibraryTask};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ExcerptTaskRequest {
    Prepare {
        course_id: String,
        selection: ExcerptSelection,
    },
    Commit {
        request: CommitRequest,
    },
}
impl ExcerptTaskRequest {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Prepare {
                course_id,
                selection,
            } => {
                if !uuid::Uuid::parse_str(course_id).is_ok_and(|id| id.to_string() == *course_id) {
                    return Err("课程标识无效".into());
                }
                super::excerpts::validate_selection(selection)
            }
            Self::Commit { request } => {
                super::excerpt_publish::validate_request(request).map(|_| ())
            }
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Prepare { .. } => "学习摘录准备",
            Self::Commit { .. } => "学习摘录保存",
        }
    }
}
pub(crate) async fn run(
    app: tauri::AppHandle,
    task: LibraryTask,
    request: ExcerptTaskRequest,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let resources = app.path().resource_dir().ok();
        run_local(&state, &task, &request, resources.as_deref())
    })
    .await
    .map_err(|_| "摘录任务中断，请检查持久回执后使用原操作重试".to_owned())?
}
pub(crate) fn run_local(
    state: &AppState,
    task: &LibraryTask,
    request: &ExcerptTaskRequest,
    resources: Option<&Path>,
) -> Result<Value, String> {
    request.validate()?;
    let current = state.tasks.get(&task.id)?;
    if current.library_id != task.library_id
        || !matches!(&current.payload,crate::tasks::TaskRequest::CourseExcerpt(saved) if saved==request)
        || !state.tasks.is_active_task(&task.id)
    {
        return Err("摘录任务已取消、失效或请求不一致".into());
    }
    let stopped = || {
        !state.tasks.is_active_task(&task.id)
            || state.tasks.maintenance().is_some()
            || super::analysis::source_scope_changed(state, &task.library_id, None)
    };
    if stopped() {
        return Err("摘录任务已停止或素材库已切换".into());
    }
    state.tasks.update(
        &task.id,
        0,
        1,
        String::new(),
        "核对来源与处理摘录".into(),
        None,
    );
    match request {
        ExcerptTaskRequest::Prepare {
            course_id,
            selection,
        } => {
            let prepared = super::excerpt_runtime::prepare_in_library(
                state,
                &task.library_id,
                course_id,
                selection,
                resources,
                &stopped,
            )?;
            if stopped() {
                return Err("摘录准备已停止，未保存目标".into());
            }
            let active = state
                .active
                .try_lock()
                .map_err(|_| "素材库繁忙，摘录准备可稍后重试")?;
            if active.marker.library_id != task.library_id
                || active.connection.is_none()
                || super::analysis_store::library_identity(&active.base_dir)?
                    != (prepared.library_id.clone(), prepared.root_hash.clone())
            {
                return Err("素材库或位置已变化，未登记旧摘录".into());
            }
            let db = crate::tasks::open_media_worker_connection(&active.db_path)?;
            db.busy_timeout(std::time::Duration::ZERO)
                .map_err(|e| e.to_string())?;
            let stored = super::excerpt_store::save_prepared(
                &db,
                &active.base_dir,
                &task.library_id,
                &prepared,
            )?;
            let value = serde_json::json!({"previewId":stored.id});
            state.tasks.update(
                &task.id,
                1,
                1,
                prepared.text.title,
                "等待明确确认".into(),
                Some(value.clone()),
            );
            if stopped() {
                let cleanup = super::excerpt_store::cancel_preview(
                    &db,
                    &active.base_dir,
                    &task.library_id,
                    &stored.id,
                );
                return Err(match cleanup {
                    Ok(_) => "摘录准备已停止，未保存目标".into(),
                    Err(reason) => format!("准备已停止；草稿保留待核对：{reason}"),
                });
            }
            Ok(value)
        }
        ExcerptTaskRequest::Commit { request } => {
            let result = super::excerpt_publish::commit_in_library(
                state,
                &task.library_id,
                request,
                &stopped,
            )?;
            let value = serde_json::to_value(result).map_err(|e| e.to_string())?;
            // A late stop cannot undo a durable receipt or delete its target.
            state.tasks.update(
                &task.id,
                1,
                1,
                String::new(),
                "已保存摘录".into(),
                Some(value.clone()),
            );
            Ok(value)
        }
    }
}
