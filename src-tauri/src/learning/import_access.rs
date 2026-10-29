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
use tauri_plugin_dialog::DialogExt;

#[derive(Clone)]
pub(crate) struct AuthorizedSource {
    scope: source_profiles::LibraryScope,
    binding: ImportSourceBinding,
    selection_id: String,
}

#[derive(Default)]
pub(crate) struct ImportSelection {
    generation: String,
    selected: Option<AuthorizedSource>,
}

impl ImportSelection {
    pub(crate) fn permits(&self, id: &str, payload: &TaskRequest) -> bool {
        let TaskRequest::LearningImport(ImportTaskRequest::Prepare {
            profile_id,
            epoch,
            vault,
            ..
        }) = payload
        else {
            return false;
        };
        self.generation == id
            && self.selected.as_ref().is_some_and(|source| {
                source.selection_id == id
                    && source.binding.profile_id == *profile_id
                    && source.binding.epoch == *epoch
                    && source.binding.vault == *vault
            })
    }
}

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
    pub selection_id: String,
    pub target_token: String,
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
    pub current_source_epoch: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub receipt: Option<Value>,
    pub revert_report: Option<Value>,
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
    selection_id: &str,
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
        selection_id: selection_id.into(),
        target_token: inspection.target_token.clone(),
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

pub(super) fn bind_with_picker(
    state: &AppState,
    library: &str,
    vault: &str,
    picker: impl FnOnce() -> Option<PathBuf>,
) -> Result<Option<ImportSourceBinding>, String> {
    let scope = scope_for(state, library)?;
    let ticket = uuid::Uuid::new_v4().to_string();
    {
        let mut current = state
            .learning_import_selection
            .lock()
            .map_err(|_| "来源会话状态不可用")?;
        current.generation = ticket.clone();
        current.selected = None;
    }
    let picked = picker();
    {
        let current = state
            .learning_import_selection
            .lock()
            .map_err(|_| "来源会话状态不可用")?;
        if current.generation != ticket {
            return Err("来源选择已变化，请重试".into());
        }
    }
    let Some(root) = picked else {
        return Ok(None);
    };
    let selection = source_markdown::create_vault_source(state, &scope, vault, &root)?;
    source_profiles::scoped(state, &scope, |_| Ok(()))?;
    let binding = ImportSourceBinding {
        profile_id: selection.profile_id().into(),
        epoch: selection.access_epoch().into(),
        vault: selection.vault_id().into(),
        root: selection.root_path().to_string_lossy().into_owned(),
        library_id: library.into(),
        boundary:
            "仅绑定你明确选择的本地目录身份；不会自动读取真实映流清单路径，也不会覆盖已有笔记正文。"
                .into(),
    };
    let mut current = state
        .learning_import_selection
        .lock()
        .map_err(|_| "来源会话状态不可用")?;
    if current.generation != ticket {
        return Err("来源选择已变化，请重试".into());
    }
    current.selected = Some(AuthorizedSource {
        scope,
        binding: binding.clone(),
        selection_id: ticket,
    });
    Ok(Some(binding))
}

fn authorized_selection(
    state: &AppState,
    library: &str,
    profile: &str,
    epoch: &str,
    vault: &str,
    root: &str,
) -> Result<
    (
        source_markdown::VaultSelection,
        source_profiles::LibraryScope,
        String,
    ),
    String,
> {
    require_active(state, library)?;
    let selection = state
        .learning_import_selection
        .lock()
        .map_err(|_| "来源会话状态不可用")?
        .selected
        .clone()
        .ok_or("请重新选择来源目录")?;
    let binding = &selection.binding;
    if binding.library_id != library
        || binding.profile_id != profile
        || binding.epoch != epoch
        || binding.vault != vault
        || Path::new(&binding.root) != Path::new(root)
    {
        return Err("来源目录未在当前会话授权，请重新选择".into());
    }
    let vault_selection = source_markdown::select_vault(
        state,
        &selection.scope,
        profile,
        epoch,
        vault,
        Path::new(root),
    )?;
    Ok((vault_selection, selection.scope, selection.selection_id))
}

fn ensure_source_current(
    state: &AppState,
    scope: &source_profiles::LibraryScope,
    selection_id: &str,
) -> Result<(), String> {
    source_profiles::scoped(state, scope, |db| {
        let current = state
            .learning_import_selection
            .lock()
            .map_err(|_| "来源会话状态不可用")?;
        let selected = current
            .selected
            .as_ref()
            .filter(|source| source.selection_id == selection_id)
            .ok_or("来源选择已变化，请重新预检")?;
        if current.generation != selection_id
            || source_profiles::get_in(db, &selected.binding.profile_id)?.access_epoch
                != selected.binding.epoch
        {
            return Err("来源选择已变化，请重新预检".into());
        }
        Ok(())
    })
}

pub(super) fn preview_authorized(
    state: &AppState,
    library: &str,
    profile: &str,
    epoch: &str,
    vault: &str,
    root: &str,
    relative: &str,
) -> Result<ImportNotePreview, String> {
    let (selection, scope, selection_id) =
        authorized_selection(state, library, profile, epoch, vault, root)?;
    let inspection = selection.inspect(state, relative)?;
    ensure_source_current(state, &scope, &selection_id)?;
    Ok(build_preview(
        &selection,
        relative,
        &inspection,
        &selection_id,
    ))
}

pub(super) fn pick_note_with_picker(
    state: &AppState,
    library: &str,
    profile: &str,
    epoch: &str,
    vault: &str,
    root: &str,
    picker: impl FnOnce(&Path) -> Option<PathBuf>,
) -> Result<Option<ImportNotePreview>, String> {
    let (selection, scope, selection_id) =
        authorized_selection(state, library, profile, epoch, vault, root)?;
    let initial_root = selection.root_path().to_path_buf();
    drop(selection);
    let picked = picker(&initial_root);
    ensure_source_current(state, &scope, &selection_id)?;
    let Some(path) = picked else {
        return Ok(None);
    };
    if !path.is_absolute() {
        return Err("请选择来源目录中的 Markdown 文件".into());
    }
    crate::media_access::reject_links(&path)?;
    let canonical = path.canonicalize().map_err(|_| "所选笔记不可读取")?;
    crate::media_access::reject_links(&canonical)?;
    let (selection, current_scope, current_id) =
        authorized_selection(state, library, profile, epoch, vault, root)?;
    if current_scope != scope || current_id != selection_id {
        return Err("来源选择已变化，请重新选择笔记".into());
    }
    let relative = canonical
        .strip_prefix(selection.root_path())
        .map_err(|_| "请选择已授权来源目录中的笔记")?;
    let mut segments = Vec::new();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err("所选笔记路径无效".into());
        };
        segments.push(name.to_str().ok_or("所选笔记路径无法识别")?);
    }
    let relative = segments.join("/");
    let inspection = selection.inspect(state, &relative)?;
    ensure_source_current(state, &scope, &selection_id)?;
    Ok(Some(build_preview(
        &selection,
        &relative,
        &inspection,
        &selection_id,
    )))
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
    Ok(build_preview(&selection, relative_path, &inspection, ""))
}

/// Domain-separated request identity: a JSON array cannot be confused by `|` (or any
/// other byte) inside vault names or paths, and it binds the previewed target token.
pub(super) fn import_request_sha256(
    profile_id: &str,
    epoch: &str,
    vault: &str,
    relative_path: &str,
    source_sha256: &str,
    target_token: &str,
) -> String {
    let seed = serde_json::json!([
        "yingliu.learning-import.prepare.v2",
        profile_id,
        epoch,
        vault,
        relative_path,
        source_sha256,
        target_token,
    ]);
    let bytes = serde_json::to_vec(&seed).unwrap_or_default();
    format!("{:x}", Sha256::digest(&bytes))
}
pub(super) fn prepare_payload(
    state: &AppState,
    library: &str,
    profile_id: &str,
    epoch: &str,
    vault: &str,
    selected_root: &str,
    relative_path: &str,
    expected_sha256: &str,
    expected_selection_id: &str,
    expected_target_token: &str,
) -> Result<(TaskRequest, source_profiles::LibraryScope, String), String> {
    let (selection, scope, selection_id) =
        authorized_selection(state, library, profile_id, epoch, vault, selected_root)?;
    if selection_id != expected_selection_id {
        return Err("来源选择已变化，请重新预检".into());
    }
    let inspection = selection.inspect(state, relative_path)?;
    let preview = build_preview(&selection, relative_path, &inspection, &selection_id);
    if preview.source_sha256 != expected_sha256 {
        return Err("来源笔记在预检后发生变化，请重新预检".into());
    }
    if preview.target_token != expected_target_token {
        return Err("目标笔记在预检后发生变化，请重新预检".into());
    }
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
    ensure_source_current(state, &scope, &selection_id)?;
    let request_sha256 = import_request_sha256(
        &preview.profile_id,
        &preview.epoch,
        &preview.vault,
        &preview.relative_path,
        &preview.source_sha256,
        &preview.target_token,
    );
    Ok((
        TaskRequest::LearningImport(ImportTaskRequest::Prepare {
            profile_id: preview.profile_id,
            epoch: preview.epoch,
            request_sha256,
            vault: preview.vault,
            relative_path: preview.relative_path,
            source_sha256: preview.source_sha256,
            title: preview.title,
            document,
            markdown,
        }),
        scope,
        selection_id,
    ))
}

pub(crate) fn verify_prepared_target(
    db: &rusqlite::Connection,
    payload: &TaskRequest,
    expected: &str,
) -> Result<(), String> {
    let TaskRequest::LearningImport(ImportTaskRequest::Prepare {
        profile_id,
        epoch,
        vault,
        relative_path,
        source_sha256,
        ..
    }) = payload
    else {
        return Err("迁入预检任务无效".into());
    };
    if source_profiles::get_in(db, profile_id)?.access_epoch != *epoch {
        return Err("来源状态已变化，请重新预检".into());
    }
    let key = super::note_origins::NoteOriginKey::new(profile_id, vault, relative_path)?;
    if super::note_origins::preview_token(db, &key, source_sha256)? != expected {
        return Err("目标笔记在预检后发生变化，请重新预检".into());
    }
    Ok(())
}

pub fn list_batches_in_library(
    state: &AppState,
    library: &str,
) -> Result<Vec<ImportBatchView>, String> {
    let scope = scope_for(state, library)?;
    let batches = import_ops::list_batches(state, &scope)?;
    batches
        .into_iter()
        .map(|b| {
            Ok(ImportBatchView {
                id: b.id,
                status: b.status,
                request_sha256: b.request_sha256,
                source_profile_id: b.source_profile_id,
                source_epoch: b.source_epoch,
                current_source_epoch: b.current_source_epoch,
                created_at: b.created_at,
                finished_at: b.finished_at,
                receipt: b
                    .receipt_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                revert_report: b
                    .revert_report_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(|_| "迁入撤回记录损坏")?,
            })
        })
        .collect()
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
) -> Result<Option<ImportSourceBinding>, String> {
    let picker_app = app.clone();
    job(app, move |state| {
        bind_with_picker(state, &expected_library_id, &vault, || {
            picker_app
                .dialog()
                .file()
                .set_title("选择学习资料来源目录")
                .blocking_pick_folder()
                .and_then(|path| path.into_path().ok())
        })
    })
    .await
}

#[tauri::command]
pub async fn pick_learning_import_note(
    app: AppHandle,
    expected_library_id: String,
    profile_id: String,
    epoch: String,
    vault: String,
    selected_root: String,
) -> Result<Option<ImportNotePreview>, String> {
    let picker_app = app.clone();
    job(app, move |state| {
        pick_note_with_picker(
            state,
            &expected_library_id,
            &profile_id,
            &epoch,
            &vault,
            &selected_root,
            |root| {
                picker_app
                    .dialog()
                    .file()
                    .set_title("选择来源目录中的 Markdown 笔记")
                    .set_directory(root)
                    .add_filter("Markdown", &["md"])
                    .blocking_pick_file()
                    .and_then(|path| path.into_path().ok())
            },
        )
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
        preview_authorized(
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
    expected_source_sha256: String,
    expected_selection_id: String,
    expected_target_token: String,
) -> Result<LibraryTask, String> {
    let library = expected_library_id.clone();
    let handle = app.clone();
    let enqueue_target_token = expected_target_token.clone();
    let (payload, scope, selection_id) = job(handle, move |app_state| {
        prepare_payload(
            app_state,
            &library,
            &profile_id,
            &epoch,
            &vault,
            &selected_root,
            &relative_path,
            &expected_source_sha256,
            &expected_selection_id,
            &expected_target_token,
        )
    })
    .await?;
    tasks::start_scoped_learning_import(
        app,
        state,
        payload,
        &scope,
        &selection_id,
        &enqueue_target_token,
    )
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
