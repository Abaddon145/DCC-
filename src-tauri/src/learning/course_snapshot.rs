//! Offline copy-only source access. SQLite never opens the source path.
//!
//! `open_isolated_readonly` is the synthetic gate used by course UI commands: a sibling
//! marker file is required, sidecars are fail-closed, and SQLite is opened only on a
//! temporary copy. On Windows, an existing `-shm` is held deny-write before that open.
//! `open` remains the older Windows lease helper for tests and is not the course UI path.
use rusqlite::{
    backup::{Backup, StepResult},
    config::DbConfig,
    Connection, OpenFlags,
};
use sha2::{Digest, Sha256};
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

#[cfg(windows)]
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

/// Sibling marker required before any synthetic course source is read.
pub(super) const SYNTHETIC_COURSE_MARKER: &str = "dcc-synthetic-course-source.ok";
pub(super) const SYNTHETIC_COURSE_MARKER_BODY: &[u8] = b"dcc-synthetic-course-source-v1\n";
const ISOLATED_LIMIT: u64 = 256 * 1024 * 1024;

fn unmarked() -> String {
    "真实课程来源尚未授权，仅允许带合成标记的只读样本".into()
}

fn isolate_failed() -> String {
    "合成课程来源无法只读隔离，请重试".into()
}

/// Checks only the SQLite database header, not its contents or integrity.
pub(super) fn validate_database_header(database: &[u8]) -> Result<u32, String> {
    let failed = || "来源数据库或 WAL 文件头无效".to_string();
    if database.len() < 100
        || &database[..16] != b"SQLite format 3\0"
        || !matches!(database[18], 1 | 2)
        || !matches!(database[19], 1 | 2)
    {
        return Err(failed());
    }
    let encoded = u16::from_be_bytes([database[16], database[17]]);
    let page_size = if encoded == 1 {
        65536
    } else {
        u32::from(encoded)
    };
    if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
        return Err(failed());
    }
    Ok(page_size)
}

/// Reject invalid WAL header dimensions before passing a snapshot to SQLite.
/// This structural check does not establish integrity or authorization.
pub(super) fn validate_wal_headers(
    database: &[u8],
    wal: &[u8],
    wal_len: u64,
) -> Result<(), String> {
    let failed = || "来源数据库或 WAL 文件头无效".to_string();
    let page_size = validate_database_header(database)?;
    if database[18] != 2 || database[19] != 2 {
        return Err(failed());
    }
    if wal_len == 0 {
        return Ok(());
    }
    if wal_len < 32 || wal.len() < 32 {
        return Err(failed());
    }
    let value = |offset: usize| {
        u32::from_be_bytes([
            wal[offset],
            wal[offset + 1],
            wal[offset + 2],
            wal[offset + 3],
        ])
    };
    if !matches!(value(0), 0x377f0682 | 0x377f0683) || value(4) != 3007000 || value(8) != page_size
    {
        return Err(failed());
    }
    Ok(())
}

fn single_link(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::MetadataExt::nlink(meta) == 1
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::MetadataExt::number_of_links(meta) == 1
    }
}

/// Refuse an unmarked path before opening or hashing the database file.
pub(super) fn require_synthetic_marker(database: &Path) -> Result<(), String> {
    if !database.is_absolute()
        || database
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(unmarked());
    }
    let parent = database.parent().ok_or_else(unmarked)?;
    let parent_meta = fs::symlink_metadata(parent).map_err(|_| unmarked())?;
    if !parent_meta.file_type().is_dir() {
        return Err(unmarked());
    }
    let marker = parent.join(SYNTHETIC_COURSE_MARKER);
    let meta = fs::symlink_metadata(&marker).map_err(|_| unmarked())?;
    if !meta.file_type().is_file()
        || !single_link(&meta)
        || meta.len() != SYNTHETIC_COURSE_MARKER_BODY.len() as u64
    {
        return Err(unmarked());
    }
    let mut file = fs::File::open(&marker).map_err(|_| unmarked())?;
    let mut body = Vec::new();
    file.read_to_end(&mut body).map_err(|_| unmarked())?;
    if body != SYNTHETIC_COURSE_MARKER_BODY {
        return Err(unmarked());
    }
    Ok(())
}

struct IsolatedBundle {
    database: Vec<u8>,
    wal: Option<Vec<u8>>,
    shm: Option<Vec<u8>>,
    digest: [u8; 32],
}

fn sidecar_name(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn read_regular(
    path: &Path,
    budget: &mut u64,
    started: Instant,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    if started.elapsed() >= timeout {
        return Err(isolate_failed());
    }
    let meta = fs::symlink_metadata(path).map_err(|_| isolate_failed())?;
    if !meta.file_type().is_file() || !single_link(&meta) {
        return Err(isolate_failed());
    }
    let len = meta.len();
    if len > *budget {
        return Err("合成课程来源超过 256 MiB 上限".into());
    }
    *budget = budget.saturating_sub(len);
    let mut file = fs::File::open(path).map_err(|_| isolate_failed())?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 65536];
    let mut read = 0u64;
    loop {
        if started.elapsed() >= timeout {
            return Err(isolate_failed());
        }
        let count = file.read(&mut chunk).map_err(|_| isolate_failed())?;
        if count == 0 {
            break;
        }
        read = read.checked_add(count as u64).ok_or_else(isolate_failed)?;
        if read > len {
            return Err(isolate_failed());
        }
        buf.extend_from_slice(&chunk[..count]);
    }
    if read != len || file.metadata().map_err(|_| isolate_failed())?.len() != len {
        return Err(isolate_failed());
    }
    Ok(buf)
}

fn optional_regular(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        _ => Err(isolate_failed()),
    }
}

fn read_bundle(path: &Path, started: Instant, timeout: Duration) -> Result<IsolatedBundle, String> {
    require_synthetic_marker(path)?;
    let journal = sidecar_name(path, "-journal");
    if fs::symlink_metadata(&journal).is_ok() {
        return Err(isolate_failed());
    }
    let wal_path = sidecar_name(path, "-wal");
    let shm_path = sidecar_name(path, "-shm");
    let has_wal = optional_regular(&wal_path)?;
    let has_shm = optional_regular(&shm_path)?;
    if has_wal != has_shm {
        return Err(isolate_failed());
    }
    let mut budget = ISOLATED_LIMIT;
    let database = read_regular(path, &mut budget, started, timeout)?;
    validate_database_header(&database)?;
    let wal = if has_wal {
        let bytes = read_regular(&wal_path, &mut budget, started, timeout)?;
        validate_wal_headers(&database, &bytes, bytes.len() as u64)?;
        Some(bytes)
    } else if database.get(18) != database.get(19) {
        return Err(isolate_failed());
    } else {
        None
    };
    let shm = if has_shm {
        Some(read_regular(&shm_path, &mut budget, started, timeout)?)
    } else {
        None
    };
    let mut hasher = Sha256::new();
    hasher.update(SYNTHETIC_COURSE_MARKER_BODY);
    hasher.update([u8::from(wal.is_some()), u8::from(shm.is_some())]);
    hasher.update((database.len() as u64).to_le_bytes());
    hasher.update(&database);
    if let Some(bytes) = &wal {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    if let Some(bytes) = &shm {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    let digest = hasher.finalize();
    let mut sha = [0u8; 32];
    sha.copy_from_slice(&digest);
    Ok(IsolatedBundle {
        database,
        wal,
        shm,
        digest: sha,
    })
}

/// Hash a marked synthetic source without opening SQLite.
pub(super) fn fingerprint_synthetic(path: &Path) -> Result<[u8; 32], String> {
    let started = Instant::now();
    Ok(read_bundle(path, started, Duration::from_secs(15))?.digest)
}

fn write_copy(
    path: &Path,
    bytes: &[u8],
    started: Instant,
    timeout: Duration,
) -> Result<(), String> {
    if started.elapsed() >= timeout {
        return Err(isolate_failed());
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| isolate_failed())?;
    output.write_all(bytes).map_err(|_| isolate_failed())?;
    Ok(())
}

fn open_copied(path: &Path, has_wal: bool) -> Result<Connection, String> {
    let connection = if has_wal {
        Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| isolate_failed())?
    } else {
        let mut uri = url::Url::from_file_path(path).map_err(|_| isolate_failed())?;
        uri.query_pairs_mut()
            .append_pair("mode", "ro")
            .append_pair("immutable", "1");
        Connection::open_with_flags(
            uri.as_str(),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| isolate_failed())?
    };
    connection
        .pragma_update(None, "query_only", 1i64)
        .map_err(|_| isolate_failed())?;
    Ok(connection)
}

/// Windows deny-write hold on an existing `-shm`. Dropping it releases the share mode.
/// Non-Windows builds do not take this lease. Missing `-shm` is not an error and is not created.
struct ShmDenyWriteLease {
    #[cfg(windows)]
    _file: Option<fs::File>,
}

impl ShmDenyWriteLease {
    fn none() -> Self {
        Self {
            #[cfg(windows)]
            _file: None,
        }
    }
}

/// When `-shm` already exists, hold it with `FILE_SHARE_READ` before any SQLite open.
/// A writer that already has the file open for write makes this fail closed.
/// The marker check happens first, so an unmarked database is not opened here.
fn acquire_product_shm_lease(database: &Path) -> Result<ShmDenyWriteLease, String> {
    #[cfg(windows)]
    {
        let shm = sidecar_name(database, "-shm");
        match fs::symlink_metadata(&shm) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ShmDenyWriteLease::none())
            }
            Ok(meta) if meta.file_type().is_file() && single_link(&meta) => {
                let file = super::source_markdown::open_checked_deny_write(&shm)
                    .map_err(|_| isolate_failed())?;
                Ok(ShmDenyWriteLease { _file: Some(file) })
            }
            _ => Err(isolate_failed()),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = database;
        Ok(ShmDenyWriteLease::none())
    }
}

/// Copy a marked synthetic database and open SQLite only on that copy.
/// `interfere` runs between the two source reads so a torn sidecar fails closed.
pub(super) fn open_isolated_readonly(
    path: &Path,
    timeout: Duration,
    interfere: impl FnOnce(),
) -> Result<Connection, String> {
    open_isolated_readonly_leased(path, timeout, interfere, acquire_product_shm_lease)
}

fn open_isolated_readonly_leased(
    path: &Path,
    timeout: Duration,
    interfere: impl FnOnce(),
    lease: impl FnOnce(&Path) -> Result<ShmDenyWriteLease, String>,
) -> Result<Connection, String> {
    if timeout.is_zero() {
        return Err(isolate_failed());
    }
    require_synthetic_marker(path)?;
    let held = lease(path)?;
    let started = Instant::now();
    let first = read_bundle(path, started, timeout)?;
    interfere();
    let second = read_bundle(path, started, timeout)?;
    if second.digest != first.digest {
        return Err(isolate_failed());
    }
    let temp = tempfile::tempdir().map_err(|_| isolate_failed())?;
    let copied = temp.path().join("snapshot.db");
    write_copy(&copied, &second.database, started, timeout)?;
    if let Some(bytes) = &second.wal {
        write_copy(&sidecar_name(&copied, "-wal"), bytes, started, timeout)?;
    }
    if let Some(bytes) = &second.shm {
        write_copy(&sidecar_name(&copied, "-shm"), bytes, started, timeout)?;
    }
    let local = open_copied(&copied, second.wal.is_some())?;
    let snapshot = snapshot_connection(
        &local,
        timeout
            .checked_sub(started.elapsed())
            .ok_or_else(isolate_failed)?,
        ISOLATED_LIMIT,
    )?;
    drop(local);
    drop(temp);
    let after = read_bundle(path, started, timeout)?;
    if after.digest != first.digest {
        return Err(isolate_failed());
    }
    drop(held);
    Ok(snapshot)
}

#[cfg(test)]
mod lease_gate_tests {
    use super::*;
    use std::cell::Cell;

    const FIXTURE: &[u8] = include_bytes!("fixtures/yingliu-migration-sample.sqlite");

    #[test]
    fn unmarked_database_is_refused_before_shm_lease_or_sqlite() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("vault.sqlite");
        fs::write(&path, FIXTURE).unwrap();
        let called = Cell::new(false);
        let error = open_isolated_readonly_leased(
            &path,
            Duration::from_secs(2),
            || {},
            |_| {
                called.set(true);
                Ok(ShmDenyWriteLease::none())
            },
        )
        .unwrap_err();
        assert!(!called.get(), "lease ran on an unmarked database");
        assert!(error.contains("尚未授权"), "{error}");
        assert!(!temp.path().join("vault.sqlite-wal").exists());
        assert!(!temp.path().join("vault.sqlite-shm").exists());
    }

    #[test]
    fn denied_shm_lease_stops_before_sqlite_opens_a_copy() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(SYNTHETIC_COURSE_MARKER),
            SYNTHETIC_COURSE_MARKER_BODY,
        )
        .unwrap();
        let path = temp.path().join("synthetic.sqlite");
        fs::write(&path, FIXTURE).unwrap();
        let error = open_isolated_readonly_leased(
            &path,
            Duration::from_secs(2),
            || {},
            |_| Err("拒绝写入租约失败".into()),
        )
        .unwrap_err();
        assert_eq!(error, "拒绝写入租约失败");
        assert_eq!(fs::read(&path).unwrap(), FIXTURE);
        assert!(!temp.path().join("synthetic.sqlite-wal").exists());
        assert!(!temp.path().join("synthetic.sqlite-shm").exists());
    }

    /// Not executed on Linux CI. Proves the share mode used by the product lease:
    /// a later write-sharing open fails, and a plain read still succeeds.
    #[cfg(windows)]
    #[test]
    fn windows_shm_deny_write_lease_blocks_share_write_open() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(SYNTHETIC_COURSE_MARKER),
            SYNTHETIC_COURSE_MARKER_BODY,
        )
        .unwrap();
        let path = temp.path().join("synthetic.sqlite");
        fs::write(&path, FIXTURE).unwrap();
        let shm = sidecar_name(&path, "-shm");
        fs::write(&shm, b"shm-bytes").unwrap();
        let lease = acquire_product_shm_lease(&path).unwrap();
        assert_eq!(fs::read(&shm).unwrap(), b"shm-bytes");
        let mut contested = fs::OpenOptions::new();
        contested
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        assert!(contested.open(&shm).is_err());
        drop(lease);
        assert!(contested.open(&shm).is_ok());
    }
}
