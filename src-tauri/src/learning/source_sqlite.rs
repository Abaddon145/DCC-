//! Internal local file identity. A separate user selection is required for any source access.
use super::source_profiles::{self, LibraryScope};
use crate::state::AppState;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use std::{
    fs::File,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
};

pub(super) struct SqliteSelection {
    scope: LibraryScope,
    profile: String,
    epoch: String,
    path: PathBuf,
    file: File,
    _directories: Vec<File>,
}

pub(super) struct SqliteCandidate {
    pub(super) path: PathBuf,
    pub(super) file: File,
    directories: Vec<File>,
    identity: String,
}

/// Checks only the SQLite database header, not its contents or integrity.
pub(super) fn validate_database_header(database: &[u8]) -> Result<u32, String> {
    super::course_snapshot::validate_database_header(database)
}

/// Reject invalid WAL header dimensions before passing a snapshot to SQLite.
/// This structural check does not establish integrity or authorization.
pub(super) fn validate_wal_headers(
    database: &[u8],
    wal: &[u8],
    wal_len: u64,
) -> Result<(), String> {
    super::course_snapshot::validate_wal_headers(database, wal, wal_len)
}

fn file_identity(file: &File) -> Result<String, String> {
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0
        || info.nNumberOfLinks != 1
        || (info.nFileIndexHigh == 0 && info.nFileIndexLow == 0)
        || (info.ftCreationTime.dwHighDateTime == 0 && info.ftCreationTime.dwLowDateTime == 0)
    {
        return Err("无法确认来源数据库身份".into());
    }
    Ok(format!(
        "win2:{:08x}:{:08x}{:08x}:{:08x}{:08x}",
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
        info.ftCreationTime.dwHighDateTime,
        info.ftCreationTime.dwLowDateTime
    ))
}

/// Caller must obtain explicit independent permission for `path` before invoking this.
/// The returned identity is local metadata, not permission to read or migrate the database.
pub(super) fn bind_selected(
    state: &AppState,
    scope: &LibraryScope,
    path: &Path,
) -> Result<SqliteSelection, String> {
    source_profiles::scoped(state, scope, |_| Ok(()))?;
    let candidate = open_candidate(path)?;
    source_profiles::scoped(state, scope, |db| bind_candidate_in(db, scope, candidate))
}

pub(super) fn open_candidate(path: &Path) -> Result<SqliteCandidate, String> {
    let name = path.file_name().ok_or("来源数据库无效")?;
    let parent = path.parent().ok_or("来源数据库无效")?;
    let (root, directories) = super::source_markdown::lock_vault_root(parent)?;
    let path = root.join(name);
    let file = super::source_markdown::open_checked_shared(&path)?;
    let identity = file_identity(&file)?;
    Ok(SqliteCandidate {
        path,
        file,
        directories,
        identity,
    })
}

pub(super) fn bind_candidate_in(
    db: &rusqlite::Connection,
    scope: &LibraryScope,
    candidate: SqliteCandidate,
) -> Result<SqliteSelection, String> {
    let identity = &candidate.identity;
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)
        .map_err(|_| "无法登记来源数据库")?;
    let stored: Option<String> = tx
        .query_row(
            "SELECT source_profile_id FROM learning_source_sqlite_bindings WHERE file_identity=?1",
            [&identity],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "无法检查来源数据库身份")?;
    let profile = if let Some(id) = stored {
        source_profiles::get_in(&tx, &id)?
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        let epoch = uuid::Uuid::new_v4().to_string();
        let created = chrono::Utc::now().to_rfc3339();
        tx.execute("INSERT INTO learning_source_profiles(id,application,access_epoch,created_at) VALUES(?1,'yingliu',?2,?3)", params![id,epoch,created]).map_err(|_| "无法保存来源身份")?;
        tx.execute("INSERT INTO learning_source_sqlite_bindings(source_profile_id,file_identity) VALUES(?1,?2)", params![id,identity]).map_err(|_| "无法登记来源数据库")?;
        source_profiles::get_in(&tx, &id)?
    };
    if file_identity(&candidate.file)? != candidate.identity {
        return Err("来源数据库身份已变化".into());
    }
    tx.commit().map_err(|_| "无法保存来源数据库身份")?;
    Ok(SqliteSelection {
        scope: scope.clone(),
        profile: profile.id,
        epoch: profile.access_epoch,
        path: candidate.path,
        file: candidate.file,
        _directories: candidate.directories,
    })
}

impl SqliteSelection {
    pub(super) fn profile_id(&self) -> &str {
        &self.profile
    }
    pub(super) fn epoch(&self) -> &str {
        &self.epoch
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn check(&self, state: &AppState) -> Result<(), String> {
        source_profiles::scoped(state, &self.scope, |db| {
            check_file_binding_in(db, &self.profile, &self.epoch, &self.file)
        })
    }
}

pub(super) fn check_file_binding_in(
    db: &rusqlite::Connection,
    profile: &str,
    epoch: &str,
    file: &File,
) -> Result<(), String> {
    if source_profiles::get_in(db, profile)?.access_epoch != epoch {
        return Err("来源状态已变化，请重新选择".into());
    }
    let identity = file_identity(file)?;
    let stored = source_profiles::sqlite_binding_in(db, profile)?;
    if stored.as_deref() != Some(&identity) {
        return Err("来源数据库身份已变化".into());
    }
    Ok(())
}
