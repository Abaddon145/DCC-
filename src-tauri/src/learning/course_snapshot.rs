//! Offline copy-only source access. SQLite never opens the source path.
use rusqlite::{
    backup::{Backup, StepResult},
    config::DbConfig,
    Connection,
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[cfg(windows)]
pub(super) struct LockedSnapshot {
    pub connection: Connection,
    pub database: fs::File,
    paths: Vec<PathBuf>,
    _files: Vec<fs::File>,
    _directories: Vec<fs::File>,
}

#[cfg(windows)]
impl LockedSnapshot {
    pub(super) fn recheck(&self) -> Result<(), String> {
        let absent = |file: &Path| matches!(fs::symlink_metadata(file),Err(error) if error.kind()==std::io::ErrorKind::NotFound);
        if !absent(&sidecar(&self.paths[0], "-journal"))
            || (self.paths.len() == 1
                && (!absent(&sidecar(&self.paths[0], "-wal"))
                    || !absent(&sidecar(&self.paths[0], "-shm"))))
        {
            return Err("课程来源状态已变化，请重试".into());
        }
        for path in &self.paths {
            super::source_markdown::open_checked_deny_write(path)?;
        }
        Ok(())
    }
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}

#[cfg(windows)]
pub(super) fn open(path: &Path) -> Result<LockedSnapshot, String> {
    let failed = || "无法生成课程来源副本，请关闭映流后重试".to_string();
    let started = Instant::now();
    let timeout = Duration::from_secs(15);
    let (root, directories) =
        super::source_markdown::lock_vault_root(path.parent().ok_or_else(failed)?)?;
    let db = root.join(path.file_name().ok_or_else(failed)?);
    let absent = |file: &Path| matches!(fs::symlink_metadata(file),Err(error) if error.kind()==std::io::ErrorKind::NotFound);
    let journal = sidecar(&db, "-journal");
    if !absent(&journal) {
        return Err(failed());
    }
    let paths = [db.clone(), sidecar(&db, "-wal"), sidecar(&db, "-shm")];
    let has_wal = !absent(&paths[1]);
    let has_shm = !absent(&paths[2]);
    if has_wal != has_shm {
        return Err(failed());
    }
    let mut files = paths
        .iter()
        .take(if has_wal { 3 } else { 1 })
        .map(|p| super::source_markdown::open_checked_deny_write(p))
        .collect::<Result<Vec<_>, _>>()?;
    let temp = tempfile::tempdir().map_err(|_| failed())?;
    let copied = temp.path().join("snapshot.db");
    let mut total = 0u64;
    for (index, file) in files.iter_mut().enumerate() {
        let size = file.metadata().map_err(|_| failed())?.len();
        total = total.checked_add(size).ok_or_else(failed)?;
        if total > 256 * 1024 * 1024 {
            return Err("课程来源副本超过 256 MiB 上限".into());
        }
        let target = match index {
            0 => copied.clone(),
            1 => sidecar(&copied, "-wal"),
            _ => sidecar(&copied, "-shm"),
        };
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(|_| failed())?;
        let mut read = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            if started.elapsed() >= timeout {
                return Err(failed());
            }
            let count = file.read(&mut buffer).map_err(|_| failed())?;
            if count == 0 {
                break;
            }
            read += count as u64;
            if read > size {
                return Err(failed());
            }
            output.write_all(&buffer[..count]).map_err(|_| failed())?;
        }
        if read != size || file.metadata().map_err(|_| failed())?.len() != size {
            return Err(failed());
        }
    }
    if !absent(&journal) || (!has_wal && (!absent(&paths[1]) || !absent(&paths[2]))) {
        return Err(failed());
    }
    let mut header = [0u8; 100];
    fs::File::open(&copied)
        .and_then(|mut f| f.read_exact(&mut header))
        .map_err(|_| failed())?;
    super::source_sqlite::validate_database_header(&header)?;
    if has_wal {
        let mut wal = fs::File::open(sidecar(&copied, "-wal")).map_err(|_| failed())?;
        let len = wal.metadata().map_err(|_| failed())?.len();
        let mut header_wal = [0u8; 32];
        if len > 0 {
            wal.read_exact(&mut header_wal).map_err(|_| failed())?
        }
        super::source_sqlite::validate_wal_headers(&header, &header_wal, len)?;
    } else if header[18] != header[19] {
        return Err(failed());
    }
    // The copied database is disposable. SQLite may create or recover sidecars only here.
    let local = Connection::open(&copied).map_err(|_| failed())?;
    let connection = snapshot_connection(
        &local,
        timeout.checked_sub(started.elapsed()).ok_or_else(failed)?,
        256 * 1024 * 1024,
    )?;
    drop(local);
    if !absent(&journal) {
        return Err(failed());
    }
    let database = files.remove(0);
    Ok(LockedSnapshot {
        connection,
        database,
        paths: paths
            .into_iter()
            .take(if has_wal { 3 } else { 1 })
            .collect(),
        _files: files,
        _directories: directories,
    })
}

pub(super) fn snapshot_connection(
    source: &Connection,
    timeout: Duration,
    max_bytes: u64,
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
    let page_size = u64::try_from(page_size).map_err(|_| failed())?;
    let within_limit = |pages: i64| {
        u64::try_from(pages)
            .ok()
            .and_then(|count| count.checked_mul(page_size))
            .is_some_and(|bytes| bytes <= max_bytes)
    };
    let pages: i64 = source
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .map_err(|_| failed())?;
    if !within_limit(pages) {
        return Err("合成来源快照超过大小上限".into());
    }
    let mut snapshot = Connection::open_in_memory().map_err(|_| failed())?;
    snapshot
        .progress_handler(1000, Some(move || started.elapsed() >= timeout))
        .map_err(|_| failed())?;
    snapshot
        .pragma_update(None, "page_size", page_size)
        .map_err(|_| failed())?;
    {
        let backup = Backup::new(source, &mut snapshot).map_err(|_| failed())?;
        loop {
            if started.elapsed() >= timeout {
                return Err(failed());
            }
            let result = backup.step(64).map_err(|_| failed())?;
            if !within_limit(i64::from(backup.progress().pagecount)) {
                return Err("合成来源快照超过大小上限".into());
            }
            match result {
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
    snapshot
        .progress_handler(0, None::<fn() -> bool>)
        .map_err(|_| failed())?;
    if started.elapsed() >= timeout {
        return Err(failed());
    }
    Ok(snapshot)
}
