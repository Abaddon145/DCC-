//! UUID-only confirmed excerpt publication; editable content stays in knowledge_notes.
use crate::{
    knowledge::{self, NoteInput},
    state::{ActiveLibrary, AppState},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::MutexGuard, time::Duration};
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub note_id: String,
    pub revision: i64,
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
    let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
    db.busy_timeout(Duration::ZERO).map_err(error)?;
    let stored = {
        let _scope = scope(state, library, &base)?;
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
        tx.commit().map_err(error)?;
        stored
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

    // Never wait for an active-library/SQLite writer while holding a source lease.
    let _scope = scope(state, library, &base)?;
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
    let mut document = p.document.clone();
    let input = match &request.destination {
        Destination::Append {
            note_id,
            expected_revision,
            insert_index,
        } => {
            let old = knowledge::get(&tx, note_id)?;
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
                    &tx,
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
    };
    let note = knowledge::save_in(&tx, input, true)?;
    tx.execute(
        "INSERT OR IGNORE INTO learning_course_notes(course_id,note_id) VALUES(?1,?2)",
        params![p.text.course_id, note.id],
    )
    .map_err(error)?;
    let mut warnings = p.warnings.clone();
    if p.frame.is_some() {
        warnings.push("已按确认仅保存文字，截图未写入".into());
    }
    let receipt = ExcerptReceipt {
        operation_id: request.operation_id.clone(),
        excerpt_id: request.excerpt_id.clone(),
        note_id: note.id,
        revision: note.revision,
        warnings,
    };
    tx.execute("UPDATE learning_excerpt_operations SET status='committed',receipt_json=?1,committed_at=?2 WHERE operation_id=?3 AND status='pending'",params![serde_json::to_string(&receipt).map_err(error)?,chrono::Utc::now().to_rfc3339(),request.operation_id]).map_err(error)?;
    tx.execute(
        "UPDATE learning_excerpts SET status='committed' WHERE id=?1 AND status='ready'",
        [&request.excerpt_id],
    )
    .map_err(error)?;
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
    tx.commit().map_err(error)
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
fn validate_request(request: &CommitRequest) -> Result<String, String> {
    uuid(&request.operation_id)?;
    uuid(&request.excerpt_id)?;
    if request.include_frame {
        return Err("截图发布尚未接入，请明确选择仅文字；未静默丢弃图片".into());
    }
    match &request.destination {
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
                || r.revision < 1
                || uuid(&r.note_id).is_err()
        })
    {
        return Err("摘录操作状态或回执身份无效".into());
    }
    Ok(Some((status, receipt)))
}
fn result(db: &Connection, receipt: ExcerptReceipt) -> Result<CommitResult, String> {
    let available: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_notes WHERE id=?1 AND deleted_at IS NULL)",
            [&receipt.note_id],
            |r| r.get(0),
        )
        .map_err(error)?;
    Ok(CommitResult {
        receipt,
        target_available: available,
    })
}
