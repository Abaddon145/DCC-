//! Learning Markdown import access: explicit vault bind, preview, and task queue.
//! Never opens a path until the caller supplies a user-selected absolute root.
//! Does not treat list hints as authorization and never silently overwrites notes.
use super::{
    import_ops, import_runner::ImportTaskRequest, note_origins::NoteOriginState, source_markdown,
    source_profiles,
};
use crate::{
    state::AppState,
    tasks::{self, LibraryTask, TaskRequest},
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceBinding {
    pub profile_id: String,
    pub epoch: String,
    pub vault: String,
    pub root: String,
    pub library_id: String,
    pub boundary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportNotePreview {
    pub profile_id: String,
    pub epoch: String,
    pub vault: String,
    pub relative_path: String,
    pub source_sha256: String,
    pub conflict: String,
    pub target_id: Option<String>,
    pub title: String,
    pub pending_conversion: bool,
    pub warnings: Vec<String>,
    pub can_prepare: bool,
    pub will_overwrite: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchView {
    pub id: String,
    pub status: String,
    pub request_sha256: String,
    pub source_profile_id: String,
    pub source_epoch: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub receipt: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemView {
    pub id: String,
    pub batch_id: String,
    pub status: String,
    pub conflict_code: Option<String>,
    pub target_id: Option<String>,
    pub source_sha256: String,
    pub stage_relative: Option<String>,
}

fn conflict_label(state: &NoteOriginState) -> (String, Option<String>) {
    match state {
        NoteOriginState::Unmapped => ("unmapped".into(), None),
        NoteOriginState::Unchanged(id) => ("unchanged".into(), Some(id.clone())),
        NoteOriginState::SourceChanged(id) => ("source_changed".into(), Some(id.clone())),
        NoteOriginState::TargetChanged(id) => ("target_changed".into(), Some(id.clone())),
        NoteOriginState::BothChanged(id) => ("both_changed".into(), Some(id.clone())),
        NoteOriginState::TargetUnavailable(id) => ("target_unavailable".into(), Some(id.clone())),
    }
}

fn title_from(relative: &str, document: &Option<Value>) -> String {
    if let Some(doc) = document {
        if let Some(text) = first_text(doc) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.chars().take(80).collect();
            }
        }
    }
    Path::new(relative)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("迁入笔记")
        .chars()
        .take(80)
        .collect()
}

fn first_text(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if map.get("type").and_then(|v| v.as_str()) == Some("text") {
                return map.get("text").and_then(|v| v.as_str()).map(str::to_string);
            }
            if let Some(Value::Array(content)) = map.get("content") {
                for child in content {
                    if let Some(text) = first_text(child) {
                        return Some(text);
                    }
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(first_text),
        _ => None,
    }
}

fn require_active(state: &AppState, library: &str) -> Result<(), String> {
    if state.active_library_id()? != library {
        return Err("素材库已切换，请在当前库重新选择来源".into());
    }
    Ok(())
}

fn scope_for(state: &AppState, library: &str) -> Result<source_profiles::LibraryScope, String> {
    require_active(state, library)?;
    let scope = source_profiles::scope(state, library)?;
    require_active(state, library)?;
    Ok(scope)
}

fn open_selection(
    state: &AppState,
    library: &str,
    profile_id: &str,
    epoch: &str,
    vault: &str,
    selected_root: &str,
) -> Result<source_markdown::VaultSelection, String> {
    let scope = scope_for(state, library)?;
    let root = PathBuf::from(selected_root);
    let selection = source_markdown::select_vault(state, &scope, profile_id, epoch, vault, &root)?;
    require_active(state, library)?;
    Ok(selection)
}

fn build_preview(
    selection: &source_markdown::VaultSelection,
    relative_path: &str,
    inspection: &source_markdown::Inspection,
) -> ImportNotePreview {
    let (conflict, target_id) = conflict_label(&inspection.state);
    let document = &inspection.conversion.document;
    let pending = inspection.conversion.pending || document.is_none();
    let title = title_from(relative_path, document);
    let can_prepare = !pending && document.is_some();
    let message = if !can_prepare {
        "转换仍有未解析内容或缺少正文，已失败闭合，不能进入暂存。".into()
    } else {
        match conflict.as_str() {
            "unmapped" => "预检通过：将新建笔记，不会改动已有正文。".into(),
            "unchanged" => "目标未变：提交时仅确认来源，不会覆盖正文。".into(),
            _ => "存在冲突：可以暂存并提交冲突回执，但绝不会静默覆盖已有笔记正文。".into(),
        }
    };
    ImportNotePreview {
        profile_id: selection.profile_id().into(),
        epoch: selection.access_epoch().into(),
        vault: selection.vault_id().into(),
        relative_path: relative_path.into(),
        source_sha256: inspection.sha256.clone(),
        conflict,
        target_id,
        title,
        pending_conversion: pending,
        warnings: inspection.conversion.warnings.clone(),
        can_prepare,
        will_overwrite: false,
        message,
    }
}

pub fn bind_in_library(
    state: &AppState,
    library: &str,
    vault: &str,
    selected_root: &str,
) -> Result<ImportSourceBinding, String> {
    let scope = scope_for(state, library)?;
    let root = PathBuf::from(selected_root);
    let selection = source_markdown::create_vault_source(state, &scope, vault, &root)?;
    require_active(state, library)?;
    Ok(ImportSourceBinding {
        profile_id: selection.profile_id().into(),
        epoch: selection.access_epoch().into(),
        vault: selection.vault_id().into(),
        root: selection.root_path().to_string_lossy().into_owned(),
        library_id: library.into(),
        boundary:
            "仅绑定你明确选择的本地目录身份；不会自动读取真实映流清单路径，也不会覆盖已有笔记正文。"
                .into(),
    })
}

pub fn preview_in_library(
    state: &AppState,
    library: &str,
    profile_id: &str,
    epoch: &str,
    vault: &str,
    selected_root: &str,
    relative_path: &str,
) -> Result<ImportNotePreview, String> {
    let selection = open_selection(state, library, profile_id, epoch, vault, selected_root)?;
    let inspection = selection.inspect(state, relative_path)?;
    require_active(state, library)?;
    Ok(build_preview(&selection, relative_path, &inspection))
}

fn prepare_payload(
    state: &AppState,
    library: &str,
    profile_id: &str,
    epoch: &str,
    vault: &str,
    selected_root: &str,
    relative_path: &str,
) -> Result<TaskRequest, String> {
    let selection = open_selection(state, library, profile_id, epoch, vault, selected_root)?;
    let inspection = selection.inspect(state, relative_path)?;
    let preview = build_preview(&selection, relative_path, &inspection);
    if !preview.can_prepare {
        return Err(preview.message);
    }
    if preview.will_overwrite {
        return Err("迁入不会覆盖已有笔记正文".into());
    }
    let (markdown, digest) = source_markdown::read_markdown_text(&selection, relative_path)?;
    if digest != preview.source_sha256 {
        return Err("来源笔记在预检后发生变化，请重新检查".into());
    }
    let document = inspection
        .conversion
        .document
        .ok_or_else(|| "尚无可用正文，不能进入暂存".to_string())?;
    require_active(state, library)?;
    let request_seed = format!(
        "{}|{}|{}|{}|{}",
        preview.profile_id,
        preview.vault,
        preview.relative_path,
        preview.source_sha256,
        preview.epoch
    );
    let request_sha256 = format!("{:x}", Sha256::digest(request_seed.as_bytes()));
    Ok(TaskRequest::LearningImport(ImportTaskRequest::Prepare {
        profile_id: preview.profile_id,
        epoch: preview.epoch,
        request_sha256,
        vault: preview.vault,
        relative_path: preview.relative_path,
        source_sha256: preview.source_sha256,
        title: preview.title,
        document,
        markdown,
    }))
}

pub fn list_batches_in_library(
    state: &AppState,
    library: &str,
) -> Result<Vec<ImportBatchView>, String> {
    let scope = scope_for(state, library)?;
    let batches = import_ops::list_batches(state, &scope)?;
    Ok(batches
        .into_iter()
        .map(|b| ImportBatchView {
            id: b.id,
            status: b.status,
            request_sha256: b.request_sha256,
            source_profile_id: b.source_profile_id,
            source_epoch: b.source_epoch,
            created_at: b.created_at,
            finished_at: b.finished_at,
            receipt: b
                .receipt_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
        })
        .collect())
}

pub fn list_items_in_library(
    state: &AppState,
    library: &str,
    batch_id: &str,
) -> Result<Vec<ImportItemView>, String> {
    let scope = scope_for(state, library)?;
    let items = import_ops::list_items(state, &scope, batch_id)?;
    Ok(items
        .into_iter()
        .map(|i| ImportItemView {
            id: i.id,
            batch_id: i.batch_id,
            status: i.status,
            conflict_code: i.conflict_code,
            target_id: i.target_id,
            source_sha256: i.source_sha256,
            stage_relative: i.stage_relative,
        })
        .collect())
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
pub async fn bind_learning_import_source(
    app: AppHandle,
    expected_library_id: String,
    vault: String,
    selected_root: String,
) -> Result<ImportSourceBinding, String> {
    job(app, move |state| {
        bind_in_library(state, &expected_library_id, &vault, &selected_root)
    })
    .await
}

#[tauri::command]
pub async fn preview_learning_import_note(
    app: AppHandle,
    expected_library_id: String,
    profile_id: String,
    epoch: String,
    vault: String,
    selected_root: String,
    relative_path: String,
) -> Result<ImportNotePreview, String> {
    job(app, move |state| {
        preview_in_library(
            state,
            &expected_library_id,
            &profile_id,
            &epoch,
            &vault,
            &selected_root,
            &relative_path,
        )
    })
    .await
}

#[tauri::command]
pub async fn queue_learning_import_prepare(
    app: AppHandle,
    state: State<'_, AppState>,
    expected_library_id: String,
    profile_id: String,
    epoch: String,
    vault: String,
    selected_root: String,
    relative_path: String,
) -> Result<LibraryTask, String> {
    let library = expected_library_id.clone();
    let handle = app.clone();
    let payload = job(handle, move |app_state| {
        prepare_payload(
            app_state,
            &library,
            &profile_id,
            &epoch,
            &vault,
            &selected_root,
            &relative_path,
        )
    })
    .await?;
    tasks::start_library_task_for_library(app, state, payload, Some(&expected_library_id))
}

#[tauri::command]
pub async fn list_learning_import_batches(
    app: AppHandle,
    expected_library_id: String,
) -> Result<Vec<ImportBatchView>, String> {
    job(app, move |state| {
        list_batches_in_library(state, &expected_library_id)
    })
    .await
}

#[tauri::command]
pub async fn list_learning_import_items(
    app: AppHandle,
    expected_library_id: String,
    batch_id: String,
) -> Result<Vec<ImportItemView>, String> {
    job(app, move |state| {
        list_items_in_library(state, &expected_library_id, &batch_id)
    })
    .await
}
