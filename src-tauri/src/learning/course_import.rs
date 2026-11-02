//! Product course migration uses a locked offline copy and one-use in-memory approval.
use super::{
    course_apply, course_snapshot,
    import_access::SqliteAccessGrant,
    import_course_preview::{CourseMediaBinding, Progress},
    import_runner::ImportTaskRequest,
    source_profiles,
};
use crate::{
    state::AppState,
    tasks::{LibraryTask, TaskRequest},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[derive(Clone)]
pub(super) struct ApprovedCourse {
    id: String,
    selection: String,
    expires: i64,
    decision: course_apply::CourseStagePreview,
    bindings: Vec<CourseMediaBinding>,
    task_id: Option<String>,
}
fn selected(state: &AppState, library: &str, ticket: &str) -> Result<SqliteAccessGrant, String> {
    if !source_profiles::valid_uuid(ticket) {
        return Err("课程来源选择无效".into());
    }
    let current = state
        .sqlite_import_selection
        .lock()
        .map_err(|_| "来源会话状态不可用")?;
    let grant = current
        .selected
        .as_ref()
        .ok_or("请重新选择课程来源数据库")?;
    if current.generation != ticket
        || grant.binding.selection_id != ticket
        || grant.binding.library_id != library
    {
        return Err("课程来源选择已变化，请重新选择".into());
    }
    Ok(grant.clone())
}
fn check(state: &AppState, grant: &SqliteAccessGrant, file: &std::fs::File) -> Result<(), String> {
    source_profiles::scoped(state, &grant.scope, |db| {
        super::source_sqlite::check_file_binding_in(
            db,
            &grant.binding.profile_id,
            &grant.binding.epoch,
            file,
        )
    })
}
fn progress(p: Option<&Progress>) -> Value {
    p.map(|p|json!({"positionMs":p.position_ms,"durationMs":p.duration_ms,"rate":p.rate,"completed":p.completed})).unwrap_or(Value::Null)
}
fn origin_conflict(
    origin: Option<&super::course_origins::CourseOriginState>,
) -> Option<&'static str> {
    use super::course_origins::CourseOriginState::*;
    match origin {
        Some(SourceChanged(_)) => Some("source_changed"),
        Some(TargetChanged(_)) => Some("target_changed"),
        Some(BothChanged(_)) => Some("both_changed"),
        Some(TargetUnavailable(_)) => Some("target_unavailable"),
        _ => None,
    }
}
fn series_conflicts(series: &super::import_course_preview::SourceSeries) -> Vec<&'static str> {
    let mut codes = origin_conflict(series.origin.as_ref())
        .into_iter()
        .collect::<Vec<_>>();
    if matches!(
        series.origin,
        Some(super::course_origins::CourseOriginState::Unmapped)
    ) {
        if !course_apply::acceptable_series(
            &series.title,
            series.description.as_deref().unwrap_or_default(),
        ) {
            codes.push("invalid_series")
        }
        if !series.items.is_empty() {
            codes.push("membership_may_be_withheld")
        }
    }
    codes
}
pub(super) fn preview_selected(
    state: &AppState,
    library: &str,
    ticket: &str,
    bindings: Vec<CourseMediaBinding>,
) -> Result<Value, String> {
    preview_selected_subset(state, library, ticket, bindings, None)
}

pub(super) fn preview_selected_subset(
    state: &AppState,
    library: &str,
    ticket: &str,
    bindings: Vec<CourseMediaBinding>,
    series_ids: Option<Vec<String>>,
) -> Result<Value, String> {
    if bindings.len() > 10000
        || bindings
            .iter()
            .any(|b| !source_profiles::valid_uuid(&b.media_id))
    {
        return Err("课程媒体绑定无效".into());
    }
    let _operation = state
        .sqlite_import_gate
        .lock()
        .map_err(|_| "来源会话状态不可用")?;
    let grant = selected(state, library, ticket)?;
    state
        .sqlite_import_selection
        .lock()
        .map_err(|_| "来源会话状态不可用")?
        .approved = None;
    source_profiles::scoped(state, &grant.scope, |_| Ok(()))?;
    let snapshot = course_snapshot::open(&grant.path)?;
    check(state, &grant, &snapshot.database)?;
    let decision = course_apply::preview_course_batch_selected(
        state,
        &grant.scope,
        &grant.binding.profile_id,
        &snapshot.connection,
        &bindings,
        series_ids,
    )?;
    let id = uuid::Uuid::new_v4().to_string();
    let expires = chrono::Utc::now().timestamp() + 900;
    let preview = decision.preview();
    course_apply::validate_preview_capacity(preview, &bindings)?;
    let series=preview.series.iter().map(|series|json!({"id":series.id,"title":series.title,"conflictCodes":series_conflicts(series),"items":series.items.iter().map(|item|{
  let mut codes=Vec::new();if item.media_id.is_none(){codes.push("unresolved")};if item.source_progress.is_none(){codes.push("progress_absent")};
  if let Some(code)=origin_conflict(item.origin.as_ref()){codes.push(code)}
  if item.target.as_ref().is_some_and(|t|item.source_progress.as_ref().is_some_and(|p|*p!=t.progress)){codes.push("divergent_progress")};
  for conflict in &preview.conflicts{if conflict.source_item_ids.contains(&item.id){codes.push(match conflict.kind{super::import_course_preview::ConflictKind::SharedMedia=>"shared_media",super::import_course_preview::ConflictKind::RepeatedSeriesMembership=>"repeated_membership",super::import_course_preview::ConflictKind::DifferentSourceProgress=>"different_source_progress",super::import_course_preview::ConflictKind::DifferentTargetProgress=>"different_target_progress"})}}
  json!({"id":item.id,"title":item.title,"mediaId":item.media_id,"sourceProgress":progress(item.source_progress.as_ref()),"targetProgress":progress(item.target.as_ref().map(|t|&t.progress)),"conflictCodes":codes})
 }).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let result = json!({"previewId":id,"selectionId":ticket,"expiresAt":expires,"sourceFingerprint":preview.source_fingerprint,"targetFingerprint":preview.target_fingerprint,"series":series});
    check(state, &grant, &snapshot.database)?;
    snapshot.recheck()?;
    state
        .sqlite_import_selection
        .lock()
        .map_err(|_| "来源会话状态不可用")?
        .approved = Some(ApprovedCourse {
        id,
        selection: ticket.into(),
        expires,
        decision,
        bindings,
        task_id: None,
    });
    Ok(result)
}
fn approval(state: &AppState, ticket: &str, id: &str) -> Result<ApprovedCourse, String> {
    let current = state
        .sqlite_import_selection
        .lock()
        .map_err(|_| "来源会话状态不可用")?;
    current
        .approved
        .as_ref()
        .filter(|p| {
            p.id == id && p.selection == ticket && p.expires > chrono::Utc::now().timestamp()
        })
        .cloned()
        .ok_or("课程预检已失效，请重新预检并确认".into())
}

pub(super) fn list_series(state: &AppState, library: &str, ticket: &str) -> Result<Value, String> {
    let _operation = state
        .sqlite_import_gate
        .lock()
        .map_err(|_| "来源会话状态不可用")?;
    let grant = selected(state, library, ticket)?;
    let snapshot = course_snapshot::open(&grant.path)?;
    check(state, &grant, &snapshot.database)?;
    super::import_course_preview::check_source_schema(&snapshot.connection)?;
    let mut stmt=snapshot.connection.prepare("SELECT s.id,s.title,count(i.id) FROM main.series s LEFT JOIN main.series_items i ON i.series_id COLLATE BINARY=s.id COLLATE BINARY GROUP BY s.id COLLATE BINARY,s.title,s.sort_order ORDER BY s.sort_order,s.id COLLATE BINARY LIMIT 10001").map_err(|_|"无法读取来源系列")?;
    let mut bytes = 0usize;
    let mut series = Vec::new();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|_| "无法读取来源系列")?;
    for row in rows {
        let (id, title, count) = row.map_err(|_| "无法读取来源系列")?;
        bytes = bytes
            .checked_add(id.len() + title.len())
            .ok_or("来源系列过大")?;
        if id.is_empty()
            || id.len() > 1024
            || count < 0
            || bytes > 2 * 1024 * 1024
            || series.len() >= 10000
        {
            return Err("来源系列过多或格式无效".into());
        }
        series.push(json!({"id":id,"title":title,"courseCount":count}));
    }
    drop(stmt);
    check(state, &grant, &snapshot.database)?;
    snapshot.recheck()?;
    Ok(json!(series))
}
pub(super) fn queue_selected(
    state: &AppState,
    library: &str,
    ticket: &str,
    id: &str,
) -> Result<(LibraryTask, bool), String> {
    let _operation = state
        .sqlite_import_gate
        .lock()
        .map_err(|_| "来源会话状态不可用")?;
    let grant = selected(state, library, ticket)?;
    let approved = approval(state, ticket, id)?;
    source_profiles::scoped(state, &grant.scope, |db| {
        if source_profiles::get_in(db, &grant.binding.profile_id)?.access_epoch
            != grant.binding.epoch
        {
            return Err("来源状态已变化".into());
        }
        if let Some(task) = approved.task_id {
            return Ok((state.tasks.get(&task)?, false));
        }
        let result = state.tasks.submit(
            library.into(),
            TaskRequest::LearningImport(ImportTaskRequest::CommitCourse {
                selection_id: ticket.into(),
                preview_id: id.into(),
            }),
        )?;
        state
            .sqlite_import_selection
            .lock()
            .map_err(|_| "来源会话状态不可用")?
            .approved
            .as_mut()
            .ok_or("课程预检已失效")?
            .task_id = Some(result.0.id.clone());
        Ok(result)
    })
}
pub(super) fn commit_selected(
    state: &AppState,
    library: &str,
    ticket: &str,
    id: &str,
    stopped: impl Fn() -> bool,
) -> Result<Value, String> {
    let _operation = state
        .sqlite_import_gate
        .lock()
        .map_err(|_| "来源会话状态不可用")?;
    let grant = selected(state, library, ticket)?;
    let approved = approval(state, ticket, id)?;
    state
        .sqlite_import_selection
        .lock()
        .map_err(|_| "来源会话状态不可用")?
        .approved = None;
    if stopped() {
        return Err("课程迁入任务已停止，请重新预检".into());
    }
    source_profiles::scoped(state, &grant.scope, |_| Ok(()))?;
    let snapshot = course_snapshot::open(&grant.path)?;
    check(state, &grant, &snapshot.database)?;
    let current = course_apply::preview_course_batch_selected(
        state,
        &grant.scope,
        &grant.binding.profile_id,
        &snapshot.connection,
        &approved.bindings,
        approved.decision.selected_series(),
    )?;
    if current.preview().source_fingerprint != approved.decision.preview().source_fingerprint {
        return Err("课程来源已变化，请重新预检".into());
    }
    if current.preview().target_fingerprint != approved.decision.preview().target_fingerprint {
        return Err("目标课程或媒体绑定已变化，请重新预检".into());
    }
    if stopped() {
        return Err("课程迁入任务已停止，请重新预检".into());
    }
    let hash = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&json!([
                "yingliu.offline-course-import.v1",
                grant.binding.profile_id,
                grant.binding.epoch,
                current.preview().source_fingerprint,
                current.preview().target_fingerprint
            ]))
            .map_err(|_| "无法计算迁入身份")?
        )
    );
    let batch = super::course_batch::create_course_batch(
        state,
        &grant.scope,
        &grant.binding.profile_id,
        &grant.binding.epoch,
        &hash,
    )?;
    course_apply::stage_course_batch(
        state,
        &grant.scope,
        &batch,
        &snapshot.connection,
        &approved.bindings,
        &approved.decision,
    )?;
    if stopped() {
        course_apply::cancel_course_batch(state, &grant.scope, &batch)?;
        return Err("课程迁入任务已停止，未提交内容已取消".into());
    }
    check(state, &grant, &snapshot.database)?;
    snapshot.recheck()?;
    let r = course_apply::commit_course_batch(
        state,
        &grant.scope,
        &batch,
        &snapshot.connection,
        &approved.bindings,
        &approved.decision,
    )?;
    Ok(
        json!({"batchId":r.batch_id,"status":r.status,"receipt":{"coursesCommitted":r.courses_committed,"courseConflicts":r.course_conflicts,"seriesCommitted":r.series_committed,"seriesConflicts":r.series_conflicts}}),
    )
}
#[tauri::command]
pub async fn list_learning_course_source_series(
    app: tauri::AppHandle,
    expected_library_id: String,
    selection_id: String,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        list_series(
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
        preview_selected_subset(
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
) -> Result<LibraryTask, String> {
    let (task, launch) = queue_selected(&state, &expected_library_id, &selection_id, &preview_id)?;
    if launch {
        tauri::async_runtime::spawn(crate::tasks::dispatch(app));
    }
    Ok(task)
}
