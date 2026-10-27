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
