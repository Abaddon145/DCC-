//! Immutable local material, not permission to resume an old write.
use super::{excerpt_runtime::PreparedExcerpt, excerpts::ExcerptSelection};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, io::Cursor, path::Path};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredExcerpt {
    pub id: String,
    pub status: String,
    pub prepared: PreparedExcerpt,
}
const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
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
        Err("摘录身份无效".into())
    }
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn identity(base: &Path, library: &str) -> Result<String, String> {
    uuid(library)?;
    let (actual, root) = super::analysis_store::library_identity(base)?;
    if actual != library {
        return Err("素材库已切换".into());
    }
    Ok(root)
}

// History remains readable without a still-existing course or source file.
fn validate(p: &PreparedExcerpt) -> Result<(), String> {
    let t = &p.text;
    for id in [&p.library_id, &p.source_file_id, &t.course_id, &t.media_id] {
        uuid(id)?;
    }
    if let Some(id) = &t.subtitle_id {
        uuid(id)?;
    }
    if !digest(&p.root_hash)
        || !digest(&p.source_sha256)
        || p.duration_ms == 0
        || p.duration_ms > 604800000
        || t.subtitle_id.is_some() != t.subtitle_sha256.is_some()
        || t.subtitle_id.is_some() == t.cues.is_empty()
        || t.subtitle_sha256.as_ref().is_some_and(|s| !digest(s))
        || t.cues.len() > 256
        || t.cues.iter().map(|cue| cue.text.len()).sum::<usize>() > 1024 * 1024
        || t.comment.len() > 20000
        || t.comment
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        || t.ranges.iter().any(|r| r.end_ms > p.duration_ms)
        || t.frame_time_ms
            .is_some_and(|at| !t.ranges.iter().any(|r| at >= r.start_ms && at < r.end_ms))
        || p.warnings.len() > 16
        || p.warnings
            .iter()
            .any(|w| w.trim().is_empty() || w.len() > 8000)
    {
        return Err("摘录来源、范围或材料大小无效，未读取损坏内容".into());
    }
    let mut ids = HashSet::new();
    let mut ranges: Vec<super::note_nodes::TimeRange> = Vec::new();
    let mut start = 0;
    for cue in &t.cues {
        if cue.id.is_empty()
            || cue.id.len() > 80
            || cue.id.chars().any(char::is_control)
            || !ids.insert(&cue.id)
            || Some(&cue.subtitle_id) != t.subtitle_id.as_ref()
            || cue.start_ms < start
            || cue.start_ms >= cue.end_ms
            || cue.end_ms > p.duration_ms
        {
            return Err("摘录字幕身份或范围无效".into());
        }
        start = cue.start_ms;
        if let Some(last) = ranges.last_mut().filter(|r| cue.start_ms <= r.end_ms) {
            last.end_ms = last.end_ms.max(cue.end_ms);
        } else {
            ranges.push(super::note_nodes::TimeRange {
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
            });
        }
    }
    if (!t.cues.is_empty() && ranges != t.ranges)
        || super::excerpts::document(t, &p.source_sha256)? != p.document
    {
        return Err("摘录正文与完整来源不一致".into());
    }
    if let Some(frame) = &p.frame {
        if Some(frame.time_ms) != t.frame_time_ms
            || frame.data_url.len() > 350000
            || !digest(&frame.sha256)
        {
            return Err("摘录截图身份无效".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(
                frame
                    .data_url
                    .strip_prefix("data:image/jpeg;base64,")
                    .ok_or("摘录截图类型无效")?,
            )
            .map_err(|_| "摘录截图编码无效")?;
        if bytes.len() > 256 * 1024 || hash(&bytes) != frame.sha256 {
            return Err("摘录截图超限或摘要不符".into());
        }
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(1024);
        limits.max_image_height = Some(1024);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().map_err(|_| "摘录截图无法解码或尺寸超限")?;
        if image.width() == 0 || image.height() == 0 {
            return Err("摘录截图尺寸无效".into());
        }
    } else if t.frame_time_ms.is_some() && p.warnings.is_empty() {
        return Err("缺少截图失败提示，未静默丢弃图片".into());
    }
    Ok(())
}

pub fn save_prepared(
    db: &Connection,
    base: &Path,
    library: &str,
    prepared: &PreparedExcerpt,
) -> Result<StoredExcerpt, String> {
    save_prepared_guarded(db, base, library, prepared, |tx| tx.commit().map_err(error))
}
pub fn save_prepared_guarded(
    db: &Connection,
    base: &Path,
    library: &str,
    prepared: &PreparedExcerpt,
    publish: impl FnOnce(Transaction<'_>) -> Result<(), String>,
) -> Result<StoredExcerpt, String> {
    validate(prepared)?;
    if identity(base, library)? != prepared.root_hash || library != prepared.library_id {
        return Err("摘录不属于此素材库位置".into());
    }
    let raw = serde_json::to_string(prepared).map_err(error)?;
    if raw.len() > MAX_PAYLOAD {
        return Err("摘录快照超过16MiB，未截断保存".into());
    }
    let sha = hash(&raw);
    // Snapshot bytes are prepared before the short write transaction.
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(error)?;
    let text = &prepared.text;
    let current = super::store::course(&tx, &text.course_id)?;
    let file = super::analysis::source_file_id(&tx, &current.media_id)?;
    let registered: Option<String> = tx
        .query_row(
            "SELECT checksum FROM media_files WHERE id=?1",
            [&file],
            |r| r.get(0),
        )
        .map_err(error)?;
    if current.media_id != text.media_id
        || current.title != text.title
        || current.duration_ms != Some(prepared.duration_ms)
        || file != prepared.source_file_id
        || registered
            .as_ref()
            .is_some_and(|s| !s.is_empty() && s != &prepared.source_sha256)
    {
        return Err("课程或原件登记已变化，请重新准备摘录".into());
    }
    let verified = super::excerpts::prepare_text(
        &tx,
        base,
        &text.course_id,
        &ExcerptSelection {
            subtitle_id: text.subtitle_id.clone(),
            cue_ids: text.cues.iter().map(|c| c.id.clone()).collect(),
            ranges: text.ranges.clone(),
            comment: text.comment.clone(),
            frame_time_ms: text.frame_time_ms,
        },
    )?;
    if serde_json::to_value(verified).map_err(error)?
        != serde_json::to_value(text).map_err(error)?
    {
        return Err("摘录字幕原件已变化，未保存旧材料".into());
    }
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM learning_excerpts WHERE payload_sha256=?1 AND status='ready'",
            [&sha],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)?;
    let stored = if let Some(id) = existing {
        let stored = get_preview(&tx, base, library, &id)?;
        if serde_json::to_string(&stored.prepared).map_err(error)? != raw {
            return Err("摘录内容摘要冲突".into());
        }
        stored
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO learning_excerpts(id,library_id,root_hash,course_id,media_id,source_file_id,payload_json,payload_sha256,status,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'ready',?9)", params![id,library,prepared.root_hash,text.course_id,text.media_id,prepared.source_file_id,raw,sha,chrono::Utc::now().to_rfc3339()]).map_err(error)?;
        StoredExcerpt {
            id,
            status: "ready".into(),
            prepared: prepared.clone(),
        }
    };
    publish(tx)?;
    Ok(stored)
}

pub fn get_preview(
    db: &Connection,
    base: &Path,
    library: &str,
    id: &str,
) -> Result<StoredExcerpt, String> {
    uuid(id)?;
    let root = identity(base, library)?;
    let (raw, sha, status, binding): (String, String, String, [String; 5]) = db.query_row(
        "SELECT payload_json,payload_sha256,status,library_id,root_hash,course_id,media_id,source_file_id FROM learning_excerpts WHERE id=?1 AND length(CAST(payload_json AS BLOB))<=?2",
        params![id, MAX_PAYLOAD], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, [r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?])),
    ).map_err(|_| "摘录不存在或记录超限")?;
    if hash(&raw) != sha {
        return Err("摘录记录摘要不符，原笔记未改动".into());
    }
    let prepared: PreparedExcerpt = serde_json::from_str(&raw).map_err(|_| "摘录记录损坏")?;
    validate(&prepared)?;
    if binding
        != [
            prepared.library_id.clone(),
            prepared.root_hash.clone(),
            prepared.text.course_id.clone(),
            prepared.text.media_id.clone(),
            prepared.source_file_id.clone(),
        ]
        || !matches!(status.as_str(), "ready" | "cancelled" | "committed")
        || (status == "ready" && (prepared.library_id != library || prepared.root_hash != root))
    {
        return Err("摘录来源登记或素材库位置已变化".into());
    }
    Ok(StoredExcerpt {
        id: id.into(),
        status,
        prepared,
    })
}

pub fn cancel_preview(
    db: &Connection,
    base: &Path,
    library: &str,
    id: &str,
) -> Result<StoredExcerpt, String> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(error)?;
    let mut stored = get_preview(&tx, base, library, id)?;
    if stored.status == "committed" {
        return Err("摘录已保存，不能取消历史回执".into());
    }
    let pending: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM learning_excerpt_operations WHERE excerpt_id=?1 AND status='pending')", [id], |r| r.get(0)).map_err(error)?;
    if pending {
        return Err("摘录有未完成的发布操作，请先恢复或取消该操作".into());
    }
    tx.execute(
        "UPDATE learning_excerpts SET status='cancelled' WHERE id=?1 AND status='ready'",
        [id],
    )
    .map_err(error)?;
    tx.commit().map_err(error)?;
    stored.status = "cancelled".into();
    Ok(stored)
}

pub(crate) fn invalidate_previews(db: &Connection) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(error)?;
    tx.execute(
        "UPDATE learning_excerpt_operations SET status='cancelled' WHERE status='pending'",
        [],
    )
    .map_err(error)?;
    tx.execute(
        "UPDATE learning_excerpts SET status='cancelled' WHERE status='ready'",
        [],
    )
    .map_err(error)?;
    tx.commit().map_err(error)
}
