//! Scoped, UUID-only excerpt reads and recovery. Never accepts a source path.
use super::{
    excerpt_publish::{CommitRequest, ExcerptReceipt},
    excerpt_store::StoredExcerpt,
};
use crate::{
    models::{Page, ProjectSummary, ReferenceBoardSummary},
    state::AppState,
};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredOperation {
    pub operation_id: String,
    pub excerpt_id: String,
    pub status: String,
    pub request: CommitRequest,
    pub receipt: Option<ExcerptReceipt>,
    pub created_at: String,
    pub preview_readable: bool,
    pub warning: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationSummary {
    pub operation_id: String,
    pub excerpt_id: String,
    pub status: String,
    pub title: String,
    pub receipt: Option<ExcerptReceipt>,
    pub created_at: String,
    pub error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcerptTargets {
    pub projects: Vec<ProjectSummary>,
    pub boards: Vec<ReferenceBoardSummary>,
}
pub fn get_in_library(state: &AppState, library: &str, id: &str) -> Result<StoredExcerpt, String> {
    uuid(id)?;
    scoped(state, library, |db, base, _| {
        super::excerpt_store::get_preview(db, base, library, id)
    })
}
pub fn operation_in_library(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<StoredOperation, String> {
    uuid(id)?;
    scoped(state, library, |db, base, _| {
        let mut record = read_record(db, id)?;
        match super::excerpt_store::get_preview(db, base, library, &record.excerpt_id) {
            Ok(_) => record.preview_readable = true,
            Err(error) => {
                record.warning = Some(if record.receipt.is_some() {
                    format!("来源预览不可读取，历史回执仍保留：{error}")
                } else {
                    format!("未提交操作的预览不可读取，记录保留：{error}")
                })
            }
        };
        Ok(record)
    })
}
pub fn list_in_library(
    state: &AppState,
    library: &str,
    course: &str,
    offset: i64,
    limit: i64,
) -> Result<Page<OperationSummary>, String> {
    uuid(course)?;
    if offset < 0 || !(1..=50).contains(&limit) {
        return Err("摘录分页无效，每页最多50条".into());
    }
    scoped(state, library, |db, _, _| {
        let total=db.query_row("SELECT count(*) FROM learning_excerpt_operations o JOIN learning_excerpts e ON e.id=o.excerpt_id WHERE e.course_id=?1",[course],|r|r.get(0)).map_err(error)?;
        let mut stmt=db.prepare("SELECT o.operation_id,o.excerpt_id,substr(o.created_at,1,128) FROM learning_excerpt_operations o JOIN learning_excerpts e ON e.id=o.excerpt_id WHERE e.course_id=?1 ORDER BY o.created_at DESC,o.operation_id DESC LIMIT ?2 OFFSET ?3").map_err(error)?;
        let rows = stmt
            .query_map(params![course, limit, offset], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        let items = rows
            .into_iter()
            .map(|(id, excerpt, created)| match read_record(db, &id) {
                Ok(record) => OperationSummary {
                    operation_id: id,
                    excerpt_id: excerpt,
                    status: record.status,
                    title: title(db, &record.request),
                    receipt: record.receipt,
                    created_at: created,
                    error: None,
                },
                Err(error) => OperationSummary {
                    operation_id: id,
                    excerpt_id: excerpt,
                    status: "unreadable".into(),
                    title: "记录需要检查".into(),
                    receipt: None,
                    created_at: created,
                    error: Some(error),
                },
            })
            .collect();
        Ok(Page {
            items,
            total,
            offset,
            limit,
        })
    })
}
pub fn cancel_preview_in_library(state: &AppState, library: &str, id: &str) -> Result<(), String> {
    uuid(id)?;
    scoped(state, library, |_, base, path| {
        let db = crate::tasks::open_media_worker_connection(path)?;
        db.busy_timeout(std::time::Duration::ZERO).map_err(error)?;
        super::excerpt_store::cancel_preview(&db, base, library, id).map(|_| ())
    })
}
pub fn cancel_operation_in_library(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<(), String> {
    let record = operation_in_library(state, library, id)?;
    super::excerpt_publish::cancel_in_library(state, library, &record.request)
}
pub fn targets_in_library(state: &AppState, library: &str) -> Result<ExcerptTargets, String> {
    scoped(state, library, |db, _, _| {
        Ok(ExcerptTargets {
            projects: crate::projects::list_projects(db, false)?,
            boards: crate::reference_boards::list(db)?,
        })
    })
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn uuid(id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id) {
        Ok(())
    } else {
        Err("摘录、操作或课程ID必须为规范UUID".into())
    }
}
fn scoped<T>(
    state: &AppState,
    library: &str,
    read: impl FnOnce(&Connection, &Path, &Path) -> Result<T, String>,
) -> Result<T, String> {
    uuid(library)?;
    state.with_active_mut(|active| {
        if active.marker.library_id != library
            || super::analysis_store::library_identity(&active.base_dir)?.0 != library
        {
            return Err("素材库已切换，未读取或更改其他库".into());
        }
        read(
            active.connection.as_ref().ok_or("数据库正在维护")?,
            &active.base_dir,
            &active.db_path,
        )
    })
}
fn read_record(db: &Connection, id: &str) -> Result<StoredOperation, String> {
    uuid(id)?;
    let (excerpt,raw,created):(String,String,String)=db.query_row("SELECT excerpt_id,request_json,substr(created_at,1,128) FROM learning_excerpt_operations WHERE operation_id=?1 AND length(CAST(request_json AS BLOB))<=65536",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"摘录操作不存在或记录超限")?;
    let request: CommitRequest =
        serde_json::from_str(&raw).map_err(|_| "摘录确认请求损坏，未执行清理或重写")?;
    let canonical = super::excerpt_publish::validate_request(&request)?;
    if request.operation_id != id || request.excerpt_id != excerpt {
        return Err("摘录操作身份不一致，记录保留".into());
    }
    let (status, receipt) = super::excerpt_publish::operation(db, &request, &canonical)?
        .ok_or("摘录操作状态缺失或回执超限")?;
    Ok(StoredOperation {
        operation_id: id.into(),
        excerpt_id: excerpt,
        status,
        request,
        receipt,
        created_at: created,
        preview_readable: false,
        warning: None,
    })
}
fn title(db: &Connection, request: &CommitRequest) -> String {
    use super::excerpt_publish::Destination;
    match &request.destination {
        Destination::New { title, .. } | Destination::Project { title, .. } => title.clone(),
        Destination::Append { note_id, .. } => db
            .query_row(
                "SELECT title FROM knowledge_notes WHERE id=?1",
                [note_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "已有笔记（名称暂不可读取）".into()),
        Destination::Board { board_id, .. } => db
            .query_row(
                "SELECT name FROM reference_boards WHERE id=?1",
                [board_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "参考板（名称暂不可读取）".into()),
    }
}
async fn job<T, F>(app: tauri::AppHandle, library: String, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AppState, &str) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        work(&app.state::<AppState>(), &library)
    })
    .await
    .map_err(|_| "摘录读取或恢复操作中断，请重新核对记录".to_owned())?
}
#[tauri::command]
pub async fn get_learning_excerpt(
    app: tauri::AppHandle,
    expected_library_id: String,
    preview_id: String,
) -> Result<StoredExcerpt, String> {
    job(app, expected_library_id, move |state, library| {
        get_in_library(state, library, &preview_id)
    })
    .await
}
#[tauri::command]
pub async fn get_learning_excerpt_operation(
    app: tauri::AppHandle,
    expected_library_id: String,
    operation_id: String,
) -> Result<StoredOperation, String> {
    job(app, expected_library_id, move |state, library| {
        operation_in_library(state, library, &operation_id)
    })
    .await
}
#[tauri::command]
pub async fn list_learning_excerpt_operations(
    app: tauri::AppHandle,
    expected_library_id: String,
    course_id: String,
    offset: i64,
    limit: i64,
) -> Result<Page<OperationSummary>, String> {
    job(app, expected_library_id, move |state, library| {
        list_in_library(state, library, &course_id, offset, limit)
    })
    .await
}
#[tauri::command]
pub async fn cancel_learning_excerpt_preview(
    app: tauri::AppHandle,
    expected_library_id: String,
    preview_id: String,
) -> Result<(), String> {
    job(app, expected_library_id, move |state, library| {
        cancel_preview_in_library(state, library, &preview_id)
    })
    .await
}
#[tauri::command]
pub async fn cancel_learning_excerpt_operation(
    app: tauri::AppHandle,
    expected_library_id: String,
    operation_id: String,
) -> Result<(), String> {
    job(app, expected_library_id, move |state, library| {
        cancel_operation_in_library(state, library, &operation_id)
    })
    .await
}
#[tauri::command]
pub async fn get_learning_excerpt_targets(
    app: tauri::AppHandle,
    expected_library_id: String,
) -> Result<ExcerptTargets, String> {
    job(app, expected_library_id, targets_in_library).await
}
