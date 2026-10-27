//! Library-scoped provenance identities. No source path or access permission is stored here.
use crate::state::AppState;
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryScope {
    library_id: String,
    root_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProfile {
    pub id: String,
    pub application: String,
    pub access_epoch: String,
    pub created_at: String,
}

fn valid_uuid(value: &str) -> bool {
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
            Ok(LibraryScope {
                library_id,
                root_hash,
            })
        })?
        .ok_or_else(|| "素材库已切换".into())
}

fn scoped<T>(
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
    if !valid_uuid(id) {
        return Err("来源身份无效".into());
    }
    scoped(state, expected, |db| {
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
    })
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
