//! UUID-only application boundary; source filesystem permissions never come from WebView.
use super::note_preview::{
    self, AttachmentSource, CourseMap, Destination, NoteCommitReceipt, StoredNotePreview,
};
use crate::state::AppState;
use serde::Deserialize;
use std::path::{Path, PathBuf};
fn uuid(id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id) {
        Ok(())
    } else {
        Err("归档或附件必须使用本库规范 UUID".into())
    }
}
fn base(state: &AppState, library: &str) -> Result<PathBuf, String> {
    uuid(library)?;
    if state.tasks.maintenance().is_some() {
        return Err("素材库正在维护，请稍后处理笔记预览".into());
    }
    state
        .with_library_if_id(library, |_, base| Ok(base.to_owned()))?
        .ok_or("素材库已切换".into())
}
fn stopped(state: &AppState, library: &str, base: &Path) -> bool {
    state.tasks.maintenance().is_some()
        || state
            .with_library_if_id(library, |_, current| Ok(current == base))
            .ok()
            .flatten()
            != Some(true)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisteredAttachment {
    pub source_key: String,
    pub attachment_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareNotePreviewRequest {
    pub archive_id: String,
    pub destination: Destination,
    pub attachments: Vec<RegisteredAttachment>,
    pub courses: CourseMap,
}
pub fn prepare_in_library(
    state: &AppState,
    library: &str,
    request: PrepareNotePreviewRequest,
) -> Result<StoredNotePreview, String> {
    let base = base(state, library)?;
    uuid(&request.archive_id)?;
    if request.attachments.len() > 1000 || request.courses.len() > 256 {
        return Err("附件或课程映射过多".into());
    }
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    let mut sources = Vec::new();
    for source in &request.attachments {
        uuid(&source.attachment_id)?;
        let (path, _) = crate::knowledge_files::path(&db, &base, &source.attachment_id)?;
        let (sha256, filename): (String, String) = db
            .query_row(
                "SELECT sha256,filename FROM knowledge_attachments WHERE id=?1",
                [&source.attachment_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| "附件登记不可用")?;
        sources.push(AttachmentSource {
            source_key: source.source_key.clone(),
            path,
            sha256,
            filename: Some(filename),
        });
    }
    let grants = crate::media_access::LocalMediaGrants::new(&state.config_directory());
    let preview = note_preview::prepare_note_preview(
        &db,
        &base,
        library,
        &request.archive_id,
        request.destination,
        &sources,
        &request.courses,
        &grants,
        &|| stopped(state, library, &base),
    )?;
    if stopped(state, library, &base) {
        return Err("素材库已切换或进入维护，预览保留在原库，未写入笔记".into());
    }
    Ok(preview)
}
pub fn get_in_library(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<StoredNotePreview, String> {
    let base = base(state, library)?;
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    let preview = note_preview::read_note_preview(&db, &base, library, id)?;
    if stopped(state, library, &base) {
        return Err("素材库已切换或进入维护，已丢弃旧预览响应".into());
    }
    Ok(preview)
}
pub fn commit_in_library(
    state: &AppState,
    library: &str,
    id: &str,
) -> Result<NoteCommitReceipt, String> {
    let base = base(state, library)?;
    let mut db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    let grants = crate::media_access::LocalMediaGrants::new(&state.config_directory());
    let result = note_preview::commit_note_preview(
        &mut db,
        &base,
        library,
        id,
        &grants,
        |tx| {
            state
                .with_library_if_id(library, |_, current| {
                    if current != base || state.tasks.maintenance().is_some() {
                        return Err("素材库位置或维护状态已变化，未提交笔记".into());
                    }
                    tx.commit().map_err(|e| e.to_string())
                })?
                .ok_or("素材库已切换，未提交笔记".into())
        },
        &|| stopped(state, library, &base),
    );
    match result {
        Ok(receipt) => Ok(receipt),
        Err(error) => {
            // Another invocation can commit while this worker is reading staged files.
            // Replay its durable acknowledgement instead of writing the note a second time.
            if !stopped(state, library, &base) {
                if let Ok(preview) = note_preview::read_note_preview(&db, &base, library, id) {
                    if let Some(receipt) = preview.receipt {
                        return Ok(receipt);
                    }
                }
            }
            Err(error)
        }
    }
}
pub fn cancel_in_library(state: &AppState, library: &str, id: &str) -> Result<Vec<String>, String> {
    let base = base(state, library)?;
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    note_preview::cancel_note_preview(&db, &base, library, id)
}

pub fn image_in_library(
    state: &AppState,
    library: &str,
    preview: &str,
    placeholder: &str,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let base = base(state, library)?;
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    let response =
        note_preview::preview_image_response(&db, &base, library, preview, placeholder, &|| {
            stopped(state, library, &base)
        })?;
    if stopped(state, library, &base) {
        return Err("素材库已切换，已丢弃旧预览图片".into());
    }
    Ok(response)
}
pub fn image_route(state: &AppState, path: &str) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let parts = path.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["note-preview", library, preview, placeholder] => {
            image_in_library(state, library, preview, placeholder)
        }
        _ => Err("预览图片地址无效".into()),
    }
}
pub fn block_in_library(
    state: &AppState,
    library: &str,
    preview: &str,
    placeholder: &str,
) -> Result<String, String> {
    let base = base(state, library)?;
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    let text = note_preview::read_preview_block(&db, &base, library, preview, placeholder)?;
    if stopped(state, library, &base) {
        return Err("素材库已切换，已丢弃旧原文响应".into());
    }
    Ok(text)
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
    .map_err(|_| "笔记预览工作线程中断，请在原库重新读取回执".to_owned())?
}
#[tauri::command]
pub async fn read_note_conversion_block(
    app: tauri::AppHandle,
    expected_library_id: String,
    preview_id: String,
    placeholder_id: String,
) -> Result<String, String> {
    job(app, expected_library_id, move |state, library| {
        block_in_library(state, library, &preview_id, &placeholder_id)
    })
    .await
}
#[tauri::command]
pub async fn prepare_note_conversion_preview(
    app: tauri::AppHandle,
    expected_library_id: String,
    request: PrepareNotePreviewRequest,
) -> Result<StoredNotePreview, String> {
    job(app, expected_library_id, move |state, library| {
        prepare_in_library(state, library, request)
    })
    .await
}
#[tauri::command]
pub async fn get_note_conversion_preview(
    app: tauri::AppHandle,
    expected_library_id: String,
    preview_id: String,
) -> Result<StoredNotePreview, String> {
    job(app, expected_library_id, move |state, library| {
        get_in_library(state, library, &preview_id)
    })
    .await
}
#[tauri::command]
pub async fn commit_note_preview(
    app: tauri::AppHandle,
    expected_library_id: String,
    preview_id: String,
) -> Result<NoteCommitReceipt, String> {
    job(app, expected_library_id, move |state, library| {
        commit_in_library(state, library, &preview_id)
    })
    .await
}
#[tauri::command]
pub async fn cancel_note_conversion_preview(
    app: tauri::AppHandle,
    expected_library_id: String,
    preview_id: String,
) -> Result<Vec<String>, String> {
    job(app, expected_library_id, move |state, library| {
        cancel_in_library(state, library, &preview_id)
    })
    .await
}
