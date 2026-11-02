//! Authorized synthetic course-source reads and one-shot commit approval.
//!
//! Selection, isolated copy, preflight, and commit run only when the sibling marker
//! `dcc-synthetic-course-source.ok` is present. An unmarked file, including a real
//! Yingliu Vault database, is refused before SQLite is opened. This is not a Tauri
//! command and does not accept a path from the frontend.
//!
//! `preview_synthetic_course_source` is a read and is not a commit grant. Only
//! `approve_synthetic_course_preflight` stores the process-local approval that
//! `queue_synthetic_course_commit` can hand to the commitCourse worker.
use super::course_origins::CourseOriginState;
use super::import_course_preview::{
    ConflictKind, CourseImportPreview, CourseMediaBinding, Progress,
};
use super::source_profiles::{self, LibraryScope};
use crate::state::AppState;
use crate::tasks::{LibraryTask, TaskRequest};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

const PREFLIGHT_TTL_SECS: i64 = 900;
const STALE_PREVIEW: &str = "课程预检已失效，请重新预检并确认";
const STALE_SELECTION: &str = "来源选择已变化，请重新选择";
const SOURCE_CHANGED: &str = "课程来源已变化，请重新预检";
const TARGET_CHANGED: &str = "目标课程或媒体绑定已变化，请重新预检";

#[derive(Clone)]
struct SelectedSynthetic {
    scope: LibraryScope,
    profile_id: String,
    epoch: String,
    selection_id: String,
    path: PathBuf,
    digest: [u8; 32],
}

struct ApprovedSynthetic {
    id: String,
    selection_id: String,
    scope: LibraryScope,
    profile_id: String,
    epoch: String,
    path: PathBuf,
    digest: [u8; 32],
    expires: i64,
    decision: super::course_apply::CourseStagePreview,
    bindings: Vec<CourseMediaBinding>,
    task_id: Option<String>,
}

/// In-memory selection and one-shot preflight for marked synthetic course databases.
/// The approval is process-local: it is not written to the library and does not survive restart.
#[derive(Default)]
pub(crate) struct SyntheticCourseSession {
    generation: String,
    selected: Option<SelectedSynthetic>,
    approved: Option<ApprovedSynthetic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SyntheticCourseTicket {
    pub selection_id: String,
    pub profile_id: String,
    pub epoch: String,
}

fn session(state: &AppState) -> Result<std::sync::MutexGuard<'_, SyntheticCourseSession>, String> {
    state
        .synthetic_course_source
        .lock()
        .map_err(|_| "来源会话状态不可用".into())
}

fn matched<'a>(
    current: &'a SyntheticCourseSession,
    scope: &LibraryScope,
    ticket: &str,
) -> Result<&'a SelectedSynthetic, String> {
    let selected = current
        .selected
        .as_ref()
        .ok_or("请重新选择课程来源数据库")?;
    if current.generation != ticket || selected.selection_id != ticket || selected.scope != *scope {
        return Err(STALE_SELECTION.into());
    }
    Ok(selected)
}

fn confirm_epoch(state: &AppState, selected: &SelectedSynthetic) -> Result<(), String> {
    let profile = source_profiles::get(state, &selected.scope, &selected.profile_id)?;
    if profile.access_epoch != selected.epoch {
        return Err("来源状态已变化，请重新选择".into());
    }
    Ok(())
}

fn selected_from_approval(approved: &ApprovedSynthetic) -> SelectedSynthetic {
    SelectedSynthetic {
        scope: approved.scope.clone(),
        profile_id: approved.profile_id.clone(),
        epoch: approved.epoch.clone(),
        selection_id: approved.selection_id.clone(),
        path: approved.path.clone(),
        digest: approved.digest,
    }
}

fn validate_bindings(bindings: &[CourseMediaBinding]) -> Result<(), String> {
    if bindings.len() > 10_000 {
        return Err("课程媒体绑定无效".into());
    }
    let mut seen = HashSet::new();
    for binding in bindings {
        if binding.source_item_id.is_empty()
            || binding.source_item_id.len() > 1024
            || binding.source_item_id.chars().any(char::is_control)
            || !source_profiles::valid_uuid(&binding.media_id)
            || !seen.insert(binding.source_item_id.as_str())
        {
            return Err("课程媒体绑定无效".into());
        }
    }
    Ok(())
}

fn bindings_resolved(
    preview: &CourseImportPreview,
    bindings: &[CourseMediaBinding],
) -> Result<(), String> {
    for binding in bindings {
        let resolved = preview
            .series
            .iter()
            .flat_map(|series| &series.items)
            .find(|item| item.id == binding.source_item_id)
            .and_then(|item| item.media_id.as_deref());
        if resolved != Some(binding.media_id.as_str()) {
            return Err("目标媒体不存在或不是当前库中的视频".into());
        }
    }
    Ok(())
}

fn progress_json(progress: Option<&Progress>) -> Value {
    progress
        .map(|progress| {
            json!({
                "positionMs": progress.position_ms,
                "durationMs": progress.duration_ms,
                "rate": progress.rate,
                "completed": progress.completed,
            })
        })
        .unwrap_or(Value::Null)
}

fn origin_conflict(origin: Option<&CourseOriginState>) -> Option<&'static str> {
    match origin {
        Some(CourseOriginState::SourceChanged(_)) => Some("source_changed"),
        Some(CourseOriginState::TargetChanged(_)) => Some("target_changed"),
        Some(CourseOriginState::BothChanged(_)) => Some("both_changed"),
        Some(CourseOriginState::TargetUnavailable(_)) => Some("target_unavailable"),
        _ => None,
    }
}

fn series_conflicts(series: &super::import_course_preview::SourceSeries) -> Vec<&'static str> {
    let mut codes = origin_conflict(series.origin.as_ref())
        .into_iter()
        .collect::<Vec<_>>();
    if matches!(series.origin, Some(CourseOriginState::Unmapped)) {
        if !super::course_apply::acceptable_series(
            &series.title,
            series.description.as_deref().unwrap_or_default(),
        ) {
            codes.push("invalid_series");
        }
        if !series.items.is_empty() {
            codes.push("membership_may_be_withheld");
        }
    }
    codes
}

fn conflict_name(kind: &ConflictKind) -> &'static str {
    match kind {
        ConflictKind::SharedMedia => "shared_media",
        ConflictKind::RepeatedSeriesMembership => "repeated_membership",
        ConflictKind::DifferentSourceProgress => "different_source_progress",
        ConflictKind::DifferentTargetProgress => "different_target_progress",
    }
}

fn preview_json(
    id: &str,
    selection_id: &str,
    expires: i64,
    preview: &CourseImportPreview,
) -> Value {
    let series = preview
        .series
        .iter()
        .map(|series| {
            let items = series
                .items
                .iter()
                .map(|item| {
                    let mut codes = Vec::new();
                    if item.media_id.is_none() {
                        codes.push("unresolved");
                    }
                    if item.source_progress.is_none() {
                        codes.push("progress_absent");
                    }
                    if let Some(code) = origin_conflict(item.origin.as_ref()) {
                        codes.push(code);
                    }
                    if item.target.as_ref().is_some_and(|target| {
                        item.source_progress
                            .as_ref()
                            .is_some_and(|progress| *progress != target.progress)
                    }) {
                        codes.push("divergent_progress");
                    }
                    for conflict in &preview.conflicts {
                        if conflict.source_item_ids.contains(&item.id) {
                            codes.push(conflict_name(&conflict.kind));
                        }
                    }
                    json!({
                        "id": item.id,
                        "title": item.title,
                        "mediaId": item.media_id,
                        "sourceProgress": progress_json(item.source_progress.as_ref()),
                        "targetProgress": progress_json(item.target.as_ref().map(|target| &target.progress)),
                        "conflictCodes": codes,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "id": series.id,
                "title": series.title,
                "conflictCodes": series_conflicts(series),
                "items": items,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "previewId": id,
        "selectionId": selection_id,
        "expiresAt": expires,
        "sourceFingerprint": preview.source_fingerprint,
        "targetFingerprint": preview.target_fingerprint,
        "series": series,
    })
}

/// Register a marked synthetic database. Does not open it with SQLite.
pub(super) fn select_synthetic_course_source(
    state: &AppState,
    library: &str,
    path: &Path,
) -> Result<SyntheticCourseTicket, String> {
    let mut current = session(state)?;
    let ticket = uuid::Uuid::new_v4().to_string();
    current.generation = ticket.clone();
    current.selected = None;
    current.approved = None;
    let scope = source_profiles::scope(state, library)?;
    let digest = super::course_snapshot::fingerprint_synthetic(path)?;
    let profile = source_profiles::create(state, &scope)?;
    current.selected = Some(SelectedSynthetic {
        scope,
        profile_id: profile.id.clone(),
        epoch: profile.access_epoch.clone(),
        selection_id: ticket.clone(),
        path: path.to_path_buf(),
        digest,
    });
    Ok(SyntheticCourseTicket {
        selection_id: ticket,
        profile_id: profile.id,
        epoch: profile.access_epoch,
    })
}

pub(super) fn release_synthetic_course_source(
    state: &AppState,
    library: &str,
    ticket: &str,
) -> Result<bool, String> {
    let scope = source_profiles::scope(state, library)?;
    let mut current = session(state)?;
    if !current
        .selected
        .as_ref()
        .is_some_and(|selected| selected.selection_id == ticket && selected.scope == scope)
    {
        return Ok(false);
    }
    current.generation = uuid::Uuid::new_v4().to_string();
    current.selected = None;
    current.approved = None;
    Ok(true)
}

/// Isolated snapshot plus course preview. The returned preview is not a commit grant.
pub(super) fn preview_synthetic_course_source(
    state: &AppState,
    library: &str,
    ticket: &str,
) -> Result<super::course_apply::CourseStagePreview, String> {
    let scope = source_profiles::scope(state, library)?;
    let current = session(state)?;
    let selected = matched(&current, &scope, ticket)?.clone();
    confirm_epoch(state, &selected)?;
    let snapshot = super::course_snapshot::open_isolated_readonly(
        &selected.path,
        Duration::from_secs(15),
        || {},
    )?;
    let digest = super::course_snapshot::fingerprint_synthetic(&selected.path)?;
    if digest != selected.digest {
        return Err("合成课程来源无法只读隔离，请重试".into());
    }
    if matched(&current, &scope, ticket).is_err() {
        return Err(STALE_SELECTION.into());
    }
    confirm_epoch(state, &selected)?;
    super::course_apply::preview_course_batch(
        state,
        &selected.scope,
        &selected.profile_id,
        &snapshot,
        &[],
    )
}

/// One-shot, 15-minute, process-local preflight. Copies the marked source, then
/// previews the copy. Does not write courses, batches, or media files.
pub(super) fn approve_synthetic_course_preflight(
    state: &AppState,
    library: &str,
    selection_id: &str,
    bindings: Vec<CourseMediaBinding>,
) -> Result<Value, String> {
    validate_bindings(&bindings)?;
    let scope = source_profiles::scope(state, library)?;
    let mut current = session(state)?;
    let selected = matched(&current, &scope, selection_id)?.clone();
    confirm_epoch(state, &selected)?;
    current.approved = None;
    let snapshot = super::course_snapshot::open_isolated_readonly(
        &selected.path,
        Duration::from_secs(15),
        || {},
    )?;
    let digest = super::course_snapshot::fingerprint_synthetic(&selected.path)?;
    if digest != selected.digest {
        return Err("合成课程来源无法只读隔离，请重试".into());
    }
    if matched(&current, &scope, selection_id).is_err() {
        return Err(STALE_SELECTION.into());
    }
    confirm_epoch(state, &selected)?;
    let decision = super::course_apply::preview_course_batch(
        state,
        &selected.scope,
        &selected.profile_id,
        &snapshot,
        &bindings,
    )?;
    super::course_apply::validate_preview_capacity(decision.preview(), &bindings)?;
    bindings_resolved(decision.preview(), &bindings)?;
    let id = uuid::Uuid::new_v4().to_string();
    let expires = chrono::Utc::now().timestamp() + PREFLIGHT_TTL_SECS;
    let body = preview_json(&id, selection_id, expires, decision.preview());
    current.approved = Some(ApprovedSynthetic {
        id,
        selection_id: selection_id.to_string(),
        scope: selected.scope,
        profile_id: selected.profile_id,
        epoch: selected.epoch,
        path: selected.path,
        digest,
        expires,
        decision,
        bindings,
        task_id: None,
    });
    Ok(body)
}

/// Queue commitCourse for an unexpired preflight. Accepts only selection and preflight UUIDs.
/// A second call while that task is still active returns the same task.
pub(super) fn queue_synthetic_course_commit(
    state: &AppState,
    library: &str,
    selection_id: &str,
    preview_id: &str,
) -> Result<(LibraryTask, bool), String> {
    if !source_profiles::valid_uuid(selection_id) || !source_profiles::valid_uuid(preview_id) {
        return Err("课程来源选择或预检无效".into());
    }
    let scope = source_profiles::scope(state, library)?;
    let mut current = session(state)?;
    let approved = current.approved.as_ref().ok_or(STALE_PREVIEW)?;
    if approved.id != preview_id
        || approved.selection_id != selection_id
        || approved.expires <= chrono::Utc::now().timestamp()
    {
        current.approved = None;
        return Err(STALE_PREVIEW.into());
    }
    if approved.scope != scope {
        current.approved = None;
        return Err("素材库会话已变化，请重新检查来源".into());
    }
    let selected = match matched(&current, &scope, selection_id) {
        Ok(selected) => selected.clone(),
        Err(error) => {
            current.approved = None;
            return Err(error);
        }
    };
    if selected.profile_id != current.approved.as_ref().unwrap().profile_id
        || selected.epoch != current.approved.as_ref().unwrap().epoch
        || selected.digest != current.approved.as_ref().unwrap().digest
    {
        current.approved = None;
        return Err(STALE_SELECTION.into());
    }
    if confirm_epoch(state, &selected).is_err() {
        current.approved = None;
        return Err("来源状态已变化，请重新选择".into());
    }
    if let Some(task_id) = current
        .approved
        .as_ref()
        .and_then(|approved| approved.task_id.clone())
    {
        let task = state.tasks.get(&task_id)?;
        let active = matches!(task.status.as_str(), "queued" | "running") && !task.stop_requested;
        if active {
            return Ok((task, false));
        }
        current.approved = None;
        return Err(STALE_PREVIEW.into());
    }
    let existing_ids = state
        .tasks
        .list(library)?
        .into_iter()
        .map(|task| task.id)
        .collect::<HashSet<_>>();
    let (task, launch) = state.tasks.submit(
        library.to_string(),
        TaskRequest::LearningImport(super::import_runner::ImportTaskRequest::CommitCourse {
            selection_id: selection_id.to_string(),
            preview_id: preview_id.to_string(),
        }),
    )?;
    if existing_ids.contains(&task.id) {
        return Err("已有未绑定预检的课程提交任务，请先停止后再排队".into());
    }
    current.approved.as_mut().ok_or(STALE_PREVIEW)?.task_id = Some(task.id.clone());
    Ok((task, launch))
}

pub(super) fn abandon_synthetic_course_preflight(
    state: &AppState,
    library: &str,
    selection_id: &str,
    preview_id: &str,
    task_id: &str,
) {
    let Ok(scope) = source_profiles::scope(state, library) else {
        return;
    };
    let Ok(mut current) = session(state) else {
        return;
    };
    let matches = current.approved.as_ref().is_some_and(|approved| {
        approved.id == preview_id
            && approved.selection_id == selection_id
            && approved.scope == scope
            && approved.task_id.as_deref() == Some(task_id)
    });
    if matches {
        current.approved = None;
    }
}

pub(super) fn commit_synthetic_course(
    state: &AppState,
    library: &str,
    selection_id: &str,
    preview_id: &str,
    task_id: &str,
    stopped: impl Fn() -> bool,
) -> Result<Value, String> {
    commit_synthetic_course_with(
        state,
        library,
        selection_id,
        preview_id,
        task_id,
        stopped,
        || {},
    )
}

fn commit_synthetic_course_with(
    state: &AppState,
    library: &str,
    selection_id: &str,
    preview_id: &str,
    task_id: &str,
    stopped: impl Fn() -> bool,
    interfere: impl FnOnce(),
) -> Result<Value, String> {
    let scope = source_profiles::scope(state, library)?;
    let mut current = session(state)?;
    let Some(approved) = current.approved.as_ref() else {
        return Err(STALE_PREVIEW.into());
    };
    if approved.id != preview_id
        || approved.selection_id != selection_id
        || approved.task_id.as_deref() != Some(task_id)
    {
        return Err("课程提交只能经预检确认后的任务执行".into());
    }
    if approved.scope != scope || approved.expires <= chrono::Utc::now().timestamp() {
        current.approved = None;
        return Err(STALE_PREVIEW.into());
    }
    if matched(&current, &scope, selection_id).is_err() {
        current.approved = None;
        return Err(STALE_SELECTION.into());
    }
    let Some(approved) = current.approved.take() else {
        return Err(STALE_PREVIEW.into());
    };
    if stopped() {
        return Err("课程迁入任务已停止，请重新预检".into());
    }
    let selected = selected_from_approval(&approved);
    confirm_epoch(state, &selected)?;
    let snapshot = super::course_snapshot::open_isolated_readonly(
        &approved.path,
        Duration::from_secs(15),
        interfere,
    )?;
    let digest = super::course_snapshot::fingerprint_synthetic(&approved.path)?;
    if digest != approved.digest {
        return Err(SOURCE_CHANGED.into());
    }
    confirm_epoch(state, &selected)?;
    if matched(&current, &scope, selection_id)
        .ok()
        .filter(|item| item.digest == approved.digest && item.epoch == approved.epoch)
        .is_none()
    {
        return Err(STALE_SELECTION.into());
    }
    let fresh = super::course_apply::preview_course_batch_selected(
        state,
        &approved.scope,
        &approved.profile_id,
        &snapshot,
        &approved.bindings,
        approved.decision.selected_series(),
    )?;
    if fresh.preview().source_fingerprint != approved.decision.preview().source_fingerprint {
        return Err(SOURCE_CHANGED.into());
    }
    if fresh.preview().target_fingerprint != approved.decision.preview().target_fingerprint {
        return Err(TARGET_CHANGED.into());
    }
    if stopped() {
        return Err("课程迁入任务已停止，请重新预检".into());
    }
    let hash = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&json!([
                "yingliu.synthetic-course-import.v1",
                approved.profile_id,
                approved.epoch,
                fresh.preview().source_fingerprint,
                fresh.preview().target_fingerprint
            ]))
            .map_err(|_| "无法计算迁入身份")?,
        )
    );
    let batch = super::course_batch::create_course_batch(
        state,
        &approved.scope,
        &approved.profile_id,
        &approved.epoch,
        &hash,
    )?;
    super::course_apply::stage_course_batch(
        state,
        &approved.scope,
        &batch,
        &snapshot,
        &approved.bindings,
        &approved.decision,
    )?;
    if stopped() {
        super::course_apply::cancel_course_batch(state, &approved.scope, &batch)?;
        return Err("课程迁入任务已停止，未提交内容已取消".into());
    }
    let receipt = super::course_apply::commit_course_batch(
        state,
        &approved.scope,
        &batch,
        &snapshot,
        &approved.bindings,
        &approved.decision,
    )?;
    Ok(json!({
        "batchId": receipt.batch_id,
        "status": receipt.status,
        "receipt": {
            "coursesCommitted": receipt.courses_committed,
            "courseConflicts": receipt.course_conflicts,
            "seriesCommitted": receipt.series_committed,
            "seriesConflicts": receipt.series_conflicts,
        }
    }))
}

#[cfg(test)]
pub(super) fn expire_synthetic_course_preflight(
    state: &AppState,
    library: &str,
    preview_id: &str,
) -> Result<(), String> {
    let scope = source_profiles::scope(state, library)?;
    let mut current = session(state)?;
    let approved = current.approved.as_mut().ok_or(STALE_PREVIEW)?;
    if approved.id != preview_id || approved.scope != scope {
        return Err(STALE_PREVIEW.into());
    }
    approved.expires = 0;
    Ok(())
}

#[cfg(test)]
pub(super) fn forget_synthetic_course_process_memory(state: &AppState) -> Result<(), String> {
    *session(state)? = SyntheticCourseSession::default();
    Ok(())
}

#[cfg(test)]
pub(super) fn commit_synthetic_course_while_copying(
    state: &AppState,
    library: &str,
    selection_id: &str,
    preview_id: &str,
    task_id: &str,
    stopped: impl Fn() -> bool,
    interfere: impl FnOnce(),
) -> Result<Value, String> {
    commit_synthetic_course_with(
        state,
        library,
        selection_id,
        preview_id,
        task_id,
        stopped,
        interfere,
    )
}
