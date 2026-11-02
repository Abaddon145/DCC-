//! Isolated synthetic SQLite snapshots; no user-source selection or IPC exists here.
use rusqlite::{config::DbConfig, Connection, OpenFlags};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::fs;

/// Synthetic read-only connection plus an optional Windows `-shm` deny-write lease.
/// The lease must outlive the connection so SQLite cannot reopen `-shm` for write/truncate.
pub(super) struct SyntheticReadonlySource {
    connection: Connection,
    #[cfg(windows)]
    _shm_deny_write: Option<std::fs::File>,
}

impl SyntheticReadonlySource {
    pub(super) fn connection(&self) -> &Connection {
        &self.connection
    }
}

#[cfg(windows)]
pub(super) struct SyntheticReadonlyShmChild {
    connection: Connection,
    _files: Vec<fs::File>,
    _directories: Vec<fs::File>,
}

#[cfg(windows)]
impl SyntheticReadonlyShmChild {
    pub(super) fn connection(&self) -> &Connection {
        &self.connection
    }
}

/// Test-only: the caller must be a separate process with no existing source connection.
#[cfg(windows)]
pub(super) fn open_synthetic_readonly_shm_child(
    path: &Path,
) -> Result<SyntheticReadonlyShmChild, String> {
    use std::io::Read;
    let failed = || "合成来源快照不可用".to_string();
    let name = path.file_name().ok_or_else(failed)?;
    let parent = path.parent().ok_or_else(failed)?;
    let (root, directories) = super::source_markdown::lock_vault_root(parent)?;
    let db = root.join(name);
    let mut files = [
        db.clone(),
        synthetic_sidecar_path(&db, "-wal"),
        synthetic_sidecar_path(&db, "-shm"),
    ]
    .iter()
    .map(|file| super::source_markdown::open_checked_shared(file))
    .collect::<Result<Vec<_>, _>>()?;
    if !synthetic_journal_absent(&db) {
        return Err(failed());
    }
    let mut database_header = [0u8; 100];
    files[0]
        .read_exact(&mut database_header)
        .map_err(|_| failed())?;
    let wal_size = files[1].metadata().map_err(|_| failed())?.len();
    let mut wal_header = [0u8; 32];
    if wal_size != 0 {
        files[1].read_exact(&mut wal_header).map_err(|_| failed())?;
    }
    super::source_sqlite::validate_wal_headers(&database_header, &wal_header, wal_size)?;
    let mut uri = url::Url::from_file_path(path).map_err(|_| failed())?;
    uri.query_pairs_mut()
        .append_pair("mode", "ro")
        .append_pair("readonly_shm", "1");
    let connection = Connection::open_with_flags(
        uri.as_str(),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| failed())?;
    if !connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .map_err(|_| failed())?
    {
        return Err(failed());
    }
    Ok(SyntheticReadonlyShmChild {
        connection,
        _files: files,
        _directories: directories,
    })
}

/// Open a synthetic source for isolated snapshots.
///
/// On Windows, when `-shm` already exists, hold it with `FILE_SHARE_READ` only before the
/// plain `READ_ONLY` open. That stops the reader from writing or truncating SHM while
/// still allowing online Backup. `readonly_shm=1` does not protect SHM when a writable
/// connection in this process already owns the shared mapping. Do not use it here.
///
/// Fail-closed: if `-shm` exists and the deny-write lease cannot be taken (typical when
/// an active writer already holds SHM writable), return an error and do not open SQLite.
/// Silent plain-`READ_ONLY` fallback is forbidden because it can mutate SHM. Not a
/// product grant; concurrent live-writer reads need a different, explicitly unsafe path.
pub(super) fn open_synthetic_readonly_source(
    path: &Path,
) -> Result<SyntheticReadonlySource, String> {
    let failed = || "合成来源快照不可用，请重试".to_string();
    #[cfg(windows)]
    let shm_deny_write = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let mut shm = path.as_os_str().to_os_string();
        shm.push("-shm");
        let shm_path = Path::new(&shm);
        if shm_path.is_file() {
            let mut options = fs::OpenOptions::new();
            options.read(true).share_mode(FILE_SHARE_READ);
            Some(options.open(shm_path).map_err(|_| failed())?)
        } else {
            None
        }
    };
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| failed())?;
    Ok(SyntheticReadonlySource {
        connection,
        #[cfg(windows)]
        _shm_deny_write: shm_deny_write,
    })
}

fn synthetic_sidecar_path(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut out = path.as_os_str().to_os_string();
    out.push(suffix);
    std::path::PathBuf::from(out)
}

#[cfg(windows)]
fn synthetic_journal_absent(path: &Path) -> bool {
    matches!(fs::symlink_metadata(synthetic_sidecar_path(path, "-journal")), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

#[cfg(windows)]
fn synthetic_sidecars_absent(path: &Path) -> bool {
    ["-wal", "-shm", "-journal"].iter().all(|suffix| {
        matches!(fs::symlink_metadata(synthetic_sidecar_path(path, suffix)), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    })
}

/// Synthetic-only: missing names cannot be locked, so this is not a source grant.
#[cfg(windows)]
fn snapshot_quiet_synthetic(
    db: &Path,
    timeout: Duration,
    interfere: impl FnOnce(),
) -> Result<Connection, String> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let failed = || "合成来源快照不可用，请重试".to_string();
    let started = Instant::now();
    let mut source = super::source_markdown::open_checked_deny_write(db)?;
    if !synthetic_sidecars_absent(db) {
        return Err(failed());
    }
    let mut header = [0u8; 100];
    source.read_exact(&mut header).map_err(|_| failed())?;
    super::source_sqlite::validate_database_header(&header)?;
    if header[18] != header[19] {
        return Err(failed());
    }
    interfere();
    if !synthetic_sidecars_absent(db) {
        return Err(failed());
    }
    source.seek(SeekFrom::Start(0)).map_err(|_| failed())?;
    let temp = tempfile::tempdir().map_err(|_| failed())?;
    let copied = temp.path().join("snapshot.db");
    let mut output = fs::File::create(&copied).map_err(|_| failed())?;
    let mut buffer = [0u8; 65536];
    loop {
        if started.elapsed() >= timeout {
            return Err(failed());
        }
        let count = source.read(&mut buffer).map_err(|_| failed())?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count]).map_err(|_| failed())?;
    }
    drop(output);
    if !synthetic_sidecars_absent(db) {
        return Err(failed());
    }
    let result = snapshot_synthetic(
        &copied,
        timeout.checked_sub(started.elapsed()).ok_or_else(failed)?,
    )?;
    if !synthetic_sidecars_absent(db) {
        return Err(failed());
    }
    Ok(result)
}

#[cfg(windows)]
#[derive(Clone, PartialEq, Eq)]
struct SidecarFingerprint {
    len: u64,
    sha256: [u8; 32],
}

/// Shared-read open + full-file SHA-256 for an existing sidecar. Fail-closed if missing.
#[cfg(windows)]
fn fingerprint_sidecar_shared(path: &Path) -> Result<SidecarFingerprint, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
    let failed = || "合成来源快照不可用，请重试".to_string();
    if !path.is_file() {
        return Err(failed());
    }
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    let mut file = options.open(path).map_err(|_| failed())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut len = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|_| failed())?;
        if n == 0 {
            break;
        }
        len = len.saturating_add(n as u64);
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut sha256 = [0u8; 32];
    sha256.copy_from_slice(&digest);
    Ok(SidecarFingerprint { len, sha256 })
}

#[cfg(windows)]
fn fingerprint_complete_sidecars(
    source_db: &Path,
) -> Result<(SidecarFingerprint, SidecarFingerprint, SidecarFingerprint), String> {
    let wal = synthetic_sidecar_path(source_db, "-wal");
    let shm = synthetic_sidecar_path(source_db, "-shm");
    Ok((
        fingerprint_sidecar_shared(source_db)?,
        fingerprint_sidecar_shared(&wal)?,
        fingerprint_sidecar_shared(&shm)?,
    ))
}

/// Byte-copy an existing source file while allowing concurrent writers (`FILE_SHARE_READ |
/// FILE_SHARE_WRITE`, no delete share). Returns the fingerprint of the bytes written.
#[cfg(windows)]
fn copy_existing_sidecar_shared(src: &Path, dst: &Path) -> Result<SidecarFingerprint, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
    let failed = || "合成来源快照不可用，请重试".to_string();
    if !src.is_file() {
        return Err(failed());
    }
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    let mut file = options.open(src).map_err(|_| failed())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|_| failed())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut sha256 = [0u8; 32];
    sha256.copy_from_slice(&digest);
    fs::write(dst, &bytes).map_err(|_| failed())?;
    Ok(SidecarFingerprint {
        len: bytes.len() as u64,
        sha256,
    })
}

/// Copy a complete live DB/WAL/SHM trio into `dest_db` under shared-read leases, then
/// re-fingerprint the live sources. Fail-closed if any sidecar is missing or if any of
/// DB/WAL/SHM changed during the copy (torn / mid-write). Does not open SQLite on the source.
#[cfg(windows)]
pub(super) fn copy_complete_sidecars_for_snapshot(
    source_db: &Path,
    dest_db: &Path,
) -> Result<(), String> {
    copy_complete_sidecars_for_snapshot_with_interference(source_db, dest_db, |_| {})
}

/// Testable copy path: `interfere` runs after the pre-copy fingerprint and before byte copies,
/// so tests can mutate a live sidecar and prove torn copies fail closed.
#[cfg(windows)]
pub(super) fn copy_complete_sidecars_for_snapshot_with_interference(
    source_db: &Path,
    dest_db: &Path,
    interfere: impl FnOnce(&Path),
) -> Result<(), String> {
    let failed = || "合成来源快照不可用，请重试".to_string();
    let source_wal = synthetic_sidecar_path(source_db, "-wal");
    let source_shm = synthetic_sidecar_path(source_db, "-shm");
    if !source_db.is_file() || !source_wal.is_file() || !source_shm.is_file() {
        return Err(failed());
    }
    let before = fingerprint_complete_sidecars(source_db)?;
    interfere(source_db);
    if let Some(parent) = dest_db.parent() {
        fs::create_dir_all(parent).map_err(|_| failed())?;
    }
    let dest_wal = synthetic_sidecar_path(dest_db, "-wal");
    let dest_shm = synthetic_sidecar_path(dest_db, "-shm");
    let copied_db = copy_existing_sidecar_shared(source_db, dest_db)?;
    let copied_wal = copy_existing_sidecar_shared(&source_wal, &dest_wal)?;
    let copied_shm = copy_existing_sidecar_shared(&source_shm, &dest_shm)?;
    // Copied bytes must match the pre-copy quiet fingerprint (detect change mid-copy).
    if (copied_db, copied_wal, copied_shm) != before {
        let _ = fs::remove_file(dest_db);
        let _ = fs::remove_file(&dest_wal);
        let _ = fs::remove_file(&dest_shm);
        return Err(failed());
    }
    let after = fingerprint_complete_sidecars(source_db)?;
    if after != before {
        let _ = fs::remove_file(dest_db);
        let _ = fs::remove_file(&dest_wal);
        let _ = fs::remove_file(&dest_shm);
        return Err(failed());
    }
    Ok(())
}

const SIDECAR_COPY_ATTEMPTS: u32 = 3;

#[cfg(windows)]
struct SyntheticSqliteSelection {
    scope: super::source_profiles::LibraryScope,
    profile: String,
    epoch: String,
    path: std::path::PathBuf,
    _directories: Vec<fs::File>,
    file: fs::File,
}

#[cfg(windows)]
#[derive(Default)]
struct SyntheticSqliteSelectionState {
    generation: String,
    selected: Option<std::sync::Arc<SyntheticSqliteSelection>>,
}

/// Test-only model: a simulated picker is required; source identity is not persistent authorization.
#[cfg(windows)]
#[derive(Default)]
pub(super) struct SyntheticSqliteSelections(std::sync::Mutex<SyntheticSqliteSelectionState>);

#[cfg(windows)]
pub(super) struct SyntheticCoursePreview {
    ticket: String,
    preview: super::course_apply::CourseStagePreview,
}

#[cfg(windows)]
impl SyntheticSqliteSelections {
    pub(super) fn revoke(&self) -> Result<(), String> {
        let mut current = self.0.lock().map_err(|_| "来源会话不可用")?;
        current.generation = uuid::Uuid::new_v4().to_string();
        current.selected = None;
        Ok(())
    }

    pub(super) fn select(
        &self,
        state: &crate::state::AppState,
        library: &str,
        profile: &str,
        epoch: &str,
        picker: impl FnOnce() -> Option<std::path::PathBuf>,
    ) -> Result<Option<String>, String> {
        use super::source_profiles;
        let scope = source_profiles::scope(state, library)?;
        if source_profiles::get(state, &scope, profile)?.access_epoch != epoch {
            return Err("来源状态已变化".into());
        }
        let ticket = uuid::Uuid::new_v4().to_string();
        {
            let mut current = self.0.lock().map_err(|_| "来源会话不可用")?;
            current.generation = ticket.clone();
            current.selected = None;
        }
        let picked = picker();
        if self.0.lock().map_err(|_| "来源会话不可用")?.generation != ticket {
            return Err("来源选择已变化".into());
        }
        let Some(path) = picked else {
            return Ok(None);
        };
        let name = path.file_name().ok_or("来源文件无效")?;
        let parent = path.parent().ok_or("来源文件无效")?;
        let (root, directories) = super::source_markdown::lock_vault_root(parent)?;
        let path = root.join(name);
        let file = super::source_markdown::open_checked_shared(&path)?;
        // This test-only registry always takes its selection lock before the library lock.
        let mut current = self.0.lock().map_err(|_| "来源会话不可用")?;
        if current.generation != ticket {
            return Err("来源选择已变化".into());
        }
        source_profiles::scoped(state, &scope, |db| {
            super::source_sqlite::check_file_binding_in(db, profile, epoch, &file)?;
            current.selected = Some(std::sync::Arc::new(SyntheticSqliteSelection {
                scope: scope.clone(),
                profile: profile.into(),
                epoch: epoch.into(),
                path,
                _directories: directories,
                file,
            }));
            Ok(Some(ticket))
        })
    }

    fn check(
        &self,
        state: &crate::state::AppState,
        ticket: &str,
        selection: &SyntheticSqliteSelection,
    ) -> Result<(), String> {
        let current = self.0.lock().map_err(|_| "来源会话不可用")?;
        if current.generation != ticket || current.selected.is_none() {
            return Err("来源选择已变化".into());
        }
        super::source_profiles::scoped(state, &selection.scope, |db| {
            super::source_sqlite::check_file_binding_in(
                db,
                &selection.profile,
                &selection.epoch,
                &selection.file,
            )
        })
    }

    /// Test-only, read-only callback. The callback must not write target data.
    pub(super) fn inspect_isolated<T>(
        &self,
        state: &crate::state::AppState,
        ticket: &str,
        inspect: impl FnOnce(&Path) -> Result<T, String>,
    ) -> Result<T, String> {
        let selection = {
            let current = self.0.lock().map_err(|_| "来源会话不可用")?;
            if current.generation != ticket {
                return Err("来源选择已变化".into());
            }
            current.selected.clone().ok_or("请先选择来源文件")?
        };
        self.check(state, ticket, &selection)?;
        let sidecars = [
            synthetic_sidecar_path(&selection.path, "-wal"),
            synthetic_sidecar_path(&selection.path, "-shm"),
        ];
        let _leases = sidecars
            .iter()
            .map(|path| super::source_markdown::open_checked_shared(path))
            .collect::<Result<Vec<_>, _>>()?;
        if !synthetic_journal_absent(&selection.path) {
            return Err("合成来源快照不可用".into());
        }
        let result = inspect(&selection.path)?;
        for path in &sidecars {
            super::source_markdown::open_checked_shared(path)?;
        }
        if !synthetic_journal_absent(&selection.path) {
            return Err("合成来源快照不可用".into());
        }
        self.check(state, ticket, &selection)?;
        Ok(result)
    }

    pub(super) fn snapshot(
        &self,
        state: &crate::state::AppState,
        ticket: &str,
        timeout: Duration,
        interfere: impl FnOnce(),
    ) -> Result<Connection, String> {
        let selection = {
            let current = self.0.lock().map_err(|_| "来源会话不可用")?;
            if current.generation != ticket {
                return Err("来源选择已变化".into());
            }
            current.selected.clone().ok_or("请先选择来源文件")?
        };
        self.check(state, ticket, &selection)?;
        let result = snapshot_pinned_synthetic(&selection.path, timeout, interfere)?;
        self.check(state, ticket, &selection)?;
        Ok(result)
    }

    // Read-only inspection only: a later failed check cannot undo callback writes.
    pub(super) fn inspect<T>(
        &self,
        state: &crate::state::AppState,
        ticket: &str,
        timeout: Duration,
        inspect: impl FnOnce(&Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let selection = {
            let current = self.0.lock().map_err(|_| "来源会话不可用")?;
            if current.generation != ticket {
                return Err("来源选择已变化".into());
            }
            current.selected.clone().ok_or("请先选择来源文件")?
        };
        let snapshot = self.snapshot(state, ticket, timeout, || {})?;
        let result = inspect(&snapshot)?;
        self.check(state, ticket, &selection)?;
        Ok(result)
    }

    pub(super) fn course_preview(
        &self,
        state: &crate::state::AppState,
        ticket: &str,
        scope: &super::source_profiles::LibraryScope,
        bindings: &[super::import_course_preview::CourseMediaBinding],
        timeout: Duration,
    ) -> Result<SyntheticCoursePreview, String> {
        let selection = {
            let current = self.0.lock().map_err(|_| "来源会话不可用")?;
            if current.generation != ticket {
                return Err("来源选择已变化".into());
            }
            current.selected.clone().ok_or("请先选择来源文件")?
        };
        if selection.scope != *scope {
            return Err("素材库会话已变化".into());
        }
        let preview = self.inspect(state, ticket, timeout, |snapshot| {
            super::course_apply::preview_course_batch(
                state,
                scope,
                &selection.profile,
                snapshot,
                bindings,
            )
        })?;
        Ok(SyntheticCoursePreview {
            ticket: ticket.into(),
            preview,
        })
    }

    /// Test-only guard: callbacks must not re-enter this registry.
    pub(super) fn course_write<T>(
        &self,
        state: &crate::state::AppState,
        approved: &SyntheticCoursePreview,
        timeout: Duration,
        before_lock: impl FnOnce(),
        write: impl FnOnce(&Connection, &super::course_apply::CourseStagePreview) -> Result<T, String>,
    ) -> Result<T, String> {
        let started = Instant::now();
        self.snapshot(state, &approved.ticket, timeout, || {})?;
        before_lock();
        let current = self.0.lock().map_err(|_| "来源会话不可用")?;
        if current.generation != approved.ticket {
            return Err("来源选择已变化".into());
        }
        let selection = current.selected.as_ref().ok_or("请先选择来源文件")?;
        super::source_profiles::scoped(state, &selection.scope, |db| {
            super::source_sqlite::check_file_binding_in(
                db,
                &selection.profile,
                &selection.epoch,
                &selection.file,
            )
        })?;
        let paths = [
            selection.path.clone(),
            synthetic_sidecar_path(&selection.path, "-wal"),
            synthetic_sidecar_path(&selection.path, "-shm"),
        ];
        let leases = paths
            .iter()
            .map(|path| super::source_markdown::open_checked_deny_write(path))
            .collect::<Result<Vec<_>, _>>()?;
        let remaining = timeout
            .checked_sub(started.elapsed())
            .ok_or("来源快照超时")?;
        let snapshot = snapshot_pinned_synthetic(&selection.path, remaining, || {})?;
        super::source_profiles::scoped(state, &selection.scope, |db| {
            super::source_sqlite::check_file_binding_in(
                db,
                &selection.profile,
                &selection.epoch,
                &selection.file,
            )
        })?;
        let result = write(&snapshot, &approved.preview);
        drop(leases);
        drop(current);
        result
    }
}

/// Experimental, test-only pinned copy. This is not a user-source grant.
#[cfg(windows)]
pub(super) fn snapshot_pinned_synthetic(
    path: &Path,
    timeout: Duration,
    interfere: impl FnOnce(),
) -> Result<Connection, String> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom, Write};
    let failed = || "合成来源快照不可用，请重试".to_string();
    let started = Instant::now();
    let name = path.file_name().ok_or_else(failed)?;
    let parent = path.parent().ok_or_else(failed)?;
    let (root, _directories) = super::source_markdown::lock_vault_root(parent)?;
    let db = root.join(name);
    if synthetic_sidecars_absent(&db) {
        return snapshot_quiet_synthetic(&db, timeout, interfere);
    }
    if !synthetic_journal_absent(&db) {
        return Err(failed());
    }
    let paths = [
        db.clone(),
        synthetic_sidecar_path(&db, "-wal"),
        synthetic_sidecar_path(&db, "-shm"),
    ];
    let mut files = paths
        .iter()
        .map(|p| super::source_markdown::open_checked_shared(p))
        .collect::<Result<Vec<_>, _>>()?;
    let fingerprint = |file: &mut fs::File,
                       mut output: Option<&mut fs::File>|
     -> Result<([u8; 32], u64), String> {
        file.seek(SeekFrom::Start(0)).map_err(|_| failed())?;
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut buf = [0u8; 65536];
        loop {
            if started.elapsed() >= timeout {
                return Err(failed());
            }
            let n = file.read(&mut buf).map_err(|_| failed())?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            size = size.checked_add(n as u64).ok_or_else(failed)?;
            if let Some(out) = output.as_deref_mut() {
                out.write_all(&buf[..n]).map_err(|_| failed())?;
            }
        }
        Ok((hash.finalize().into(), size))
    };
    let before = files
        .iter_mut()
        .map(|file| fingerprint(file, None))
        .collect::<Result<Vec<_>, _>>()?;
    interfere();
    if !synthetic_journal_absent(&db) {
        return Err(failed());
    }
    let temp = tempfile::tempdir().map_err(|_| failed())?;
    let copied = temp.path().join("snapshot.db");
    let destinations = [
        copied.clone(),
        synthetic_sidecar_path(&copied, "-wal"),
        synthetic_sidecar_path(&copied, "-shm"),
    ];
    for (index, file) in files.iter_mut().enumerate() {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destinations[index])
            .map_err(|_| failed())?;
        if fingerprint(file, Some(&mut output))? != before[index] {
            return Err(failed());
        }
    }
    for (index, file) in files.iter_mut().enumerate() {
        if fingerprint(file, None)? != before[index] {
            return Err(failed());
        }
    }
    let mut db_header = [0u8; 100];
    fs::File::open(&copied)
        .and_then(|mut f| f.read_exact(&mut db_header))
        .map_err(|_| failed())?;
    let mut wal_header = [0u8; 32];
    let mut wal = fs::File::open(&destinations[1]).map_err(|_| failed())?;
    let wal_len = wal.metadata().map_err(|_| failed())?.len();
    if wal_len != 0 {
        wal.read_exact(&mut wal_header).map_err(|_| failed())?;
    }
    super::source_sqlite::validate_wal_headers(&db_header, &wal_header, wal_len)?;
    drop(wal);
    let snapshot = snapshot_synthetic(
        &copied,
        timeout.checked_sub(started.elapsed()).ok_or_else(failed)?,
    )?;
    if !synthetic_journal_absent(&db) {
        return Err(failed());
    }
    Ok(snapshot)
}

/// Safe concurrent path: copy complete sidecars with before/after SHA-256+size consistency,
/// retrying a few times if the live writer mutates mid-copy, then deny-write snapshot the
/// copy only. Fail-closed when every attempt sees a torn/inconsistent set. Not a product grant.
#[cfg(windows)]
pub(super) fn snapshot_synthetic_via_sidecar_copy(
    source_db: &Path,
    dest_db: &Path,
    timeout: Duration,
) -> Result<Connection, String> {
    let failed = || "合成来源快照不可用，请重试".to_string();
    let mut last_err = failed();
    for _ in 0..SIDECAR_COPY_ATTEMPTS {
        match copy_complete_sidecars_for_snapshot(source_db, dest_db) {
            Ok(()) => return snapshot_synthetic(dest_db, timeout),
            Err(err) => {
                last_err = err;
                let _ = fs::remove_file(dest_db);
                let _ = fs::remove_file(synthetic_sidecar_path(dest_db, "-wal"));
                let _ = fs::remove_file(synthetic_sidecar_path(dest_db, "-shm"));
            }
        }
    }
    Err(last_err)
}

pub(super) fn snapshot_synthetic(path: &Path, timeout: Duration) -> Result<Connection, String> {
    let source = open_synthetic_readonly_source(path)?;
    snapshot_synthetic_connection(source.connection(), timeout)
}

pub(super) fn snapshot_synthetic_connection(
    source: &Connection,
    timeout: Duration,
) -> Result<Connection, String> {
    // ponytail: 256 MiB page-count limit; use bounded selective extraction for larger sources.
    snapshot_synthetic_connection_with_limit(source, timeout, 256 * 1024 * 1024)
}

pub(super) fn snapshot_synthetic_connection_with_limit(
    source: &Connection,
    timeout: Duration,
    max_bytes: u64,
) -> Result<Connection, String> {
    super::course_snapshot::snapshot_connection(source, timeout, max_bytes)
}
