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
/// plain `READ_ONLY` open. That stops the reader from writing or truncating closed-writer
/// SHM while still allowing online Backup. (`readonly_shm=1` also freezes SHM bytes for
/// queries, but hangs Backup on the bundled SQLite — do not use it for this path.)
///
/// Not a product grant. If an active writer already holds SHM writable, the deny-write
/// lease is skipped and SHM may still change; that residual remains open.
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
            // Sharing violation means an active writer (or incompatible lease) already
            // holds SHM writable; fall back to plain READ_ONLY and accept SHM may change.
            options.open(shm_path).ok()
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
