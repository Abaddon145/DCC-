//! Isolated synthetic SQLite snapshots; no user-source selection or IPC exists here.
use rusqlite::{
    backup::{Backup, StepResult},
    config::DbConfig,
    Connection, OpenFlags,
};
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

/// Open a synthetic source for isolated snapshots.
///
/// On Windows, when `-shm` already exists, hold it with `FILE_SHARE_READ` only before the
/// plain `READ_ONLY` open. That stops the reader from writing or truncating SHM while
/// still allowing online Backup. (`readonly_shm=1` freezes SHM for queries but hangs
/// Backup on the bundled SQLite — do not use it here.)
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

/// Byte-copy an existing source file while allowing concurrent writers (`FILE_SHARE_READ |
/// FILE_SHARE_WRITE`, no delete share). Used only to build an isolated sidecar set; the
/// source is never opened by SQLite on this path.
#[cfg(windows)]
fn copy_existing_sidecar_shared(src: &Path, dst: &Path) -> Result<(), String> {
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
    fs::write(dst, bytes).map_err(|_| failed())
}

/// Copy a complete live DB/WAL/SHM trio into `dest_db` (+ `-wal` / `-shm`) under shared-read
/// leases. Fail-closed if any sidecar is missing; does not create missing source files and
/// does not open SQLite against the live source. Torn copies under concurrent writers remain
/// a residual — callers should copy after a committed quiet point when possible.
#[cfg(windows)]
pub(super) fn copy_complete_sidecars_for_snapshot(
    source_db: &Path,
    dest_db: &Path,
) -> Result<(), String> {
    let failed = || "合成来源快照不可用，请重试".to_string();
    let source_wal = synthetic_sidecar_path(source_db, "-wal");
    let source_shm = synthetic_sidecar_path(source_db, "-shm");
    if !source_db.is_file() || !source_wal.is_file() || !source_shm.is_file() {
        return Err(failed());
    }
    if let Some(parent) = dest_db.parent() {
        fs::create_dir_all(parent).map_err(|_| failed())?;
    }
    let dest_wal = synthetic_sidecar_path(dest_db, "-wal");
    let dest_shm = synthetic_sidecar_path(dest_db, "-shm");
    copy_existing_sidecar_shared(source_db, dest_db)?;
    copy_existing_sidecar_shared(&source_wal, &dest_wal)?;
    copy_existing_sidecar_shared(&source_shm, &dest_shm)?;
    Ok(())
}

/// Safe concurrent path while a live writer may hold source SHM writable: copy complete
/// sidecars out, then run the SHM deny-write snapshot against the copy only. Source files are
/// not opened by SQLite, so this reader cannot mutate source SHM. Not a product grant.
#[cfg(windows)]
pub(super) fn snapshot_synthetic_via_sidecar_copy(
    source_db: &Path,
    dest_db: &Path,
    timeout: Duration,
) -> Result<Connection, String> {
    copy_complete_sidecars_for_snapshot(source_db, dest_db)?;
    snapshot_synthetic(dest_db, timeout)
}

pub(super) fn snapshot_synthetic(path: &Path, timeout: Duration) -> Result<Connection, String> {
    let source = open_synthetic_readonly_source(path)?;
    snapshot_synthetic_connection(source.connection(), timeout)
}

pub(super) fn snapshot_synthetic_connection(
    source: &Connection,
    timeout: Duration,
) -> Result<Connection, String> {
    let failed = || "合成来源快照不可用，请重试".to_string();
    let started = Instant::now();
    // SQLite may otherwise checkpoint the WAL when the last connection closes.
    if !source
        .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .map_err(|_| failed())?
    {
        return Err(failed());
    }
    source.busy_timeout(Duration::ZERO).map_err(|_| failed())?;
    let page_size: i64 = source
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .map_err(|_| failed())?;
    let mut snapshot = Connection::open_in_memory().map_err(|_| failed())?;
    snapshot
        .pragma_update(None, "page_size", page_size)
        .map_err(|_| failed())?;
    {
        let backup = Backup::new(source, &mut snapshot).map_err(|_| failed())?;
        loop {
            if started.elapsed() >= timeout {
                return Err(failed());
            }
            match backup.step(64).map_err(|_| failed())? {
                StepResult::Done => break,
                StepResult::More => continue,
                StepResult::Busy | StepResult::Locked => return Err(failed()),
                _ => return Err(failed()),
            }
        }
    }
    let integrity: String = snapshot
        .query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))
        .map_err(|_| failed())?;
    let user_version: i64 = snapshot
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| failed())?;
    let (count, min, max): (i64, i64, i64) = snapshot
        .query_row(
            "SELECT count(*), min(version), max(version) FROM schema_version",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| failed())?;
    let foreign_key_error = snapshot
        .prepare("PRAGMA foreign_key_check")
        .map_err(|_| failed())?
        .query([])
        .map_err(|_| failed())?
        .next()
        .map_err(|_| failed())?
        .is_some();
    if integrity != "ok" || user_version != 8 || (count, min, max) != (1, 8, 8) || foreign_key_error
    {
        return Err(failed());
    }
    Ok(snapshot)
}
