use super::import_snapshot::snapshot_synthetic;
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
    writer.execute_batch("BEGIN; INSERT INTO video_notes VALUES(1,'a'); INSERT INTO video_notes VALUES(2,'b'); COMMIT;").unwrap();
    let wal = temp.path().join("synthetic.db-wal");
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
        2
    );
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
    writer
        .execute(
            "INSERT INTO video_notes VALUES(3,'writer still active')",
            [],
        )
        .unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        2
    );
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
    let snapshot = snapshot_synthetic(&path, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(snapshot);
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
