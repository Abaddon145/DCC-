use super::import_snapshot::{snapshot_synthetic, snapshot_synthetic_connection};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

fn source(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.pragma_update(None, "page_size", 8192).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    let _: i64 = db
        .query_row("PRAGMA wal_autocheckpoint=0", [], |row| row.get(0))
        .unwrap();
    db.execute_batch(
        "CREATE TABLE schema_version(version INTEGER NOT NULL);
         INSERT INTO schema_version VALUES(8);
         PRAGMA user_version=8;
         CREATE TABLE video_notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL);",
    )
    .unwrap();
    let _: (i64, i64, i64) = db
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    db
}

#[test]
fn learning_import_snapshot_reads_wal_without_changing_source_database_or_wal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    writer.execute_batch("BEGIN; INSERT INTO video_notes VALUES(1,'a'); INSERT INTO video_notes VALUES(2,'b'); COMMIT;").unwrap();
    let wal = temp.path().join("synthetic.db-wal");
    let shm = temp.path().join("synthetic.db-shm");
    let db_before = fs::read(&path).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    assert!(!wal_before.is_empty());

    // The test-only copy proves that the main file by itself misses WAL commits.
    let main_only = temp.path().join("main-only.db");
    fs::write(&main_only, &db_before).unwrap();
    let incomplete =
        Connection::open_with_flags(&main_only, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        incomplete
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    drop(incomplete);

    // Concurrent live-writer visibility still works via an explicitly non-SHM-safe open.
    let live_reader = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    let live_snapshot =
        snapshot_synthetic_connection(&live_reader, Duration::from_secs(2)).unwrap();
    assert_eq!(
        live_snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        2
    );
    writer
        .execute(
            "INSERT INTO video_notes VALUES(3,'writer still active')",
            [],
        )
        .unwrap();
    assert_eq!(
        live_snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        2
    );
    drop(live_snapshot);
    drop(live_reader);

    #[cfg(windows)]
    {
        // Task12c13: SHM-safe helper must fail closed while the writer holds SHM writable.
        assert!(shm.is_file());
        let shm_before = fs::read(&shm).unwrap();
        assert!(snapshot_synthetic(&path, Duration::from_secs(2)).is_err());
        assert_eq!(fs::read(&shm).unwrap(), shm_before);
    }

    drop(writer);
    let db_after_writer = fs::read(&path).unwrap();
    let wal_after_writer = fs::read(&wal).unwrap();
    #[cfg(windows)]
    let shm_after_writer = fs::read(&shm).unwrap();
    let snapshot = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("PRAGMA page_size", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        8192
    );
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        3
    );
    assert_eq!(fs::read(&path).unwrap(), db_after_writer);
    assert_eq!(fs::read(&wal).unwrap(), wal_after_writer);
    #[cfg(windows)]
    assert_eq!(fs::read(&shm).unwrap(), shm_after_writer);
}

#[test]
fn learning_import_snapshot_last_reader_does_not_checkpoint_source_wal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    writer
        .execute("INSERT INTO video_notes VALUES(1,'only in wal')", [])
        .unwrap();
    drop(writer);
    let wal = temp.path().join("synthetic.db-wal");
    let db_before = fs::read(&path).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    assert!(!wal_before.is_empty());
    let snapshot = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
}

#[test]
fn learning_import_snapshot_rejects_wrong_schema_corruption_and_invalid_foreign_keys() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer.execute_batch("PRAGMA user_version=7;").unwrap();
    assert!(snapshot_synthetic(&path, Duration::from_secs(1)).is_err());
    writer
        .execute_batch("PRAGMA user_version=8; UPDATE schema_version SET version=7;")
        .unwrap();
    assert!(snapshot_synthetic(&path, Duration::from_secs(1)).is_err());
    writer
        .execute_batch("UPDATE schema_version SET version=8; INSERT INTO schema_version VALUES(8);")
        .unwrap();
    assert!(snapshot_synthetic(&path, Duration::from_secs(1)).is_err());
    writer.execute_batch("DELETE FROM schema_version WHERE rowid=(SELECT MAX(rowid) FROM schema_version); PRAGMA foreign_keys=OFF; CREATE TABLE parents(id INTEGER PRIMARY KEY); CREATE TABLE children(parent INTEGER REFERENCES parents(id)); INSERT INTO children VALUES(99);").unwrap();
    assert!(snapshot_synthetic(&path, Duration::from_secs(1)).is_err());
    let corrupt = temp.path().join("corrupt.db");
    fs::write(&corrupt, b"not a sqlite database").unwrap();
    assert!(snapshot_synthetic(&corrupt, Duration::from_secs(1)).is_err());
    assert!(snapshot_synthetic(&temp.path().join("missing.db"), Duration::from_secs(1)).is_err());
    assert!(!temp.path().join("missing.db").exists());
}

#[test]
fn learning_import_snapshot_busy_source_fails_in_bounded_time() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("locked.db");
    let writer = Connection::open(&path).unwrap();
    writer.execute_batch("CREATE TABLE schema_version(version INTEGER); INSERT INTO schema_version VALUES(8); PRAGMA user_version=8; BEGIN EXCLUSIVE;").unwrap();
    let started = Instant::now();
    assert!(snapshot_synthetic(&path, Duration::from_millis(100)).is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
    writer.execute_batch("ROLLBACK;").unwrap();
}

#[cfg(windows)]
fn synthetic_windows_lease(path: &Path, directory: bool) -> std::fs::File {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    options.custom_flags(
        FILE_FLAG_OPEN_REPARSE_POINT
            | if directory {
                FILE_FLAG_BACKUP_SEMANTICS
            } else {
                0
            },
    );
    let file = options.open(path).unwrap();
    let metadata = file.metadata().unwrap();
    assert_eq!(metadata.file_attributes() & 0x400, 0);
    assert_eq!(metadata.is_dir(), directory);
    file
}

#[cfg(windows)]
fn snapshot_complete_sidecars(
    path: &Path,
    after_leases: impl FnOnce(),
) -> Result<Connection, String> {
    use std::{
        ffi::c_void,
        fs::OpenOptions,
        os::windows::{
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        },
        ptr,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    };
    let error = || "合成 sidecar 不完整或身份不匹配".to_owned();
    let root = path.parent().ok_or_else(error)?;
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_os_string();
    shm.push("-shm");
    let lease = |file: &Path, directory: bool| -> Result<std::fs::File, String> {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        if directory {
            options.access_mode(FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY);
        }
        options.custom_flags(
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
        );
        let lease = options.open(file).map_err(|_| error())?;
        let metadata = lease.metadata().map_err(|_| error())?;
        if metadata.file_attributes() & 0x400 != 0 || metadata.is_dir() != directory {
            return Err(error());
        }
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(lease.as_raw_handle(), &mut info) } == 0
            || (!directory && info.nNumberOfLinks != 1)
        {
            return Err(error());
        }
        Ok(lease)
    };
    let _root_lease = lease(root, true)?;
    let db_lease = lease(path, false)?;
    let wal_lease = lease(Path::new(&wal), false)?;
    let _shm_lease = lease(Path::new(&shm), false)?;
    after_leases();
    let identity = |handle: *mut c_void| -> Result<(u32, u32, u32), String> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(handle, &mut info) } == 0 {
            return Err(error());
        }
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    };
    let reader = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| error())?;
    reader
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .map_err(|_| error())?;
    reader.busy_timeout(Duration::ZERO).map_err(|_| error())?;
    reader.execute_batch("BEGIN").map_err(|_| error())?;
    reader
        .query_row("SELECT count(*) FROM schema_version", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|_| error())?;
    let mut main_handle: *mut c_void = ptr::null_mut();
    let rc = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            reader.handle(),
            b"main\0".as_ptr().cast(),
            rusqlite::ffi::SQLITE_FCNTL_WIN32_GET_HANDLE,
            (&mut main_handle as *mut *mut c_void).cast(),
        )
    };
    if rc != rusqlite::ffi::SQLITE_OK
        || main_handle.is_null()
        || identity(main_handle)? != identity(db_lease.as_raw_handle())?
    {
        return Err(error());
    }
    let mut journal: *mut rusqlite::ffi::sqlite3_file = ptr::null_mut();
    let rc = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            reader.handle(),
            b"main\0".as_ptr().cast(),
            rusqlite::ffi::SQLITE_FCNTL_JOURNAL_POINTER,
            (&mut journal as *mut *mut rusqlite::ffi::sqlite3_file).cast(),
        )
    };
    if rc != rusqlite::ffi::SQLITE_OK || journal.is_null() {
        return Err(error());
    }
    let methods = unsafe { (*journal).pMethods };
    if methods.is_null() {
        return Err(error());
    }
    let control = unsafe { (*methods).xFileControl }.ok_or_else(error)?;
    let mut wal_handle: *mut c_void = ptr::null_mut();
    let rc = unsafe {
        control(
            journal,
            rusqlite::ffi::SQLITE_FCNTL_WIN32_GET_HANDLE,
            (&mut wal_handle as *mut *mut c_void).cast(),
        )
    };
    if rc != rusqlite::ffi::SQLITE_OK
        || wal_handle.is_null()
        || identity(wal_handle)? != identity(wal_lease.as_raw_handle())?
    {
        return Err(error());
    }
    let snapshot =
        super::import_snapshot::snapshot_synthetic_connection(&reader, Duration::from_secs(2))?;
    reader.execute_batch("ROLLBACK").map_err(|_| error())?;
    Ok(snapshot)
}

#[cfg(windows)]
#[test]
fn learning_import_windows_complete_sidecars_are_required_before_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    let missing = root.join("missing.db");
    fs::write(&missing, b"untouched").unwrap();
    assert!(snapshot_complete_sidecars(&missing, || panic!("missing sidecar accepted")).is_err());
    assert_eq!(fs::read(&missing).unwrap(), b"untouched");
    assert!(!root.join("missing.db-wal").exists());
    assert!(!root.join("missing.db-shm").exists());
    fs::write(root.join("missing.db-wal"), b"fixture").unwrap();
    assert!(snapshot_complete_sidecars(&missing, || panic!("missing SHM accepted")).is_err());
    assert!(!root.join("missing.db-shm").exists());
    fs::remove_file(root.join("missing.db-wal")).unwrap();
    fs::write(root.join("missing.db-shm"), b"fixture").unwrap();
    assert!(snapshot_complete_sidecars(&missing, || panic!("missing WAL accepted")).is_err());
    assert!(!root.join("missing.db-wal").exists());
    let path = root.join("active.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'wal-only')", [])
        .unwrap();
    let snapshot = snapshot_complete_sidecars(&path, || {
        for name in ["active.db", "active.db-wal", "active.db-shm"] {
            assert!(fs::rename(root.join(name), root.join(format!("{name}.moved"))).is_err());
        }
    })
    .unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    writer
        .execute("INSERT INTO video_notes VALUES(2,'still active')", [])
        .unwrap();
    assert_eq!(
        snapshot_complete_sidecars(&path, || {})
            .unwrap()
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[cfg(windows)]
#[test]
fn learning_import_windows_sqlite_journal_pointer_does_not_guarantee_open_wal() {
    use std::ptr;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("delete-mode.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE test(id INTEGER);").unwrap();
    let mut journal: *mut rusqlite::ffi::sqlite3_file = ptr::null_mut();
    let rc = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            db.handle(),
            b"main\0".as_ptr().cast(),
            rusqlite::ffi::SQLITE_FCNTL_JOURNAL_POINTER,
            (&mut journal as *mut *mut rusqlite::ffi::sqlite3_file).cast(),
        )
    };
    assert_eq!(rc, rusqlite::ffi::SQLITE_OK);
    assert!(!journal.is_null());
    assert!(unsafe { (*journal).pMethods }.is_null());
}

#[cfg(windows)]
#[test]
fn learning_import_windows_sqlite_handles_match_leased_database_and_wal() {
    use std::{ffi::c_void, os::windows::io::AsRawHandle, ptr};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    fn file_identity(handle: *mut c_void) -> (u32, u32, u32) {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        assert_ne!(unsafe { GetFileInformationByHandle(handle, &mut info) }, 0);
        (
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        )
    }

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    let path = root.join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'wal-only')", [])
        .unwrap();
    let wal = root.join("synthetic.db-wal");
    let unrelated = root.join("other.db");
    fs::write(&unrelated, b"other").unwrap();
    let root_lease = synthetic_windows_lease(&root, true);
    let db_lease = synthetic_windows_lease(&path, false);
    let wal_lease = synthetic_windows_lease(&wal, false);
    let other_lease = synthetic_windows_lease(&unrelated, false);
    let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        reader
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let mut db_handle: *mut c_void = ptr::null_mut();
    let rc = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            reader.handle(),
            b"main\0".as_ptr().cast(),
            rusqlite::ffi::SQLITE_FCNTL_WIN32_GET_HANDLE,
            (&mut db_handle as *mut *mut c_void).cast(),
        )
    };
    assert_eq!(rc, rusqlite::ffi::SQLITE_OK);
    assert!(!db_handle.is_null());
    assert_eq!(
        file_identity(db_handle),
        file_identity(db_lease.as_raw_handle())
    );
    assert_ne!(
        file_identity(db_handle),
        file_identity(other_lease.as_raw_handle())
    );
    let mut journal: *mut rusqlite::ffi::sqlite3_file = ptr::null_mut();
    let rc = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            reader.handle(),
            b"main\0".as_ptr().cast(),
            rusqlite::ffi::SQLITE_FCNTL_JOURNAL_POINTER,
            (&mut journal as *mut *mut rusqlite::ffi::sqlite3_file).cast(),
        )
    };
    assert_eq!(rc, rusqlite::ffi::SQLITE_OK);
    assert!(!journal.is_null());
    let mut wal_handle: *mut c_void = ptr::null_mut();
    let methods = unsafe { (*journal).pMethods };
    assert!(!methods.is_null());
    let control = unsafe { (*methods).xFileControl }.expect("journal supports file control");
    let rc = unsafe {
        control(
            journal,
            rusqlite::ffi::SQLITE_FCNTL_WIN32_GET_HANDLE,
            (&mut wal_handle as *mut *mut c_void).cast(),
        )
    };
    assert_eq!(rc, rusqlite::ffi::SQLITE_OK);
    assert!(!wal_handle.is_null());
    assert_eq!(
        file_identity(wal_handle),
        file_identity(wal_lease.as_raw_handle())
    );
    assert_ne!(
        file_identity(wal_handle),
        file_identity(db_lease.as_raw_handle())
    );
    writer
        .execute(
            "INSERT INTO video_notes VALUES(2,'writer still active')",
            [],
        )
        .unwrap();
    // Live writer holds SHM writable; SHM-safe snapshot_synthetic must fail closed.
    assert!(snapshot_synthetic(&path, Duration::from_secs(2)).is_err());
    assert_eq!(
        snapshot_synthetic_connection(&reader, Duration::from_secs(2))
            .unwrap()
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(reader);
    drop((db_lease, wal_lease, other_lease, root_lease));
    drop(writer);
}

#[cfg(windows)]
#[test]
fn learning_import_windows_lease_protects_existing_files_while_sqlite_writes() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    let root = parent.join("source");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'wal')", [])
        .unwrap();
    let wal = root.join("synthetic.db-wal");
    let shm = root.join("synthetic.db-shm");
    assert!(wal.is_file() && shm.is_file());
    let parent_lease = synthetic_windows_lease(&parent, true);
    let root_lease = synthetic_windows_lease(&root, true);
    let db_lease = synthetic_windows_lease(&path, false);
    let wal_lease = synthetic_windows_lease(&wal, false);
    let shm_lease = synthetic_windows_lease(&shm, false);
    assert!(fs::rename(&parent, temp.path().join("replaced-parent")).is_err());
    assert!(fs::rename(&path, root.join("replaced.db")).is_err());
    assert!(fs::rename(&wal, root.join("replaced-wal")).is_err());
    assert!(fs::rename(&shm, root.join("replaced-shm")).is_err());
    writer
        .execute("INSERT INTO video_notes VALUES(2,'still writable')", [])
        .unwrap();
    assert!(snapshot_synthetic(&path, Duration::from_secs(2)).is_err());
    let reader = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    let snapshot = snapshot_synthetic_connection(&reader, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(snapshot);
    drop(reader);
    drop((db_lease, wal_lease, shm_lease, root_lease, parent_lease));
    drop(writer);
}

#[cfg(windows)]
#[test]
fn learning_import_windows_lease_does_not_protect_absent_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    let db = root.join("synthetic.db");
    let wal = root.join("synthetic.db-wal");
    fs::write(&db, b"fixture").unwrap();
    let root_lease = synthetic_windows_lease(&root, true);
    let db_lease = synthetic_windows_lease(&db, false);
    assert!(!wal.exists());
    fs::write(&wal, b"new sidecar").unwrap();
    assert_eq!(fs::read(&wal).unwrap(), b"new sidecar");
    drop((db_lease, root_lease));
}

#[cfg(windows)]
#[test]
fn learning_import_windows_cross_process_child() {
    if !std::env::args().any(|arg| arg == "--exact") {
        return;
    }
    let Some(root) = std::env::var_os("STACKDCC_LEASE_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root).canonicalize().unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(root.starts_with(&temp));
    assert_eq!(root.file_name().unwrap(), "source");
    assert_eq!(root.parent().unwrap().file_name().unwrap(), "parent");
    let token = std::env::var("STACKDCC_LEASE_TEST_TOKEN").unwrap();
    assert!(uuid::Uuid::parse_str(&token).is_ok());
    assert_eq!(
        fs::read_to_string(root.join(".lease-fixture")).unwrap(),
        token
    );
    let parent = root.parent().unwrap();
    assert!(fs::rename(parent, parent.with_file_name("replaced-parent")).is_err());
    let db = root.join("synthetic.db");
    assert!(fs::rename(&db, root.join("replaced.db")).is_err());
    assert!(fs::rename(root.join("synthetic.db-wal"), root.join("replaced-wal")).is_err());
    assert!(fs::rename(root.join("synthetic.db-shm"), root.join("replaced-shm")).is_err());
    let writer = Connection::open(db).unwrap();
    writer
        .execute("INSERT INTO video_notes VALUES(2,'separate process')", [])
        .unwrap();
}

#[cfg(windows)]
#[test]
fn learning_import_windows_cross_process_lease_allows_sqlite_writer() {
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    let root = parent.join("source");
    fs::create_dir_all(&root).unwrap();
    let token = uuid::Uuid::new_v4().to_string();
    fs::write(root.join(".lease-fixture"), &token).unwrap();
    let path = root.join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'first process')", [])
        .unwrap();
    let parent_lease = synthetic_windows_lease(&parent, true);
    let root_lease = synthetic_windows_lease(&root, true);
    let db_lease = synthetic_windows_lease(&path, false);
    let wal_lease = synthetic_windows_lease(&root.join("synthetic.db-wal"), false);
    let shm_lease = synthetic_windows_lease(&root.join("synthetic.db-shm"), false);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "learning::import_snapshot_tests::learning_import_windows_cross_process_child",
            "--nocapture",
        ])
        .env("STACKDCC_LEASE_TEST_ROOT", &root)
        .env("STACKDCC_LEASE_TEST_TOKEN", &token)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("synthetic child exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "synthetic child failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(snapshot_synthetic(&path, Duration::from_secs(2)).is_err());
    let reader = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    let snapshot = snapshot_synthetic_connection(&reader, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(snapshot);
    drop(reader);
    drop((db_lease, wal_lease, shm_lease, root_lease, parent_lease));
    drop(writer);
}
#[cfg(windows)]
fn sidecar_paths(path: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_os_string();
    shm.push("-shm");
    (std::path::PathBuf::from(wal), std::path::PathBuf::from(shm))
}

/// Documents the 12c11 residual hazard: a plain READ_ONLY open after the writer exits
/// still mutates the existing -shm bytes on Windows bundled SQLite.
#[cfg(windows)]
#[test]
fn learning_import_windows_default_readonly_open_mutates_shm_after_writer_exits() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("hazard.db");
    {
        let writer = source(&path);
        writer
            .set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )
            .unwrap();
        writer
            .execute("INSERT INTO video_notes VALUES(1,'wal-only')", [])
            .unwrap();
    }
    let (wal, shm) = sidecar_paths(&path);
    assert!(wal.exists());
    assert!(shm.exists());
    let shm_before = fs::read(&shm).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    let db_before = fs::read(&path).unwrap();
    {
        let reader = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        reader
            .set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )
            .unwrap();
        assert_eq!(
            reader
                .query_row("SELECT count(*) FROM video_notes", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
    assert_ne!(
        fs::read(&shm).unwrap(),
        shm_before,
        "plain READ_ONLY open must still be treated as SHM-mutating"
    );
}

/// Safe synthetic path: deny-write SHM lease + plain READ_ONLY preserves closed-writer
/// SHM bytes and still completes an online Backup of WAL-committed rows.
#[cfg(windows)]
#[test]
fn learning_import_windows_shm_deny_write_lease_preserves_shm_and_snapshots_wal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("safe.db");
    {
        let writer = source(&path);
        writer
            .set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )
            .unwrap();
        writer
            .execute("INSERT INTO video_notes VALUES(1,'wal-only')", [])
            .unwrap();
    }
    let (wal, shm) = sidecar_paths(&path);
    let shm_before = fs::read(&shm).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    let db_before = fs::read(&path).unwrap();
    let snapshot = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
    assert_eq!(
        fs::read(&shm).unwrap(),
        shm_before,
        "deny-write SHM lease must not allow reader write/truncate of closed-writer SHM"
    );
}

/// Holding the SHM handle with FILE_SHARE_READ (deny write to others) also stops a
/// plain READ_ONLY open from mutating SHM after the writer has exited.
#[cfg(windows)]
#[test]
fn learning_import_windows_shm_share_read_lease_blocks_reader_shm_writes() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease.db");
    {
        let writer = source(&path);
        writer
            .set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )
            .unwrap();
        writer
            .execute("INSERT INTO video_notes VALUES(1,'wal-only')", [])
            .unwrap();
    }
    let (_, shm) = sidecar_paths(&path);
    let shm_before = fs::read(&shm).unwrap();
    let mut options = fs::OpenOptions::new();
    options.read(true).share_mode(FILE_SHARE_READ);
    let _shm_lease = options.open(&shm).unwrap();
    {
        let reader = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        assert_eq!(
            reader
                .query_row("SELECT count(*) FROM video_notes", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    assert_eq!(fs::read(&shm).unwrap(), shm_before);
}
/// Task12c13: while an active writer holds SHM writable, the SHM-safe helper refuses to
/// open (fail closed) and therefore does not mutate SHM. Plain READ_ONLY is left for
/// explicit concurrent experiments only.
#[cfg(windows)]
#[test]
fn learning_import_windows_active_writer_fails_closed_without_mutating_shm() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("live.db");
    let writer = source(&path);
    writer
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    writer
        .execute("INSERT INTO video_notes VALUES(1,'live')", [])
        .unwrap();
    let (wal, shm) = sidecar_paths(&path);
    assert!(wal.exists() && shm.exists());
    let shm_before = fs::read(&shm).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    let db_before = fs::read(&path).unwrap();
    assert!(snapshot_synthetic(&path, Duration::from_secs(2)).is_err());
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
    assert_eq!(
        fs::read(&shm).unwrap(),
        shm_before,
        "fail-closed must not open SQLite and must not mutate SHM"
    );
    drop(writer);
    let snapshot = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

/// Documents that the unsafe plain READ_ONLY path can still see live WAL commits while a
/// writer is active; that path is not SHM-safe and must not be used by snapshot_synthetic.
#[cfg(windows)]
#[test]
fn learning_import_windows_active_writer_plain_readonly_still_reads_wal_unsafely() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("unsafe.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'a')", [])
        .unwrap();
    assert!(snapshot_synthetic(&path, Duration::from_secs(2)).is_err());
    let reader = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    let snapshot = snapshot_synthetic_connection(&reader, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(snapshot);
    drop(reader);
    drop(writer);
}
