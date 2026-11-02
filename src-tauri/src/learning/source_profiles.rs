//! Library-scoped provenance identities. No source path or access permission is stored here.
use crate::state::AppState;
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryScope {
    library_id: String,
    root_hash: String,
    session_nonce: Uuid,
}

impl LibraryScope {
    pub(crate) fn library_id(&self) -> &str {
        &self.library_id
    }

    pub(crate) fn root_hash(&self) -> &str {
        &self.root_hash
    }

    pub(crate) fn session_nonce(&self) -> Uuid {
        self.session_nonce
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProfile {
    pub id: String,
    pub application: String,
    pub access_epoch: String,
    pub created_at: String,
}

pub(super) fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value)
        .is_ok_and(|uuid| !uuid.is_nil() && uuid.hyphenated().to_string() == value)
}

/// Capture the active library identity for a later scoped operation.
pub fn scope(state: &AppState, expected_library_id: &str) -> Result<LibraryScope, String> {
    state
        .with_library_if_id(expected_library_id, |_, base| {
            if state.tasks.maintenance().is_some() {
                return Err("素材库正在维护，请稍后重试".into());
            }
            let (library_id, root_hash) = super::analysis_store::library_identity(base)?;
            if library_id != expected_library_id {
                return Err("素材库身份已变化".into());
            }
            let session_nonce = *state
                .source_scope_nonce
                .lock()
                .map_err(|_| "来源会话状态不可用")?;
            Ok(LibraryScope {
                library_id,
                root_hash,
                session_nonce,
            })
        })?
        .ok_or_else(|| "素材库已切换".into())
}

pub(super) fn scoped<T>(
    state: &AppState,
    expected: &LibraryScope,
    f: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    state
        .with_library_if_id(&expected.library_id, |db, base| {
            if state.tasks.maintenance().is_some() {
                return Err("素材库正在维护，请稍后重试".into());
            }
            let current = super::analysis_store::library_identity(base)?;
            if current != (expected.library_id.clone(), expected.root_hash.clone()) {
                return Err("素材库身份已变化".into());
            }
            if *state
                .source_scope_nonce
                .lock()
                .map_err(|_| "来源会话状态不可用")?
                != expected.session_nonce
            {
                return Err("素材库会话已变化，请重新检查来源".into());
            }
            f(db)
        })?
        .ok_or_else(|| "素材库已切换".into())
}

/// Creates provenance metadata only; it grants no access to external files.
pub fn create(state: &AppState, expected: &LibraryScope) -> Result<SourceProfile, String> {
    scoped(state, expected, |db| {
        let profile = SourceProfile {
            id: Uuid::new_v4().to_string(),
            application: "yingliu".into(),
            access_epoch: Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        db.execute("INSERT INTO learning_source_profiles(id,application,access_epoch,created_at) VALUES(?1,?2,?3,?4)",
            [&profile.id, &profile.application, &profile.access_epoch, &profile.created_at])
            .map_err(|_| "无法保存来源身份")?;
        Ok(profile)
    })
}

pub fn get(state: &AppState, expected: &LibraryScope, id: &str) -> Result<SourceProfile, String> {
    scoped(state, expected, |db| get_in(db, id))
}

pub(super) fn get_in(db: &Connection, id: &str) -> Result<SourceProfile, String> {
    if !valid_uuid(id) {
        return Err("来源身份无效".into());
    }
    let profile = db.query_row("SELECT id,application,access_epoch,created_at FROM learning_source_profiles WHERE id=?1", [id], |row| {
        Ok(SourceProfile { id:row.get(0)?, application:row.get(1)?, access_epoch:row.get(2)?, created_at:row.get(3)? })
    }).optional().map_err(|_| "来源身份不可读取")?.ok_or("来源身份不存在")?;
    if !valid_uuid(&profile.id)
        || !valid_uuid(&profile.access_epoch)
        || profile.application != "yingliu"
        || chrono::DateTime::parse_from_rfc3339(&profile.created_at).is_err()
    {
        return Err("来源身份记录损坏".into());
    }
    Ok(profile)
}

pub(super) fn sqlite_binding_in(db: &Connection, id: &str) -> Result<Option<String>, String> {
    let binding: Option<String> = db
        .query_row(
            "SELECT file_identity FROM learning_source_sqlite_bindings WHERE source_profile_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "无法检查来源数据库身份")?;
    if let Some(value) = binding.as_deref() {
        let parts: Vec<_> = value.split(':').collect();
        let hex = |part: &str| {
            part.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if parts.len() != 4
            || parts[0] != "win2"
            || parts[1].len() != 8
            || parts[2].len() != 16
            || parts[3].len() != 16
            || !parts[1..].iter().all(|part| hex(part))
            || parts[2] == "0000000000000000"
            || parts[3] == "0000000000000000"
        {
            return Err("来源数据库身份记录损坏".into());
        }
    }
    Ok(binding)
}

/// Restore keeps provenance but invalidates any future local grants bound to an old epoch.
pub fn invalidate_epochs(db: &Connection) -> Result<(), String> {
    let epoch = Uuid::new_v4().to_string();
    db.execute(
        "UPDATE learning_source_profiles SET access_epoch=?1",
        [epoch],
    )
    .map_err(|_| "无法使来源访问授权失效")?;
    Ok(())
}
