//! UUID-only confirmed excerpt publication; editable content stays in knowledge_notes.
use crate::{
    knowledge::{self, NoteInput},
    state::{ActiveLibrary, AppState},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::MutexGuard, time::Duration};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Destination {
    New {
        title: String,
        notebook_id: Option<String>,
    },
    Append {
        note_id: String,
        expected_revision: i64,
        insert_index: usize,
    },
    Project {
        project_id: String,
        title: String,
        notebook_id: Option<String>,
    },
    Board {
        board_id: String,
        x: f64,
        y: f64,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitRequest {
    pub operation_id: String,
    pub excerpt_id: String,
    pub destination: Destination,
    pub include_frame: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExcerptReceipt {
    pub operation_id: String,
    pub excerpt_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_item_id: Option<String>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub receipt: ExcerptReceipt,
    pub target_available: bool,
}
pub fn commit_in_library(
    state: &AppState,
    library: &str,
    request: &CommitRequest,
    cancelled: &dyn Fn() -> bool,
) -> Result<CommitResult, String> {
    commit_guarded(state, library, request, cancelled, |tx| {
        tx.commit().map_err(|e| e.to_string())
    })
}
pub(super) fn commit_guarded(
    state: &AppState,
    library: &str,
    request: &CommitRequest,
    cancelled: &dyn Fn() -> bool,
    publish: impl FnOnce(Transaction<'_>) -> Result<(), String>,
) -> Result<CommitResult, String> {
    let raw = validate_request(request)?;
    if cancelled() || state.tasks.maintenance().is_some() {
        return Err("摘录提交已停止或素材库正在维护".into());
    }
    let base = state
        .with_library_if_id(library, |_, base| Ok(base.to_owned()))?
        .ok_or("素材库已切换")?;
    let (db, stored) = {
        let _scope = scope(state, library, &base)?;
        let db = crate::tasks::open_media_worker_connection(&_scope.db_path)?;
        db.busy_timeout(Duration::ZERO).map_err(error)?;
        if let Some(receipt) = active_confirmation(&_scope, &base, library, request, &raw, true)? {
            return result(_scope.connection.as_ref().ok_or("数据库正在维护")?, receipt);
        }
        let tx = Transaction::new_unchecked(&db, TransactionBehavior::Immediate).map_err(error)?;
        if let Some((status, receipt)) = operation(&tx, request, &raw)? {
            if status == "committed" {
                return result(&tx, receipt.ok_or("摘录回执缺失")?);
            }
            if status != "pending" {
                return Err("此摘录操作已取消，请重新准备".into());
            }
        } else {
            let preview =
                super::excerpt_store::get_preview(&tx, &base, library, &request.excerpt_id)?;
            if preview.status != "ready" {
                return Err("摘录预览已使用或取消，请重新准备".into());
            }
            let used: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM learning_excerpt_operations WHERE excerpt_id=?1)",
                    [&request.excerpt_id],
                    |r| r.get(0),
                )
                .map_err(error)?;
            if used {
                return Err("此摘录已有其他操作，请使用原操作重试".into());
            }
            tx.execute("INSERT INTO learning_excerpt_operations(operation_id,excerpt_id,request_json,request_sha256,status,created_at) VALUES(?1,?2,?3,?4,'pending',?5)",params![request.operation_id,request.excerpt_id,raw,hash(&raw),chrono::Utc::now().to_rfc3339()]).map_err(error)?;
        }
        let stored = super::excerpt_store::get_preview(&tx, &base, library, &request.excerpt_id)?;
        if stored.status != "ready" {
            return Err("摘录确认已失效，请重新准备".into());
        }
        if request.include_frame && stored.prepared.frame.is_none() {
            return Err("截图未准备成功，请明确重新选择仅文字；未创建空图片".into());
        }
        tx.commit().map_err(error)?;
        (db, stored)
    };
    let p = &stored.prepared;
    let stopped = || {
        cancelled()
            || state.tasks.maintenance().is_some()
            || super::analysis::source_scope_changed(state, library, Some(&base))
    };
    if stopped() {
        return Err("摘录操作已停止，待处理意图保留".into());
    }
    let live = super::analysis::live_source_controlled(
        state,
        library,
        &p.text.course_id,
        None,
        false,
        cancelled,
    )?;
    let _subtitle_lease = if let Some(id) = &p.text.subtitle_id {
        let (sha, _cues, lease, path) =
            super::subtitles::verified_cues_leased(&live.db, &live.base, &p.text.course_id, id)?;
        if Some(sha) != p.text.subtitle_sha256 {
            return Err("字幕原件已改变，未保存旧摘录".into());
        }
        Some((lease, path))
    } else {
        None
    };
    let text = super::excerpts::prepare_text(
        &live.db,
        &live.base,
        &p.text.course_id,
        &super::excerpts::ExcerptSelection {
            subtitle_id: p.text.subtitle_id.clone(),
            cue_ids: p.text.cues.iter().map(|c| c.id.clone()).collect(),
            ranges: p.text.ranges.clone(),
            comment: p.text.comment.clone(),
            frame_time_ms: p.text.frame_time_ms,
        },
    )?;
    if live.base != base
        || live.sha != p.source_sha256
        || live.file_id != p.source_file_id
        || serde_json::to_value(&text).map_err(error)?
            != serde_json::to_value(&p.text).map_err(error)?
        || live.course.duration_ms != Some(p.duration_ms)
        || stopped()
    {
        return Err("摘录来源或课程已变化，请重新准备；已有内容保留".into());
    }

    let manifest = if request.include_frame {
        Some(bind_manifest(state, library, &base, &db, request, p)?)
    } else {
        None
    };
    // Never wait for an active-library/SQLite writer while holding a source lease.
    let _scope = scope(state, library, &base)?;
    // A restore may have replaced the active connection while source bytes were read.
    // Reopen only after taking the scope; never write through the old source worker.
    let db = crate::tasks::open_media_worker_connection(&_scope.db_path)?;
    db.busy_timeout(Duration::ZERO).map_err(error)?;
    let outcome = (|| {
        if let Some(receipt) = active_confirmation(&_scope, &base, library, request, &raw, false)? {
            return result(_scope.connection.as_ref().ok_or("数据库正在维护")?, receipt);
        }
        let tx = Transaction::new_unchecked(&db, TransactionBehavior::Immediate).map_err(error)?;
        let (status, old_receipt) = operation(&tx, request, &raw)?.ok_or("摘录操作登记缺失")?;
        if status == "committed" {
            return result(&tx, old_receipt.ok_or("摘录回执缺失")?);
        }
        if status != "pending" {
            return Err("摘录操作已取消".into());
        }
        let current = super::excerpt_store::get_preview(&tx, &base, library, &request.excerpt_id)?;
        let course = super::store::course(&tx, &p.text.course_id)?;
        let grants = crate::media_access::LocalMediaGrants::new(&state.config_directory());
        if current.status != "ready"
            || serde_json::to_value(&current.prepared).map_err(error)?
                != serde_json::to_value(p).map_err(error)?
            || course.media_id != p.text.media_id
            || course.title != p.text.title
            || course.duration_ms != Some(p.duration_ms)
            || super::analysis::source_file_id(&tx, &course.media_id)? != p.source_file_id
            || !live
                .prepared
                .matches_current(&tx, &base, &live.file_id, &grants)
            || super::analysis_store::library_identity(&base)?
                != (library.to_owned(), p.root_hash.clone())
            || cancelled()
            || state.tasks.maintenance().is_some()
        {
            return Err("提交前素材库或来源登记已变化，未改动笔记".into());
        }
        if let Some(id) = &p.text.subtitle_id {
            let (path, sha) = super::subtitles::registered(&tx, &base, &p.text.course_id, id)?;
            if Some(sha) != p.text.subtitle_sha256
                || _subtitle_lease.as_ref().map(|(_, held)| held) != Some(&path)
            {
                return Err("字幕登记已改变，未改动笔记".into());
            }
        }
        if let Some(manifest) = &manifest {
            let bound:Option<String>=tx.query_row("SELECT manifest_relative FROM learning_excerpt_operations WHERE operation_id=?1",[&request.operation_id],|r|r.get(0)).map_err(error)?;
            if bound != Some(super::excerpt_files::locator(manifest)?) {
                return Err("截图清单登记已改变".into());
            }
        }
        let (receipt, _published_leases) = write_target(&tx, &base, request, p, manifest.as_ref())?;
        tx.execute("UPDATE learning_excerpt_operations SET status='committed',receipt_json=?1,committed_at=?2 WHERE operation_id=?3 AND status='pending'",params![serde_json::to_string(&receipt).map_err(error)?,chrono::Utc::now().to_rfc3339(),request.operation_id]).map_err(error)?;
        tx.execute(
            "UPDATE learning_excerpts SET status='committed' WHERE id=?1 AND status='ready'",
            [&request.excerpt_id],
        )
        .map_err(error)?;
        if cancelled() || state.tasks.maintenance().is_some() {
            return Err("摘录已停止，未提交目标；原有内容保留".into());
        }
        if let Err(failure) = publish(tx) {
            // An exception can mean a lost acknowledgement, not a rolled-back commit.
            return match operation(&db, request, &raw) {
                Ok(Some((status, Some(saved)))) if status == "committed" => result(&db, saved),
                Ok(_) => Err(failure),
                Err(read) => Err(format!(
                    "提交状态待核对，请保留操作ID重试：{failure}；回执暂不可读：{read}"
                )),
            };
        }
        result(&db, receipt)
    })();
    if let Err(failure) = outcome {
        match operation(&db, request, &raw) {
            Ok(Some((status, Some(receipt)))) if status == "committed" => {
                return result(&db, receipt)
            }
            Err(read) => return Err(format!("保存状态待核对，副本保留：{failure}；{read}")),
            _ => {}
        }
        if let Some(manifest) = &manifest {
            if let Err(cleanup) = super::excerpt_files::cleanup_uncommitted(&db, &base, manifest) {
                return Err(format!("{failure}；副本保留待核对：{cleanup}"));
            }
        }
        return Err(failure);
    }
    outcome
}
pub fn cancel_in_library(
    state: &AppState,
    library: &str,
    request: &CommitRequest,
) -> Result<(), String> {
    let raw = validate_request(request)?;
    let base = state
        .with_library_if_id(library, |_, base| Ok(base.to_owned()))?
        .ok_or("素材库已切换")?;
    let _scope = scope(state, library, &base)?;
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    db.busy_timeout(Duration::ZERO).map_err(error)?;
    let tx = Transaction::new_unchecked(&db, TransactionBehavior::Immediate).map_err(error)?;
    let (status, _) = operation(&tx, request, &raw)?.ok_or("没有此摘录操作")?;
    if status == "committed" {
        return Err("摘录已保存，不能取消历史回执".into());
    }
    tx.execute("UPDATE learning_excerpt_operations SET status='cancelled' WHERE operation_id=?1 AND status='pending'",[&request.operation_id]).map_err(error)?;
    tx.execute(
        "UPDATE learning_excerpts SET status='cancelled' WHERE id=?1 AND status='ready'",
        [&request.excerpt_id],
    )
    .map_err(error)?;
    let manifest: Option<String> = tx
        .query_row(
            "SELECT manifest_relative FROM learning_excerpt_operations WHERE operation_id=?1",
            [&request.operation_id],
            |r| r.get(0),
        )
        .map_err(error)?;
    tx.commit().map_err(error)?;
    if let Some(location) = manifest {
        let manifest = super::excerpt_files::read_manifest(
            &base,
            &request.operation_id,
            &request.excerpt_id,
            library,
            &hash(&raw),
            &location,
        )?;
        super::excerpt_files::cleanup_uncommitted(&db, &base, &manifest)?;
    }
    Ok(())
}

fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn uuid(id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id) {
        Ok(())
    } else {
        Err("摘录操作或目标ID无效".into())
    }
}
pub(super) fn validate_request(request: &CommitRequest) -> Result<String, String> {
    uuid(&request.operation_id)?;
    uuid(&request.excerpt_id)?;
    match &request.destination {
        Destination::Board { board_id, x, y } => {
            uuid(board_id)?;
            if !request.include_frame
                || !x.is_finite()
                || !y.is_finite()
                || x.abs() > 1_000_000.0
                || y.abs() > 1_000_000.0
            {
                return Err("参考板需要真实截图和有效位置".into());
            }
        }
        Destination::New { title, notebook_id }
        | Destination::Project {
            title, notebook_id, ..
        } => {
            if title.trim().is_empty()
                || title.chars().count() > 300
                || title.chars().any(char::is_control)
            {
                return Err("摘录笔记标题无效".into());
            }
            if let Some(id) = notebook_id {
                uuid(id)?;
            }
            if let Destination::Project { project_id, .. } = &request.destination {
                uuid(project_id)?;
            }
        }
        Destination::Append {
            note_id,
            expected_revision,
            insert_index,
        } => {
            uuid(note_id)?;
            if *expected_revision < 1 || *insert_index > 50000 {
                return Err("目标修订或插入位置无效".into());
            }
        }
    }
    let raw = serde_json::to_string(request).map_err(error)?;
    if raw.len() > 65536 {
        return Err("摘录确认请求超限".into());
    }
    Ok(raw)
}
fn scope<'a>(
    state: &'a AppState,
    library: &str,
    base: &Path,
) -> Result<MutexGuard<'a, ActiveLibrary>, String> {
    let active = state
        .active
        .try_lock()
        .map_err(|_| "素材库繁忙，请使用同一操作稍后重试")?;
    if active.marker.library_id != library
        || active.base_dir != base
        || active.connection.is_none()
        || state.tasks.maintenance().is_some()
        || super::analysis_store::library_identity(base)?.0 != library
    {
        return Err("素材库或维护状态已变化".into());
    }
    Ok(active)
}
fn operation(
    db: &Connection,
    request: &CommitRequest,
    raw: &str,
) -> Result<Option<(String, Option<ExcerptReceipt>)>, String> {
    let row:Option<(String,String,String,String,Option<String>)>=db.query_row("SELECT excerpt_id,request_json,request_sha256,status,receipt_json FROM learning_excerpt_operations WHERE operation_id=?1 AND length(CAST(request_json AS BLOB))<=65536 AND (receipt_json IS NULL OR length(CAST(receipt_json AS BLOB))<=65536)",[&request.operation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(error)?;
    let Some((excerpt, saved, sha, status, receipt)) = row else {
        return Ok(None);
    };
    if excerpt != request.excerpt_id || saved != raw || sha != hash(raw) {
        return Err("操作ID已绑定其他确认内容，未重复写入".into());
    }
    let receipt: Option<ExcerptReceipt> = receipt
        .map(|r| serde_json::from_str(&r).map_err(|_| "摘录回执损坏"))
        .transpose()?;
    if !matches!(status.as_str(), "pending" | "committed" | "cancelled")
        || (status == "committed") != receipt.is_some()
        || receipt.as_ref().is_some_and(|r| {
            r.operation_id != request.operation_id
                || r.excerpt_id != request.excerpt_id
                || !valid_receipt_target(request, r)
        })
    {
        return Err("摘录操作状态或回执身份无效".into());
    }
    Ok(Some((status, receipt)))
}
fn result(db: &Connection, receipt: ExcerptReceipt) -> Result<CommitResult, String> {
    let available: bool = if let Some(id) = &receipt.note_id {
        db.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_notes WHERE id=?1 AND deleted_at IS NULL)",
            [id],
            |r| r.get(0),
        )
        .map_err(error)?
    } else {
        db.query_row("SELECT EXISTS(SELECT 1 FROM reference_board_items WHERE id=?1 AND board_id=?2 AND deleted_at IS NULL)",params![receipt.board_item_id,receipt.board_id],|r|r.get(0)).map_err(error)?
    };
    Ok(CommitResult {
        receipt,
        target_available: available,
    })
}

fn valid_receipt_target(request: &CommitRequest, receipt: &ExcerptReceipt) -> bool {
    match &request.destination {
        Destination::Board { board_id, .. } => {
            receipt.note_id.is_none()
                && receipt.revision.is_none()
                && receipt.board_id.as_ref() == Some(board_id)
                && receipt
                    .board_item_id
                    .as_deref()
                    .is_some_and(|id| uuid(id).is_ok())
        }
        _ => {
            receipt
                .note_id
                .as_deref()
                .is_some_and(|id| uuid(id).is_ok())
                && receipt.revision.is_some_and(|r| r > 0)
                && receipt.board_id.is_none()
                && receipt.board_item_id.is_none()
                && match &request.destination {
                    Destination::Append { note_id, .. } => {
                        receipt.note_id.as_ref() == Some(note_id)
                    }
                    _ => true,
                }
        }
    }
}
fn active_confirmation(
    active: &ActiveLibrary,
    base: &Path,
    library: &str,
    request: &CommitRequest,
    raw: &str,
    allow_new: bool,
) -> Result<Option<ExcerptReceipt>, String> {
    let db = active.connection.as_ref().ok_or("数据库正在维护")?;
    match operation(db, request, raw)? {
        Some((status, receipt)) if status == "committed" => {
            return Ok(Some(receipt.ok_or("摘录回执缺失")?))
        }
        Some((status, _)) if status == "pending" => {}
        Some(_) => return Err("当前活动库中的操作已取消，旧工作连接不能继续保存".into()),
        None if allow_new => {}
        None => return Err("当前活动库中没有此待处理操作，请重新准备".into()),
    }
    if super::excerpt_store::get_preview(db, base, library, &request.excerpt_id)?.status != "ready"
    {
        return Err("当前活动库中的摘录确认已失效".into());
    }
    Ok(None)
}

fn bind_manifest(
    state: &AppState,
    library: &str,
    base: &Path,
    db: &Connection,
    request: &CommitRequest,
    p: &super::excerpt_runtime::PreparedExcerpt,
) -> Result<super::excerpt_files::PublicationManifest, String> {
    use super::excerpt_files::{self, FileDestination};
    let frame = p.frame.as_ref().ok_or("没有可发布的截图")?;
    let staged =
        excerpt_files::prepare_files(base, &request.operation_id, library, &p.root_hash, frame)?;
    let raw = serde_json::to_string(request).map_err(error)?;
    let sha = hash(&raw);
    let location: Option<String> = db
        .query_row(
            "SELECT manifest_relative FROM learning_excerpt_operations WHERE operation_id=?1",
            [&request.operation_id],
            |r| r.get(0),
        )
        .map_err(error)?;
    let candidate = if let Some(location) = location {
        location
    } else {
        let target = match &request.destination {
            Destination::Board { board_id, .. } => FileDestination::Board {
                board_id: board_id.clone(),
            },
            Destination::Append { note_id, .. } => FileDestination::Note {
                note_id: note_id.clone(),
            },
            _ => FileDestination::Note {
                note_id: uuid::Uuid::new_v4().to_string(),
            },
        };
        excerpt_files::create_manifest(
            base,
            &request.operation_id,
            &request.excerpt_id,
            library,
            &p.root_hash,
            &sha,
            target,
            &staged,
        )?
        .1
    };
    let location = {
        let _scope = scope(state, library, base)?;
        if active_confirmation(&_scope, base, library, request, &raw, false)?.is_some() {
            return Err("摘录已保存，请使用同一操作读取回执".into());
        }
        let current = crate::tasks::open_media_worker_connection(&_scope.db_path)?;
        current.busy_timeout(Duration::ZERO).map_err(error)?;
        let tx =
            Transaction::new_unchecked(&current, TransactionBehavior::Immediate).map_err(error)?;
        if operation(&tx, request, &raw)?.map(|v| v.0).as_deref() != Some("pending") {
            return Err("摘录状态已变化，请用同一操作重读回执".into());
        }
        tx.execute("UPDATE learning_excerpt_operations SET manifest_relative=?1 WHERE operation_id=?2 AND manifest_relative IS NULL AND status='pending'",params![candidate,request.operation_id]).map_err(error)?;
        let winner: String = tx
            .query_row(
                "SELECT manifest_relative FROM learning_excerpt_operations WHERE operation_id=?1",
                [&request.operation_id],
                |r| r.get(0),
            )
            .map_err(error)?;
        tx.commit().map_err(error)?;
        winner
    };
    // ponytail: retain rare unbound, operation-owned manifests after contention;
    // add bounded manual cleanup when a recovery UI needs to reclaim those few KiB.
    let manifest = excerpt_files::read_manifest(
        base,
        &request.operation_id,
        &request.excerpt_id,
        library,
        &sha,
        &location,
    )?;
    let target_matches = match (&request.destination, &manifest.target) {
        (Destination::Board { board_id, .. }, FileDestination::Board { board_id: stored }) => {
            board_id == stored
        }
        (Destination::Append { note_id, .. }, FileDestination::Note { note_id: stored }) => {
            note_id == stored
        }
        (Destination::New { .. } | Destination::Project { .. }, FileDestination::Note { .. }) => {
            true
        }
        _ => false,
    };
    if !target_matches
        || manifest.root_hash != p.root_hash
        || manifest.width != staged.width
        || manifest.height != staged.height
        || manifest.files[0].sha256 != frame.sha256
        || manifest.files[0].size != staged.original.size
        || manifest
            .files
            .get(1)
            .is_some_and(|f| f.sha256 != staged.thumbnail.sha256 || f.size != staged.thumbnail.size)
    {
        return Err("截图清单与确认目标或原帧不符".into());
    }
    Ok(manifest)
}

fn write_target(
    tx: &Connection,
    base: &Path,
    request: &CommitRequest,
    p: &super::excerpt_runtime::PreparedExcerpt,
    manifest: Option<&super::excerpt_files::PublicationManifest>,
) -> Result<(ExcerptReceipt, Vec<std::fs::File>), String> {
    use super::excerpt_files::{self, FileDestination};
    if let Destination::Board { board_id, x, y } = &request.destination {
        let manifest = manifest.ok_or("参考板必须有真实截图")?;
        if manifest.target
            != (FileDestination::Board {
                board_id: board_id.clone(),
            })
        {
            return Err("参考板清单目标不符".into());
        }
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM reference_boards WHERE id=?1)",
                [board_id],
                |r| r.get(0),
            )
            .map_err(error)?;
        if !exists {
            return Err("参考板不存在，未发布截图".into());
        }
        excerpt_files::publish_files(base, manifest)?;
        let leases = excerpt_files::lock_published_files(base, manifest)?;
        let stored = crate::images::StoredImage {
            id: manifest.image_id.clone(),
            original_name: format!(
                "学习截图-{}.jpg",
                p.frame.as_ref().ok_or("截图时间缺失")?.time_ms
            ),
            original_rel_path: manifest.files[0].final_relative.clone(),
            thumbnail_rel_path: manifest.files[1].final_relative.clone(),
            pixel_width: manifest.width,
            pixel_height: manifest.height,
        };
        crate::reference_boards::insert_prepared_jpeg_in(tx, board_id, &stored, *x, *y)?;
        tx.execute(
            "INSERT INTO learning_excerpt_board_items(item_id,excerpt_id) VALUES(?1,?2)",
            params![stored.id, request.excerpt_id],
        )
        .map_err(error)?;
        return Ok((
            ExcerptReceipt {
                operation_id: request.operation_id.clone(),
                excerpt_id: request.excerpt_id.clone(),
                note_id: None,
                revision: None,
                board_id: Some(board_id.clone()),
                board_item_id: Some(stored.id),
                warnings: p.warnings.clone(),
            },
            leases,
        ));
    }
    let mut document = p.document.clone();
    if let Some(manifest) = manifest {
        let at = p.frame.as_ref().ok_or("截图时间缺失")?.time_ms;
        let reference = super::note_nodes::LearningReference {
            course_id: p.text.course_id.clone(),
            media_id: p.text.media_id.clone(),
            fingerprint: p.source_sha256.clone(),
            ranges: vec![super::note_nodes::TimeRange {
                start_ms: at,
                end_ms: at + 1,
            }],
            label: p.text.title.clone(),
        };
        reference.validate()?;
        let body = document["content"].as_array_mut().ok_or("摘录正文无效")?;
        body.push(serde_json::json!({"type":"image","attrs":{"src":format!("http://dcc-media.localhost/note/{}",manifest.image_id),"alt":p.text.title}}));
        body.push(serde_json::json!({"type":"learningReference","attrs":reference}));
    }
    let mut input = match &request.destination {
        Destination::Append {
            note_id,
            expected_revision,
            insert_index,
        } => {
            let old = knowledge::get(tx, note_id)?;
            if old.deleted_at.is_some() || old.revision != *expected_revision {
                return Err("目标笔记已修改或删除，请保留当前草稿并重新确认位置".into());
            }
            let mut body = old.document;
            let nodes = body
                .get_mut("content")
                .and_then(|v| v.as_array_mut())
                .ok_or("目标笔记正文结构无效")?;
            if *insert_index > nodes.len() {
                return Err("摘录插入位置已失效，未改动笔记".into());
            }
            nodes.splice(
                *insert_index..*insert_index,
                document["content"]
                    .as_array()
                    .ok_or("摘录正文无效")?
                    .iter()
                    .cloned(),
            );
            NoteInput {
                id: Some(old.id),
                expected_revision: Some(old.revision),
                notebook_id: old.notebook_id,
                title: old.title,
                document: body,
                tags: old.tags,
                favorite: old.favorite,
                journal_date: old.journal_date,
            }
        }
        Destination::New { title, notebook_id }
        | Destination::Project {
            title, notebook_id, ..
        } => {
            if let Destination::Project { project_id, .. } = &request.destination {
                let (name, status): (String, String) = tx
                    .query_row(
                        "SELECT name,status FROM projects WHERE id=?1",
                        [project_id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .map_err(|_| "项目不存在")?;
                if status == "archived" {
                    return Err("项目已归档，请先恢复项目".into());
                }
                document["content"].as_array_mut().ok_or("摘录正文无效")?.insert(0,serde_json::json!({"type":"paragraph","content":[{"type":"reference","attrs":{"kind":"project","targetId":project_id,"label":name}}]}));
                crate::media_library::link_projects(
                    tx,
                    vec![p.text.media_id.clone()],
                    vec![project_id.clone()],
                    true,
                )?;
            }
            NoteInput {
                id: None,
                expected_revision: None,
                notebook_id: notebook_id.clone(),
                title: title.clone(),
                document,
                tags: vec![],
                favorite: false,
                journal_date: None,
            }
        }
        Destination::Board { .. } => return Err("参考板目标无效".into()),
    };
    let was_new = input.id.is_none();
    let mut leases = vec![];
    if let Some(manifest) = manifest {
        let FileDestination::Note { note_id } = &manifest.target else {
            return Err("笔记清单目标无效".into());
        };
        if let Some(id) = &input.id {
            if id != note_id {
                return Err("笔记清单目标不符".into());
            }
        } else {
            let mut seed = input.clone();
            seed.document =
                serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[]}]});
            let note = knowledge::save_reserved_new_in(tx, seed, note_id)?;
            input.id = Some(note.id);
            input.expected_revision = Some(note.revision);
        }
        excerpt_files::publish_files(base, manifest)?;
        leases = excerpt_files::lock_published_files(base, manifest)?;
        crate::knowledge_files::register_published_jpeg(
            tx,
            base,
            note_id,
            &manifest.image_id,
            &format!(
                "学习截图-{}.jpg",
                p.frame.as_ref().ok_or("截图时间缺失")?.time_ms
            ),
            &manifest.files[0].sha256,
            manifest.files[0].size,
        )?;
    }
    let note = knowledge::save_in(tx, input, true)?;
    if was_new && manifest.is_some() {
        tx.execute(
            "DELETE FROM knowledge_revisions WHERE note_id=?1",
            [&note.id],
        )
        .map_err(error)?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO learning_course_notes(course_id,note_id) VALUES(?1,?2)",
        params![p.text.course_id, note.id],
    )
    .map_err(error)?;
    let mut warnings = p.warnings.clone();
    if p.frame.is_some() && !request.include_frame {
        warnings.push("已按确认仅保存文字，截图未写入".into());
    }
    Ok((
        ExcerptReceipt {
            operation_id: request.operation_id.clone(),
            excerpt_id: request.excerpt_id.clone(),
            note_id: Some(note.id),
            revision: Some(note.revision),
            board_id: None,
            board_item_id: None,
            warnings,
        },
        leases,
    ))
}
