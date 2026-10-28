//! Internal provenance only. The caller must authorize and verify source bytes separately.
use super::source_profiles;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub(super) struct NoteOriginKey {
    profile: String,
    encoded: String,
}

impl NoteOriginKey {
    pub(super) fn new(profile: &str, vault: &str, path: &str) -> Result<Self, String> {
        let valid_text = |value: &str| {
            !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
        };
        if !source_profiles::valid_uuid(profile)
            || !valid_text(vault)
            || !valid_text(path)
            || path.contains(['\\', ':'])
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err("笔记来源身份无效".into());
        }
        let encoded = serde_json::to_string(&[profile, "markdown", vault, path])
            .map_err(|_| "笔记来源身份无效")?;
        Ok(Self {
            profile: profile.into(),
            encoded,
        })
    }

    pub(super) fn encoded(&self) -> &str {
        &self.encoded
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum NoteOriginState {
    Unmapped,
    Unchanged(String),
    SourceChanged(String),
    TargetChanged(String),
    BothChanged(String),
    TargetUnavailable(String),
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn note_snapshot(db: &Connection, id: &str) -> Result<Option<(i64, String)>, String> {
    let deleted: Option<Option<String>> = db
        .query_row(
            "SELECT deleted_at FROM knowledge_notes WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "无法检查目标笔记")?;
    if !matches!(deleted, Some(None)) {
        return Ok(None);
    }
    let note = crate::knowledge::get(db, id).map_err(|_| "目标笔记不可读取")?;
    let bytes = serde_json::to_vec(&note).map_err(|_| "目标笔记不可读取")?;
    Ok(Some((
        note.revision,
        format!("{:x}", Sha256::digest(bytes)),
    )))
}

fn check_epoch(db: &Connection, key: &NoteOriginKey, expected_epoch: &str) -> Result<(), String> {
    let profile = source_profiles::get_in(db, &key.profile)?;
    if profile.access_epoch != expected_epoch {
        return Err("来源状态已变化，请重新检查".into());
    }
    Ok(())
}

pub(super) fn inspect_scoped(
    state: &crate::state::AppState,
    scope: &source_profiles::LibraryScope,
    key: &NoteOriginKey,
    expected_epoch: &str,
    source_hash: &str,
) -> Result<NoteOriginState, String> {
    source_profiles::scoped(state, scope, |db| {
        check_epoch(db, key, expected_epoch)?;
        inspect(db, key, source_hash)
    })
}

/// Reads only target database metadata; no source file is opened or verified.
pub(super) fn inspect(
    db: &Connection,
    key: &NoteOriginKey,
    source_hash: &str,
) -> Result<NoteOriginState, String> {
    if !valid_hash(source_hash) {
        return Err("来源摘要无效".into());
    }
    source_profiles::get_in(db, &key.profile)?;
    let stored: Option<(String, String, String, i64, String)> = db.query_row(
        "SELECT target_kind,target_id,source_sha256,target_revision,target_sha256 FROM learning_origin_map WHERE source_profile_id=?1 AND source_key_json=?2",
        params![key.profile, key.encoded], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
    ).optional().map_err(|_| "无法检查笔记来源")?;
    let Some((kind, id, previous_source, revision, previous_target)) = stored else {
        return Ok(NoteOriginState::Unmapped);
    };
    if kind != "note"
        || !source_profiles::valid_uuid(&id)
        || !valid_hash(&previous_source)
        || revision < 0
        || !valid_hash(&previous_target)
    {
        return Err("笔记来源记录损坏".into());
    }
    let Some((current_revision, current_target)) = note_snapshot(db, &id)? else {
        return Ok(NoteOriginState::TargetUnavailable(id));
    };
    let source_changed = source_hash != previous_source;
    let target_changed = current_revision != revision || current_target != previous_target;
    Ok(match (source_changed, target_changed) {
        (false, false) => NoteOriginState::Unchanged(id),
        (true, false) => NoteOriginState::SourceChanged(id),
        (false, true) => NoteOriginState::TargetChanged(id),
        (true, true) => NoteOriginState::BothChanged(id),
    })
}

/// Record only inside the transaction that publishes the verified target note.
pub(super) fn record_note_in(
    db: &Connection,
    key: &NoteOriginKey,
    note_id: &str,
    source_hash: &str,
    expected_epoch: &str,
) -> Result<(), String> {
    if db.is_autocommit() || !source_profiles::valid_uuid(note_id) {
        return Err("笔记来源登记需要有效事务和目标".into());
    }
    check_epoch(db, key, expected_epoch)?;
    let (revision, target_hash) = note_snapshot(db, note_id)?.ok_or("目标笔记不可用")?;
    match inspect(db,key,source_hash)? {
        NoteOriginState::Unmapped => db.execute(
            "INSERT INTO learning_origin_map(source_profile_id,source_key_json,target_kind,target_id,source_sha256,target_revision,target_sha256) VALUES(?1,?2,'note',?3,?4,?5,?6)",
            params![key.profile,key.encoded,note_id,source_hash,revision,target_hash],
        ).map(|_| ()).map_err(|_| "笔记来源已关联或无法登记".into()),
        NoteOriginState::Unchanged(existing) if existing == note_id => Ok(()),
        _ => Err("笔记来源或目标已变化，请重新检查".into()),
    }
}
