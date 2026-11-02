//! Learning import uses the shared task lifecycle; no separate queue or product UI.
use super::{import_ops, source_profiles};
use crate::{state::AppState, tasks::LibraryTask};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub(crate) const MAX_PREPARE_NOTES: usize = 32;
pub(crate) const MAX_PREPARE_MARKDOWN_BYTES: usize = 5 * 1024 * 1024;
pub(crate) const MAX_PREPARE_BATCH_BYTES: usize = 20 * 1024 * 1024;

/// One verified Markdown note inside a prepare task.
/// A single-note prepare leaves `items` empty and uses the legacy fields so older
/// queued tasks still deserialize. A multi-note prepare puts every note here and
/// leaves the legacy fields blank.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareMarkdownItem {
    pub relative_path: String,
    pub source_sha256: String,
    pub title: String,
    pub document: Value,
    /// Verified UTF-8 markdown already authorized by the caller; must match source_sha256.
    pub markdown: String,
}

/// Caller-supplied identity for one note in a batch prepare. The backend re-reads
/// the authorized vault and ignores any markdown body from the frontend.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct ImportPrepareNote {
    pub relative_path: String,
    pub source_sha256: String,
    pub target_token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ImportTaskRequest {
    Prepare {
        profile_id: String,
        epoch: String,
        request_sha256: String,
        vault: String,
        relative_path: String,
        source_sha256: String,
        title: String,
        document: Value,
        /// Verified UTF-8 markdown already authorized by the caller; must match source_sha256.
        markdown: String,
        /// Empty for the single-note shape. Non-empty replaces the legacy fields.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        items: Vec<PrepareMarkdownItem>,
    },
    Commit {
        batch_id: String,
    },
    Cancel {
        batch_id: String,
    },
    Revert {
        batch_id: String,
        expected_epoch: String,
    },
    CancelCourse {
        batch_id: String,
    },
    CommitCourse {
        selection_id: String,
        preview_id: String,
    },
    RevertCourse {
        batch_id: String,
        expected_epoch: String,
    },
}

impl ImportTaskRequest {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::CommitCourse {
                selection_id,
                preview_id,
            } => {
                if !source_profiles::valid_uuid(selection_id)
                    || !source_profiles::valid_uuid(preview_id)
                {
                    return Err("课程来源选择或预检无效".into());
                }
                Ok(())
            }
            Self::Prepare {
                profile_id,
                epoch,
                request_sha256,
                vault,
                ..
            } => {
                let notes = prepare_entries(self)?;
                if !source_profiles::valid_uuid(profile_id) || !source_profiles::valid_uuid(epoch) {
                    return Err("来源身份无效".into());
                }
                if !valid_hash(request_sha256) {
                    return Err("迁入摘要无效".into());
                }
                validate_prepare_notes(profile_id, vault, &notes)?;
                Ok(())
            }
            Self::Commit { batch_id }
            | Self::Cancel { batch_id }
            | Self::CancelCourse { batch_id } => {
                if !source_profiles::valid_uuid(batch_id) {
                    return Err("迁入批次无效".into());
                }
                Ok(())
            }
            Self::Revert {
                batch_id,
                expected_epoch,
            }
            | Self::RevertCourse {
                batch_id,
                expected_epoch,
            } => {
                if !source_profiles::valid_uuid(batch_id)
                    || !source_profiles::valid_uuid(expected_epoch)
                {
                    return Err("迁入批次或来源状态无效".into());
                }
                Ok(())
            }
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Prepare { .. } => "学习资料迁入准备",
            Self::Commit { .. } => "学习资料迁入提交",
            Self::Cancel { .. } => "学习资料迁入取消",
            Self::Revert { .. } => "学习资料迁入撤回",
            Self::CancelCourse { .. } => "课程迁入取消",
            Self::CommitCourse { .. } => "课程迁入提交",
            Self::RevertCourse { .. } => "课程迁入撤回",
        }
    }
}

fn prepare_entries(request: &ImportTaskRequest) -> Result<Vec<PrepareMarkdownItem>, String> {
    let ImportTaskRequest::Prepare {
        items,
        relative_path,
        source_sha256,
        title,
        document,
        markdown,
        ..
    } = request
    else {
        return Err("迁入任务类型无效".into());
    };
    if items.is_empty() {
        return Ok(vec![PrepareMarkdownItem {
            relative_path: relative_path.clone(),
            source_sha256: source_sha256.clone(),
            title: title.clone(),
            document: document.clone(),
            markdown: markdown.clone(),
        }]);
    }
    if !relative_path.is_empty()
        || !source_sha256.is_empty()
        || !title.is_empty()
        || !markdown.is_empty()
        || !document.is_null()
    {
        return Err("迁入准备不能同时携带单篇与多篇正文".into());
    }
    Ok(items.clone())
}

fn validate_prepare_notes(
    profile_id: &str,
    vault: &str,
    notes: &[PrepareMarkdownItem],
) -> Result<(), String> {
    if notes.is_empty() || notes.len() > MAX_PREPARE_NOTES {
        return Err("迁入批次笔记数量无效".into());
    }
    let mut seen = HashSet::new();
    let mut total = 0usize;
    for note in notes {
        if !valid_hash(&note.source_sha256) {
            return Err("迁入摘要无效".into());
        }
        if note.title.trim().is_empty() || note.title.len() > 512 {
            return Err("迁入笔记标题无效".into());
        }
        if note.markdown.is_empty() || note.markdown.len() > MAX_PREPARE_MARKDOWN_BYTES {
            return Err("迁入原文超限".into());
        }
        let digest = format!("{:x}", Sha256::digest(note.markdown.as_bytes()));
        if digest != note.source_sha256 {
            return Err("迁入原文摘要不匹配".into());
        }
        let _ = super::note_origins::NoteOriginKey::new(profile_id, vault, &note.relative_path)?;
        if !seen.insert(note.relative_path.to_lowercase()) {
            return Err("迁入批次包含重复笔记".into());
        }
        total = total.saturating_add(note.markdown.len());
        if total > MAX_PREPARE_BATCH_BYTES {
            return Err("迁入批次原文超限".into());
        }
    }
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) async fn run(
    app: tauri::AppHandle,
    task: LibraryTask,
    request: ImportTaskRequest,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        run_local(&state, &task, &request)
    })
    .await
    .map_err(|_| "迁入任务中断，请检查批次状态后重试".to_owned())?
}

pub(crate) fn run_local(
    state: &AppState,
    task: &LibraryTask,
    request: &ImportTaskRequest,
) -> Result<Value, String> {
    request.validate()?;
    let current = state.tasks.get(&task.id)?;
    if current.library_id != task.library_id
        || !matches!(
            &current.payload,
            crate::tasks::TaskRequest::LearningImport(saved) if saved == request
        )
        || !state.tasks.is_active_task(&task.id)
    {
        return Err("迁入任务已取消、失效或请求不一致".into());
    }
    let stopped = || !state.tasks.is_active_task(&task.id) || state.tasks.maintenance().is_some();
    if stopped() {
        return Err("迁入任务已停止或素材库正在维护".into());
    }
    let scope = source_profiles::scope(state, &task.library_id)?;
    state
        .tasks
        .update(&task.id, 0, 1, String::new(), "处理迁入批次".into(), None);
    match request {
        ImportTaskRequest::CommitCourse {
            selection_id,
            preview_id,
        } => {
            #[cfg(windows)]
            {
                let value = super::course_import::commit_selected(
                    state,
                    &task.library_id,
                    selection_id,
                    preview_id,
                    stopped,
                )?;
                state.tasks.update(
                    &task.id,
                    1,
                    1,
                    String::new(),
                    "课程迁入提交已完成".into(),
                    Some(value.clone()),
                );
                Ok(value)
            }
            #[cfg(not(windows))]
            {
                let _ = (selection_id, preview_id);
                Err("课程来源迁入当前仅支持 Windows".into())
            }
        }
        ImportTaskRequest::Prepare {
            profile_id,
            epoch,
            request_sha256,
            vault,
            ..
        } => {
            if stopped() {
                return Err("迁入准备已停止".into());
            }
            let notes = prepare_entries(request)?;
            let batch = import_ops::create_batch(state, &scope, profile_id, epoch, request_sha256)?;
            if batch.status == "committed" {
                // Same request replayed after commit: hand back the durable receipt instead
                // of adding or staging anything new.
                let value = json!({
                    "batchId": batch.id,
                    "status": batch.status,
                    "receipt": batch.receipt_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
                });
                state.tasks.update(
                    &task.id,
                    1,
                    1,
                    notes
                        .first()
                        .map(|note| note.title.clone())
                        .unwrap_or_default(),
                    "迁入已提交（重放）".into(),
                    Some(value.clone()),
                );
                return Ok(value);
            }
            if batch.status != "pending" {
                return Err("迁入批次已结束，请使用新的请求摘要".into());
            }
            let mut item_ids = Vec::with_capacity(notes.len());
            for note in &notes {
                if stopped() {
                    let _ = import_ops::cancel_batch(state, &scope, &batch.id);
                    return Err("迁入准备已停止，已取消未提交批次".into());
                }
                let item = import_ops::add_item(
                    state,
                    &scope,
                    &batch.id,
                    import_ops::NewImportItem {
                        vault: vault.clone(),
                        relative_path: note.relative_path.clone(),
                        source_sha256: note.source_sha256.clone(),
                        title: note.title.clone(),
                        document: note.document.clone(),
                        expected_target_id: None,
                        expected_target_revision: None,
                        expected_target_sha256: None,
                    },
                )?;
                let staged = import_ops::stage_item_bytes(
                    state,
                    &scope,
                    &batch.id,
                    &item.id,
                    note.markdown.as_bytes(),
                )?;
                item_ids.push(staged.id);
            }
            if stopped() {
                let _ = import_ops::cancel_batch(state, &scope, &batch.id);
                return Err("迁入准备已停止，已取消未提交批次".into());
            }
            let value = json!({
                "batchId": batch.id,
                "itemId": item_ids.first(),
                "itemIds": item_ids,
                "status": "staged",
                "count": notes.len(),
            });
            let label = if notes.len() == 1 {
                notes[0].title.clone()
            } else {
                format!("{} 篇笔记", notes.len())
            };
            state.tasks.update(
                &task.id,
                notes.len(),
                notes.len(),
                label,
                "等待明确提交".into(),
                Some(value.clone()),
            );
            Ok(value)
        }
        ImportTaskRequest::Commit { batch_id } => {
            let batch = import_ops::commit_batch(state, &scope, batch_id)?;
            let phase = match batch.status.as_str() {
                "committed" => "迁入已提交",
                "cancelled" => "批次已取消，未提交的条目未写入，已提交内容保留",
                _ => return Err("无法确认迁入批次状态".into()),
            };
            let value = json!({
                "batchId": batch.id,
                "status": batch.status,
                "receipt": batch.receipt_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
            });
            // Late stop cannot undo a durable commit receipt.
            state.tasks.update(
                &task.id,
                1,
                1,
                String::new(),
                phase.into(),
                Some(value.clone()),
            );
            Ok(value)
        }
        ImportTaskRequest::Cancel { batch_id } => {
            let batch = import_ops::cancel_batch(state, &scope, batch_id)?;
            let phase = match batch.status.as_str() {
                "cancelled" => "迁入已取消",
                "committed" => "批次已提交，无法取消；已提交内容保留",
                _ => return Err("无法确认迁入批次状态".into()),
            };
            let value = json!({
                "batchId": batch.id,
                "status": batch.status,
            });
            state.tasks.update(
                &task.id,
                1,
                1,
                String::new(),
                phase.into(),
                Some(value.clone()),
            );
            Ok(value)
        }
        ImportTaskRequest::Revert {
            batch_id,
            expected_epoch,
        } => {
            let report = import_ops::revert_batch(state, &scope, expected_epoch, batch_id)?;
            if report["batchId"].as_str() != Some(batch_id.as_str())
                || !report["reverted"].is_array()
                || !report["kept"].is_array()
                || report["archivesKept"] != true
            {
                return Err("迁入撤回记录损坏，请检查批次".into());
            }
            let value = json!({"batchId": batch_id, "status": "reverted", "report": report});
            state.tasks.update(
                &task.id,
                1,
                1,
                String::new(),
                "迁入撤回已完成".into(),
                Some(value.clone()),
            );
            Ok(value)
        }
        ImportTaskRequest::CancelCourse { batch_id } => {
            let receipt = super::course_apply::cancel_course_batch(state, &scope, batch_id)?;
            if receipt.batch_id != *batch_id
                || !matches!(receipt.status.as_str(), "cancelled" | "committed")
            {
                return Err("无法确认课程批次状态".into());
            }
            let value = json!({"batchId":receipt.batch_id,"status":receipt.status});
            state.tasks.update(
                &task.id,
                1,
                1,
                String::new(),
                "课程批次状态已确认".into(),
                Some(value.clone()),
            );
            Ok(value)
        }
        ImportTaskRequest::RevertCourse {
            batch_id,
            expected_epoch,
        } => {
            let report =
                super::course_apply::revert_course_batch(state, &scope, expected_epoch, batch_id)?;
            if !super::course_batch::valid_revert_report(&report.to_string(), batch_id) {
                return Err("课程撤回记录损坏，请检查批次".into());
            }
            let reverted = report["reverted"]
                .as_array()
                .ok_or("课程撤回记录损坏，请检查批次")?
                .len();
            let kept = report["kept"]
                .as_array()
                .ok_or("课程撤回记录损坏，请检查批次")?
                .len();
            let value = json!({"batchId":batch_id,"status":"reverted","summary":{"revertedCount":reverted,"keptCount":kept,"archivesKept":true}});
            state.tasks.update(
                &task.id,
                1,
                1,
                String::new(),
                "课程迁入撤回已完成".into(),
                Some(value.clone()),
            );
            Ok(value)
        }
    }
}
