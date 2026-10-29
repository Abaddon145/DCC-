//! Learning import uses the shared task lifecycle; no separate queue or product UI.
use super::{import_ops, source_profiles};
use crate::{state::AppState, tasks::LibraryTask};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const MAX_MARKDOWN_BYTES: usize = 5 * 1024 * 1024;

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
    },
    Commit {
        batch_id: String,
    },
    Cancel {
        batch_id: String,
    },
}

impl ImportTaskRequest {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Prepare {
                profile_id,
                epoch,
                request_sha256,
                vault,
                relative_path,
                source_sha256,
                title,
                document: _,
                markdown,
            } => {
                if !source_profiles::valid_uuid(profile_id) || !source_profiles::valid_uuid(epoch) {
                    return Err("来源身份无效".into());
                }
                if !valid_hash(request_sha256) || !valid_hash(source_sha256) {
                    return Err("迁入摘要无效".into());
                }
                if title.trim().is_empty() || title.len() > 512 {
                    return Err("迁入笔记标题无效".into());
                }
                if markdown.is_empty() || markdown.len() > MAX_MARKDOWN_BYTES {
                    return Err("迁入原文超限".into());
                }
                if !markdown.is_char_boundary(markdown.len()) {
                    return Err("迁入原文无效".into());
                }
                let digest = format!("{:x}", Sha256::digest(markdown.as_bytes()));
                if digest != *source_sha256 {
                    return Err("迁入原文摘要不匹配".into());
                }
                // Reuse origin key validation (path segments, vault text).
                let _ = super::note_origins::NoteOriginKey::new(profile_id, vault, relative_path)?;
                Ok(())
            }
            Self::Commit { batch_id } | Self::Cancel { batch_id } => {
                if !source_profiles::valid_uuid(batch_id) {
                    return Err("迁入批次无效".into());
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
        }
    }
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
        ImportTaskRequest::Prepare {
            profile_id,
            epoch,
            request_sha256,
            vault,
            relative_path,
            source_sha256,
            title,
            document,
            markdown,
        } => {
            if stopped() {
                return Err("迁入准备已停止".into());
            }
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
                    title.clone(),
                    "迁入已提交（重放）".into(),
                    Some(value.clone()),
                );
                return Ok(value);
            }
            if batch.status != "pending" {
                return Err("迁入批次已结束，请使用新的请求摘要".into());
            }
            let item = import_ops::add_item(
                state,
                &scope,
                &batch.id,
                import_ops::NewImportItem {
                    vault: vault.clone(),
                    relative_path: relative_path.clone(),
                    source_sha256: source_sha256.clone(),
                    title: title.clone(),
                    document: document.clone(),
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
                markdown.as_bytes(),
            )?;
            if stopped() {
                let _ = import_ops::cancel_batch(state, &scope, &batch.id);
                return Err("迁入准备已停止，已取消未提交批次".into());
            }
            let value = json!({
                "batchId": batch.id,
                "itemId": staged.id,
                "status": staged.status,
            });
            state.tasks.update(
                &task.id,
                1,
                1,
                title.clone(),
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
    }
}
