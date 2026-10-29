//! Internal provenance; not an authorization to read or import source data.
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub(super) struct CourseOriginKey {
    profile: String,
    encoded: String,
}

pub(super) struct SeriesOriginKey(CourseOriginKey);

impl SeriesOriginKey {
    pub(super) fn new(profile: &str, source_id: &str) -> Result<Self, String> {
        let mut key = CourseOriginKey::new(profile, source_id)?;
        key.encoded = serde_json::to_string(&[profile, "series", source_id])
            .map_err(|_| "系列来源身份无效")?;
        Ok(Self(key))
    }
}

pub(super) fn inspect_series(
    db: &Connection,
    key: &SeriesOriginKey,
    hash: &str,
) -> Result<CourseOriginState, String> {
    inspect_kind(db, &key.0, hash, "series")
}

pub(super) fn record_series_in(
    db: &Connection,
    key: &SeriesOriginKey,
    series: &str,
    hash: &str,
    epoch: &str,
) -> Result<(), String> {
    record_kind(db, &key.0, series, hash, epoch, "series")
}

impl CourseOriginKey {
    pub(super) fn new(profile: &str, source_id: &str) -> Result<Self, String> {
        if !super::source_profiles::valid_uuid(profile)
            || source_id.is_empty()
            || source_id.len() > 4096
            || source_id.chars().any(char::is_control)
        {
            return Err("课程来源身份无效".into());
        }
        let encoded = serde_json::to_string(&[profile, "course", source_id])
            .map_err(|_| "课程来源身份无效")?;
        Ok(Self {
            profile: profile.into(),
            encoded,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(super) enum CourseOriginState {
    Unmapped,
    Unchanged(String),
    SourceChanged(String),
    TargetChanged(String),
    BothChanged(String),
    TargetUnavailable(String),
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}

fn target_snapshot(db: &Connection, id: &str) -> Result<Option<(i64, String)>, String> {
    let media: Option<(String, String, String, i64)> = db.query_row("SELECT e.id,e.primary_file_id,f.checksum,f.file_size FROM learning_courses c JOIN media_entries e ON e.id=c.media_entry_id JOIN media_files f ON f.id=e.primary_file_id AND f.entry_id=e.id AND f.role='main' WHERE c.id=?1 AND e.kind='video' AND e.deleted_at IS NULL", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional().map_err(|_| "无法检查目标课程")?;
    let Some(media) = media else {
        return Ok(None);
    };
    let course = super::store::course(db, id).map_err(|_| "目标课程不可读取")?;
    let mut stmt = db.prepare("SELECT s.id,s.revision,m.position FROM learning_series_courses m JOIN learning_series s ON s.id=m.series_id WHERE m.course_id=?1 ORDER BY s.id,m.position").map_err(|_| "目标系列不可读取")?;
    let memberships = stmt
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(|_| "目标系列不可读取")?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "目标系列不可读取")?;
    let bytes = serde_json::to_vec(&("dcc.course-origin.target.v1", &course, media, memberships))
        .map_err(|_| "无法检查目标课程")?;
    Ok(Some((
        course.progress_revision,
        format!("{:x}", Sha256::digest(bytes)),
    )))
}

pub(super) fn inspect(
    db: &Connection,
    key: &CourseOriginKey,
    hash: &str,
) -> Result<CourseOriginState, String> {
    inspect_kind(db, key, hash, "course")
}

fn target_snapshot_kind(
    db: &Connection,
    id: &str,
    kind: &str,
) -> Result<Option<(i64, String)>, String> {
    if kind == "course" {
        return target_snapshot(db, id);
    }
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM learning_series WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| "无法检查目标系列")?;
    if !exists {
        return Ok(None);
    }
    let series = super::store::series(db, id).map_err(|_| "目标系列不可读取")?;
    let bytes = serde_json::to_vec(&("dcc.series-origin.target.v1", &series))
        .map_err(|_| "目标系列不可读取")?;
    Ok(Some((
        series.revision,
        format!("{:x}", Sha256::digest(bytes)),
    )))
}

fn inspect_kind(
    db: &Connection,
    key: &CourseOriginKey,
    hash: &str,
    expected_kind: &str,
) -> Result<CourseOriginState, String> {
    if !valid_hash(hash) {
        return Err("课程来源摘要无效".into());
    }
    super::source_profiles::get_in(db, &key.profile)?;
    let stored: Option<(String,String,String,i64,String)> = db.query_row("SELECT target_kind,target_id,source_sha256,target_revision,target_sha256 FROM learning_origin_map WHERE source_profile_id=?1 AND source_key_json=?2", params![key.profile,key.encoded], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|_| "无法检查课程来源")?;
    let Some((kind, id, old_source, revision, old_target)) = stored else {
        return Ok(CourseOriginState::Unmapped);
    };
    if kind != expected_kind
        || !super::source_profiles::valid_uuid(&id)
        || !valid_hash(&old_source)
        || !valid_hash(&old_target)
        || revision < 0
    {
        return Err("课程来源记录损坏".into());
    }
    let Some((current_revision, current_target)) = target_snapshot_kind(db, &id, expected_kind)?
    else {
        return Ok(CourseOriginState::TargetUnavailable(id));
    };
    Ok(
        match (
            hash != old_source,
            current_revision != revision || current_target != old_target,
        ) {
            (false, false) => CourseOriginState::Unchanged(id),
            (true, false) => CourseOriginState::SourceChanged(id),
            (false, true) => CourseOriginState::TargetChanged(id),
            (true, true) => CourseOriginState::BothChanged(id),
        },
    )
}

/// Fingerprint the stored mapping, not just its conflict category.
pub(super) fn preview_token(
    db: &Connection,
    key: &CourseOriginKey,
    hash: &str,
) -> Result<String, String> {
    preview_token_kind(db, key, hash, "course")
}

pub(super) fn preview_series_token(
    db: &Connection,
    key: &SeriesOriginKey,
    hash: &str,
) -> Result<String, String> {
    preview_token_kind(db, &key.0, hash, "series")
}

fn preview_token_kind(
    db: &Connection,
    key: &CourseOriginKey,
    hash: &str,
    kind: &str,
) -> Result<String, String> {
    let _ = inspect_kind(db, key, hash, kind)?;
    let mapping: Option<(String,String,String,i64,String)> = db.query_row("SELECT target_kind,target_id,source_sha256,target_revision,target_sha256 FROM learning_origin_map WHERE source_profile_id=?1 AND source_key_json=?2", params![key.profile,key.encoded], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|_| "无法检查课程来源")?;
    let target = match &mapping {
        Some((_, id, _, _, _)) => target_snapshot_kind(db, id, kind)?,
        None => None,
    };
    let bytes = serde_json::to_vec(&(
        "dcc.learning-origin.preview.v1",
        kind,
        &key.encoded,
        hash,
        mapping,
        target,
    ))
    .map_err(|_| "无法检查课程来源")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Only call in the same transaction that publishes the confirmed target course.
pub(super) fn record_in(
    db: &Connection,
    key: &CourseOriginKey,
    course: &str,
    hash: &str,
    epoch: &str,
) -> Result<(), String> {
    record_kind(db, key, course, hash, epoch, "course")
}

fn record_kind(
    db: &Connection,
    key: &CourseOriginKey,
    target: &str,
    hash: &str,
    epoch: &str,
    kind: &str,
) -> Result<(), String> {
    if db.is_autocommit() || !super::source_profiles::valid_uuid(target) {
        return Err("课程来源登记需要有效事务和目标".into());
    }
    if super::source_profiles::get_in(db, &key.profile)?.access_epoch != epoch {
        return Err("来源状态已变化，请重新检查".into());
    }
    let unavailable = if kind == "series" {
        "目标系列不可用"
    } else {
        "目标课程不可用"
    };
    let occupied = if kind == "series" {
        "系列来源已关联或无法登记"
    } else {
        "课程来源已关联或无法登记"
    };
    let (revision, target_hash) = target_snapshot_kind(db, target, kind)?.ok_or(unavailable)?;
    match inspect_kind(db, key, hash, kind)? {
        CourseOriginState::Unmapped => db.execute("INSERT INTO learning_origin_map(source_profile_id,source_key_json,target_kind,target_id,source_sha256,target_revision,target_sha256) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![key.profile,key.encoded,kind,target,hash,revision,target_hash]).map(|_| ()).map_err(|_| occupied.into()),
        CourseOriginState::Unchanged(existing) if existing == target => Ok(()),
        _ => Err("课程来源或目标已变化，请重新检查".into()),
    }
}

pub(super) fn stored_in(
    db: &Connection,
    key: &CourseOriginKey,
) -> Result<Option<(String, String)>, String> {
    stored_kind(db, key, "course")
}

pub(super) fn stored_series_in(
    db: &Connection,
    key: &SeriesOriginKey,
) -> Result<Option<(String, String)>, String> {
    stored_kind(db, &key.0, "series")
}

fn stored_kind(
    db: &Connection,
    key: &CourseOriginKey,
    kind: &str,
) -> Result<Option<(String, String)>, String> {
    db.query_row(
        "SELECT target_id,source_sha256 FROM learning_origin_map WHERE source_profile_id=?1 AND source_key_json=?2 AND target_kind=?3",
        params![key.profile, key.encoded, kind],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(|_| "无法检查课程来源".into())
}

/// Drop provenance only inside the revert transaction that deletes this exact target.
pub(super) fn forget_in(
    db: &Connection,
    key: &CourseOriginKey,
    course: &str,
) -> Result<(), String> {
    forget_kind(db, key, course, "course")
}

pub(super) fn forget_series_in(
    db: &Connection,
    key: &SeriesOriginKey,
    series: &str,
) -> Result<(), String> {
    forget_kind(db, &key.0, series, "series")
}

fn forget_kind(
    db: &Connection,
    key: &CourseOriginKey,
    target: &str,
    kind: &str,
) -> Result<(), String> {
    if db.is_autocommit() {
        return Err("课程来源撤销需要有效事务".into());
    }
    let removed = db
        .execute(
            "DELETE FROM learning_origin_map WHERE source_profile_id=?1 AND source_key_json=?2 AND target_kind=?3 AND target_id=?4",
            params![key.profile, key.encoded, kind, target],
        )
        .map_err(|_| "无法撤销课程来源")?;
    if removed != 1 {
        return Err("课程来源已变化，无法撤销".into());
    }
    Ok(())
}
