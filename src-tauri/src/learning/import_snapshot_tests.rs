#[cfg(windows)]
use super::import_snapshot::{
    copy_complete_sidecars_for_snapshot_with_interference, snapshot_synthetic_via_sidecar_copy,
};
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

#[cfg(windows)]
#[test]
fn learning_import_readonly_shm_child_rejects_missing_sidecars_without_creation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'safe')", [])
        .unwrap();
    drop(writer);
    let wal = temp.path().join("synthetic.db-wal");
    let shm = temp.path().join("synthetic.db-shm");
    assert!(!wal.exists() && !shm.exists());
    let before = fs::read(&path).unwrap();
    assert!(super::import_snapshot::open_synthetic_readonly_shm_child(&path).is_err());
    assert!(!wal.exists() && !shm.exists());
    fs::write(&wal, b"synthetic-wal").unwrap();
    assert!(super::import_snapshot::open_synthetic_readonly_shm_child(&path).is_err());
    assert!(!shm.exists());
    assert_eq!(fs::read(&wal).unwrap(), b"synthetic-wal");
    fs::remove_file(&wal).unwrap();
    fs::write(&shm, b"synthetic-shm").unwrap();
    assert!(super::import_snapshot::open_synthetic_readonly_shm_child(&path).is_err());
    assert!(!wal.exists());
    assert_eq!(fs::read(&shm).unwrap(), b"synthetic-shm");
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[cfg(windows)]
#[test]
fn learning_import_readonly_shm_child_rejects_existing_rollback_journal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'wal-only')", [])
        .unwrap();
    let files = [
        path.clone(),
        temp.path().join("synthetic.db-wal"),
        temp.path().join("synthetic.db-shm"),
    ];
    let journal = temp.path().join("synthetic.db-journal");
    fs::write(&journal, b"synthetic-journal").unwrap();
    let before = files
        .iter()
        .map(fs::read)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(super::import_snapshot::open_synthetic_readonly_shm_child(&path).is_err());
    assert_eq!(
        before,
        files
            .iter()
            .map(fs::read)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    );
    assert_eq!(fs::read(&journal).unwrap(), b"synthetic-journal");
    drop(writer);
}

#[cfg(windows)]
#[test]
fn learning_import_readonly_shm_child_rejects_corrupt_wal_before_opening_sqlite() {
    use std::io::{Seek, SeekFrom, Write};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'wal-only')", [])
        .unwrap();
    writer
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    drop(writer);
    let wal = temp.path().join("synthetic.db-wal");
    let shm = temp.path().join("synthetic.db-shm");
    let mut corrupt = fs::OpenOptions::new().write(true).open(&wal).unwrap();
    corrupt.seek(SeekFrom::Start(8)).unwrap();
    corrupt.write_all(&0u32.to_be_bytes()).unwrap();
    drop(corrupt);
    let files = [path.clone(), wal, shm];
    let before = files
        .iter()
        .map(fs::read)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        super::import_snapshot::open_synthetic_readonly_shm_child(&path)
            .err()
            .unwrap(),
        "来源数据库或 WAL 文件头无效"
    );
    assert_eq!(
        before,
        files
            .iter()
            .map(fs::read)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    );
}

#[cfg(windows)]
#[test]
fn learning_import_readonly_shm_process_child() {
    if !std::env::args().any(|arg| arg == "--exact") {
        return;
    }
    let Some(root) = std::env::var_os("STACKDCC_READONLY_WAL_ROOT") else {
        return;
    };
    let raw = std::path::PathBuf::from(root);
    let root = raw.canonicalize().unwrap();
    assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert_eq!(root.file_name().unwrap(), "source");
    let token = std::env::var("STACKDCC_READONLY_WAL_TOKEN").unwrap();
    assert!(uuid::Uuid::parse_str(&token).is_ok());
    assert_eq!(fs::read_to_string(root.join(".fixture")).unwrap(), token);
    let lease =
        super::import_snapshot::open_synthetic_readonly_shm_child(&raw.join("synthetic.db"))
            .unwrap();
    let reader = lease.connection();
    if std::env::var_os("STACKDCC_READONLY_WAL_CONCURRENT").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        use rusqlite::backup::{Backup, StepResult};
        reader
            .set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )
            .unwrap();
        reader.busy_timeout(Duration::ZERO).unwrap();
        let mut snapshot = Connection::open_in_memory().unwrap();
        {
            let backup = Backup::new(reader, &mut snapshot).unwrap();
            assert!(matches!(backup.step(1).unwrap(), StepResult::More));
            fs::write(root.join(".ready"), &token).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while fs::read_to_string(root.join(".resume")).ok().as_deref() != Some(token.as_str()) {
                assert!(
                    Instant::now() < deadline,
                    "synthetic writer did not resume backup"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            loop {
                assert!(
                    Instant::now() < deadline,
                    "synthetic concurrent backup timed out"
                );
                match backup.step(64).unwrap() {
                    StepResult::Done => break,
                    StepResult::More => continue,
                    _ => panic!("synthetic concurrent backup blocked"),
                }
            }
        }
        let values: (i64, i64) = snapshot
            .query_row("SELECT a,b FROM proof", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(values, (10, 10));
        return;
    }
    let snapshot = snapshot_synthetic_connection(reader, Duration::from_secs(3)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT body FROM video_notes WHERE id=1", [], |row| row
                .get::<_, String>(
                0
            ))
            .unwrap(),
        "wal-only"
    );
}

#[cfg(windows)]
#[test]
fn learning_import_readonly_shm_separate_process_preserves_source_files() {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("中文 # %").join("source");
    fs::create_dir_all(&root).unwrap();
    let token = uuid::Uuid::new_v4().to_string();
    fs::write(root.join(".fixture"), &token).unwrap();
    let path = root.join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'wal-only')", [])
        .unwrap();
    assert!(!root.join("synthetic.db-journal").exists());
    let (locked_root, _directories) = super::source_markdown::lock_vault_root(&root).unwrap();
    let leased_paths = [
        locked_root.join("synthetic.db"),
        locked_root.join("synthetic.db-wal"),
        locked_root.join("synthetic.db-shm"),
    ];
    let _leases = leased_paths
        .iter()
        .map(|path| super::source_markdown::open_checked_shared(path))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (index, source) in leased_paths.iter().enumerate() {
        assert!(fs::rename(source, locked_root.join(format!("replacement-{index}"))).is_err());
    }
    let files = [
        path.clone(),
        root.join("synthetic.db-wal"),
        root.join("synthetic.db-shm"),
    ];
    let before = files
        .iter()
        .map(fs::read)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "learning::import_snapshot_tests::learning_import_readonly_shm_process_child",
            "--nocapture",
        ])
        .env("STACKDCC_READONLY_WAL_ROOT", &root)
        .env("STACKDCC_READONLY_WAL_TOKEN", &token)
        .env("STACKDCC_READONLY_WAL_CONCURRENT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("synthetic readonly child exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "synthetic readonly child failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let after = files
        .iter()
        .map(fs::read)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(before, after);
    assert!(!root.join("synthetic.db-journal").exists());
    drop(writer);
}

#[cfg(windows)]
#[test]
fn learning_import_readonly_shm_process_backup_survives_concurrent_commits() {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    let token = uuid::Uuid::new_v4().to_string();
    fs::write(root.join(".fixture"), &token).unwrap();
    let writer = source(&root.join("synthetic.db"));
    writer
        .execute_batch("CREATE TABLE proof(a INTEGER, b INTEGER); INSERT INTO proof VALUES(0,0);")
        .unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "learning::import_snapshot_tests::learning_import_readonly_shm_process_child",
            "--nocapture",
        ])
        .env("STACKDCC_READONLY_WAL_ROOT", &root)
        .env("STACKDCC_READONLY_WAL_TOKEN", &token)
        .env("STACKDCC_READONLY_WAL_CONCURRENT", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut committed = false;
    while child.try_wait().unwrap().is_none() {
        if !committed
            && fs::read_to_string(root.join(".ready")).ok().as_deref() == Some(token.as_str())
        {
            assert!(!root.join("synthetic.db-journal").exists());
            assert!(fs::rename(root.join("synthetic.db"), root.join("moved.db")).is_err());
            assert!(fs::rename(&root, temp.path().join("moved-source")).is_err());
            for _ in 0..10 {
                writer.execute("UPDATE proof SET a=a+1,b=b+1", []).unwrap();
            }
            fs::write(root.join(".resume"), &token).unwrap();
            committed = true;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("synthetic concurrent readonly child exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "synthetic concurrent child failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        committed,
        "synthetic writer did not run between backup steps"
    );
}

#[cfg(windows)]
#[test]
fn learning_import_readonly_shm_selection_discards_stale_child_result() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_profiles, source_sqlite};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    for change in [
        "unchanged",
        "revoke",
        "epoch",
        "scope",
        "hardlink",
        "wal_hardlink",
        "shm_hardlink",
        "journal",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let state = crate::state::test_state(&temp.path().join("library"));
        let library = state.active_library_id().unwrap();
        let scope = source_profiles::scope(&state, &library).unwrap();
        let root = temp.path().join("source");
        fs::create_dir(&root).unwrap();
        let marker = uuid::Uuid::new_v4().to_string();
        fs::write(root.join(".fixture"), &marker).unwrap();
        let path = root.join("synthetic.db");
        let writer = source(&path);
        writer
            .execute_batch(
                "CREATE TABLE proof(a INTEGER, b INTEGER); INSERT INTO proof VALUES(0,0);",
            )
            .unwrap();
        let bound = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
        let choices = SyntheticSqliteSelections::default();
        let mut invoked = false;
        assert!(choices
            .inspect_isolated(&state, "invalid", |_| {
                invoked = true;
                Ok(())
            })
            .is_err());
        assert!(!invoked);
        let ticket = choices
            .select(&state, &library, bound.profile_id(), bound.epoch(), || {
                Some(path.clone())
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            choices
                .inspect_isolated(&state, &ticket, |selected| {
                    assert_eq!(
                        selected.canonicalize().unwrap(),
                        path.canonicalize().unwrap()
                    );
                    Ok("current selection")
                })
                .unwrap(),
            "current selection"
        );
        if change == "journal" {
            let journal = root.join("synthetic.db-journal");
            fs::write(&journal, b"synthetic").unwrap();
            let mut called = false;
            assert!(choices
                .inspect_isolated(&state, &ticket, |_| {
                    called = true;
                    Ok(())
                })
                .is_err());
            assert!(!called);
            fs::remove_file(journal).unwrap();
        }
        let child_completed = std::cell::Cell::new(false);
        let result = choices.inspect_isolated(&state, &ticket, |selected| {
            assert_eq!(
                selected.canonicalize().unwrap(),
                path.canonicalize().unwrap()
            );
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "learning::import_snapshot_tests::learning_import_readonly_shm_process_child",
                    "--nocapture",
                ])
                .env("STACKDCC_READONLY_WAL_ROOT", selected.parent().unwrap())
                .env("STACKDCC_READONLY_WAL_TOKEN", &marker)
                .env("STACKDCC_READONLY_WAL_CONCURRENT", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|error| error.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut changed = false;
            while child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                if !changed
                    && fs::read_to_string(root.join(".ready")).ok().as_deref()
                        == Some(marker.as_str())
                {
                    match change {
                        "unchanged" => (),
                        "revoke" => choices.revoke()?,
                        "epoch" => {
                            state.with_library(|db, _| source_profiles::invalidate_epochs(db))?
                        }
                        "scope" => *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4(),
                        "hardlink" => fs::hard_link(&path, root.join("alias.db"))
                            .map_err(|error| error.to_string())?,
                        "wal_hardlink" => {
                            fs::hard_link(root.join("synthetic.db-wal"), root.join("alias.wal"))
                                .map_err(|error| error.to_string())?
                        }
                        "shm_hardlink" => {
                            fs::hard_link(root.join("synthetic.db-shm"), root.join("alias.shm"))
                                .map_err(|error| error.to_string())?
                        }
                        "journal" => fs::write(root.join("synthetic.db-journal"), b"synthetic")
                            .map_err(|error| error.to_string())?,
                        _ => unreachable!(),
                    }
                    for _ in 0..10 {
                        writer
                            .execute("UPDATE proof SET a=a+1,b=b+1", [])
                            .map_err(|error| error.to_string())?;
                    }
                    fs::write(root.join(".resume"), &marker).map_err(|error| error.to_string())?;
                    changed = true;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("synthetic child timeout".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let output = child
                .wait_with_output()
                .map_err(|error| error.to_string())?;
            assert!(changed, "did not change {change} during backup");
            assert!(
                output.status.success(),
                "synthetic child failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            child_completed.set(true);
            Ok("snapshot ready")
        });
        assert!(
            child_completed.get(),
            "synthetic child did not finish after {change}"
        );
        if change == "unchanged" {
            assert_eq!(result.unwrap(), "snapshot ready");
            continue;
        }
        let error = result.expect_err("stale child result must not be returned");
        let expected = match change {
            "revoke" => "来源选择已变化",
            "epoch" => "来源状态已变化，请重新选择",
            "scope" => "素材库会话已变化，请重新检查来源",
            "hardlink" => "无法确认来源数据库身份",
            "wal_hardlink" | "shm_hardlink" => "来源文件身份不可验证",
            "journal" => "合成来源快照不可用",
            _ => unreachable!(),
        };
        assert_eq!(error, expected);
    }
}

#[test]
fn learning_import_snapshot_deadline_includes_validation_queries() {
    let source = Connection::open_in_memory().unwrap();
    source
        .execute_batch(
            "PRAGMA user_version=8;
        CREATE VIEW schema_version AS WITH RECURSIVE values_to_check(n) AS
        (VALUES(1) UNION ALL SELECT n+1 FROM values_to_check WHERE n<10000000)
        SELECT 8 AS version FROM (SELECT sum(n) FROM values_to_check);",
        )
        .unwrap();
    assert!(snapshot_synthetic_connection(&source, Duration::from_millis(100)).is_err());
    source.execute_batch("DROP VIEW schema_version; CREATE TABLE schema_version(version INTEGER); INSERT INTO schema_version VALUES(8);").unwrap();
    let snapshot = snapshot_synthetic_connection(&source, Duration::from_millis(100)).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let sum: i64 = snapshot.query_row("WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<10000) SELECT sum(n) FROM numbers", [], |row| row.get(0)).unwrap();
    assert_eq!(sum, 50005000);
}

#[test]
fn learning_import_snapshot_rejects_oversized_database_before_returning_it() {
    let source = Connection::open_in_memory().unwrap();
    source
        .execute_batch(
            "CREATE TABLE schema_version(version INTEGER NOT NULL);
         INSERT INTO schema_version VALUES(8);
         PRAGMA user_version=8;
         CREATE TABLE content(value BLOB);
         INSERT INTO content VALUES(zeroblob(262144));",
        )
        .unwrap();
    let pages: u64 = source
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    let page_size: u64 = source
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap();
    let exact_limit = pages * page_size;
    let snapshot = super::import_snapshot::snapshot_synthetic_connection_with_limit(
        &source,
        Duration::from_secs(2),
        exact_limit,
    )
    .unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT length(value) FROM content", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        262144
    );
    assert_eq!(
        super::import_snapshot::snapshot_synthetic_connection_with_limit(
            &source,
            Duration::from_secs(2),
            exact_limit - 1,
        )
        .err()
        .unwrap(),
        "合成来源快照超过大小上限"
    );
    assert_eq!(
        source
            .query_row("SELECT length(value) FROM content", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        262144
    );
}

#[cfg(windows)]
#[test]
fn learning_import_wal_header_gate_rejects_invalid_page_sizes_before_sqlite() {
    let mut database = [0u8; 100];
    database[..16].copy_from_slice(b"SQLite format 3\0");
    database[16..18].copy_from_slice(&4096u16.to_be_bytes());
    database[18] = 2;
    database[19] = 2;
    let mut wal = [0u8; 32];
    wal[..4].copy_from_slice(&0x377f0682u32.to_be_bytes());
    wal[4..8].copy_from_slice(&3007000u32.to_be_bytes());
    wal[8..12].copy_from_slice(&4096u32.to_be_bytes());
    assert!(super::source_sqlite::validate_wal_headers(&database, &wal, 32).is_ok());
    assert!(super::source_sqlite::validate_wal_headers(&database, &[], 0).is_ok());
    for invalid in [0u32, 1, 513, 8192, 131072] {
        let mut bad = wal;
        bad[8..12].copy_from_slice(&invalid.to_be_bytes());
        assert!(super::source_sqlite::validate_wal_headers(&database, &bad, 32).is_err());
    }
    assert!(super::source_sqlite::validate_wal_headers(&database, &wal[..12], 12).is_err());
    let mut bad_magic = wal;
    bad_magic[0] = 0;
    assert!(super::source_sqlite::validate_wal_headers(&database, &bad_magic, 32).is_err());
    let mut bad_version = wal;
    bad_version[4..8].copy_from_slice(&0u32.to_be_bytes());
    assert!(super::source_sqlite::validate_wal_headers(&database, &bad_version, 32).is_err());
    let mut bad_mode = database;
    bad_mode[19] = 1;
    assert!(super::source_sqlite::validate_wal_headers(&bad_mode, &wal, 32).is_err());
    database[16..18].copy_from_slice(&1u16.to_be_bytes());
    wal[8..12].copy_from_slice(&65536u32.to_be_bytes());
    assert!(super::source_sqlite::validate_wal_headers(&database, &wal, 32).is_ok());
}

#[cfg(windows)]
#[test]
fn learning_import_wal_header_gate_stops_corrupt_copy_before_snapshot() {
    use std::io::{Seek, SeekFrom, Write};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'synthetic')", [])
        .unwrap();
    writer
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    drop(writer);
    let wal_path = temp.path().join("synthetic.db-wal");
    let mut wal = fs::OpenOptions::new().write(true).open(&wal_path).unwrap();
    wal.seek(SeekFrom::Start(8)).unwrap();
    wal.write_all(&0u32.to_be_bytes()).unwrap();
    drop(wal);
    let before = [
        fs::read(&path).unwrap(),
        fs::read(&wal_path).unwrap(),
        fs::read(temp.path().join("synthetic.db-shm")).unwrap(),
    ];
    let error = match super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {},
    ) {
        Ok(_) => panic!("invalid WAL was accepted"),
        Err(error) => error,
    };
    assert_eq!(error, "来源数据库或 WAL 文件头无效");
    assert_eq!(
        before,
        [
            fs::read(&path).unwrap(),
            fs::read(&wal_path).unwrap(),
            fs::read(temp.path().join("synthetic.db-shm")).unwrap()
        ]
    );
}

#[cfg(windows)]
#[test]
fn learning_import_sqlite_selection_rejects_profile_for_different_file() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_profiles, source_sqlite};
    let temp = tempfile::tempdir().unwrap();
    let state = crate::state::test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let first = temp.path().join("first.db");
    let second = temp.path().join("second.db");
    let _writer_first = source(&first);
    let _writer_second = source(&second);
    let bound = source_sqlite::bind_selected(&state, &scope, &first).unwrap();
    let choices = SyntheticSqliteSelections::default();
    let unbound = source_profiles::create(&state, &scope).unwrap();
    assert!(choices
        .select(&state, &library, &unbound.id, &unbound.access_epoch, || {
            Some(first.clone())
        })
        .is_err());
    assert!(choices
        .select(
            &state,
            &library,
            bound.profile_id(),
            bound.epoch(),
            || Some(second)
        )
        .is_err());
    let ticket = choices
        .select(&state, &library, bound.profile_id(), bound.epoch(), || {
            Some(first.clone())
        })
        .unwrap()
        .unwrap();
    fs::hard_link(&first, temp.path().join("linked.db")).unwrap();
    assert!(choices
        .snapshot(&state, &ticket, Duration::from_secs(2), || {})
        .is_err());
    fs::remove_file(temp.path().join("linked.db")).unwrap();
    assert!(choices
        .snapshot(&state, &ticket, Duration::from_secs(2), || {
            fs::hard_link(&first, temp.path().join("linked-late.db")).unwrap();
        })
        .is_err());
}

#[cfg(windows)]
#[test]
fn learning_import_sqlite_selection_requires_current_choice_scope_and_epoch() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_profiles, source_sqlite};
    let temp = tempfile::tempdir().unwrap();
    let state = crate::state::test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let path = temp.path().join("selected.db");
    let writer = source(&path);
    let bound = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    writer
        .execute("INSERT INTO video_notes VALUES(1,'selected')", [])
        .unwrap();
    let choices = SyntheticSqliteSelections::default();
    assert!(choices
        .snapshot(&state, "unselected", Duration::from_secs(2), || {})
        .is_err());
    let first = choices
        .select(&state, &library, &profile.id, &profile.access_epoch, || {
            Some(path.clone())
        })
        .unwrap()
        .unwrap();
    let result = choices
        .snapshot(&state, &first, Duration::from_secs(2), || {})
        .unwrap();
    assert_eq!(
        result
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(choices
        .select(&state, &library, &profile.id, &profile.access_epoch, || {
            None
        })
        .unwrap()
        .is_none());
    assert!(choices
        .snapshot(&state, &first, Duration::from_secs(2), || {})
        .is_err());
    let second = choices
        .select(&state, &library, &profile.id, &profile.access_epoch, || {
            Some(path.clone())
        })
        .unwrap()
        .unwrap();
    state
        .with_library(|db, _| source_profiles::invalidate_epochs(db))
        .unwrap();
    assert!(choices
        .snapshot(&state, &second, Duration::from_secs(2), || {})
        .is_err());
    let current = source_profiles::get(&state, &scope, &profile.id).unwrap();
    let third = choices
        .select(&state, &library, &profile.id, &current.access_epoch, || {
            Some(path.clone())
        })
        .unwrap()
        .unwrap();
    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    assert!(choices
        .snapshot(&state, &third, Duration::from_secs(2), || {})
        .is_err());
}

#[cfg(windows)]
#[test]
fn learning_import_sqlite_selection_rejects_reselection_during_snapshot() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_profiles, source_sqlite};
    let temp = tempfile::tempdir().unwrap();
    let state = crate::state::test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let path = temp.path().join("selected.db");
    let _writer = source(&path);
    let bound = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    let choices = SyntheticSqliteSelections::default();
    let first = choices
        .select(&state, &library, &profile.id, &profile.access_epoch, || {
            Some(path.clone())
        })
        .unwrap()
        .unwrap();
    assert!(choices
        .snapshot(&state, &first, Duration::from_secs(2), || {
            assert!(choices
                .select(&state, &library, &profile.id, &profile.access_epoch, || {
                    None
                })
                .unwrap()
                .is_none());
        })
        .is_err());
}

#[cfg(windows)]
#[test]
fn learning_import_sqlite_selection_rejects_stale_picker_and_mid_read_epoch() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_profiles, source_sqlite};
    let temp = tempfile::tempdir().unwrap();
    let state = crate::state::test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let path = temp.path().join("selected.db");
    let _writer = source(&path);
    let bound = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    let choices = SyntheticSqliteSelections::default();
    let latest = std::cell::RefCell::new(String::new());
    let stale = choices.select(&state, &library, &profile.id, &profile.access_epoch, || {
        *latest.borrow_mut() = choices
            .select(&state, &library, &profile.id, &profile.access_epoch, || {
                Some(path.clone())
            })
            .unwrap()
            .unwrap();
        Some(path.clone())
    });
    assert!(stale.is_err());
    assert!(choices
        .snapshot(&state, &latest.borrow(), Duration::from_secs(2), || {
            state
                .with_library(|db, _| source_profiles::invalidate_epochs(db))
                .unwrap();
        })
        .is_err());
    let current = source_profiles::get(&state, &scope, &profile.id).unwrap();
    assert!(choices
        .select(&state, &library, &profile.id, &current.access_epoch, || {
            *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
            Some(path.clone())
        })
        .is_err());
}

#[cfg(windows)]
#[test]
fn learning_import_pinned_snapshot_accepts_quiet_database_without_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.sqlite");
    let bytes = include_bytes!("fixtures/yingliu-migration-sample.sqlite");
    fs::write(&path, bytes).unwrap();
    let snapshot =
        super::import_snapshot::snapshot_pinned_synthetic(&path, Duration::from_secs(2), || {
            assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
            assert!(fs::rename(&path, temp.path().join("renamed.sqlite")).is_err());
        })
        .unwrap();
    let target = crate::db::open_database(&temp.path().join("target.db")).unwrap();
    let preview = super::import_course_preview::preview_courses(&snapshot, &target, &[]).unwrap();
    assert_eq!(preview.series[0].id, "series-1");
    assert_eq!(
        preview.series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        11000
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(!temp
            .path()
            .join(format!("synthetic.sqlite{suffix}"))
            .exists());
    }
}

#[cfg(windows)]
#[test]
fn learning_import_quiet_snapshot_rejects_sidecar_race_and_writer() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.sqlite");
    let bytes = include_bytes!("fixtures/yingliu-migration-sample.sqlite");
    fs::write(&path, bytes).unwrap();
    let sidecar = temp.path().join("synthetic.sqlite-wal");
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {
            fs::write(&sidecar, b"new wal").unwrap();
        }
    )
    .is_err());
    assert_eq!(fs::read(&sidecar).unwrap(), b"new wal");
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {}
    )
    .is_err());
    fs::remove_file(sidecar).unwrap();
    let journal = temp.path().join("synthetic.sqlite-journal");
    fs::write(&journal, b"journal").unwrap();
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {}
    )
    .is_err());
    fs::remove_file(journal).unwrap();
    let writer = fs::OpenOptions::new().write(true).open(&path).unwrap();
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {}
    )
    .is_err());
    drop(writer);
    assert!(
        super::import_snapshot::snapshot_pinned_synthetic(&path, Duration::ZERO, || {}).is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[cfg(windows)]
#[test]
fn learning_import_quiet_snapshot_accepts_rollback_journal_mode() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.sqlite");
    fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let db = Connection::open(&path).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    drop(db);
    let before = fs::read(&path).unwrap();
    let snapshot =
        super::import_snapshot::snapshot_pinned_synthetic(&path, Duration::from_secs(2), || {})
            .unwrap();
    assert_eq!(
        snapshot
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        8
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(!temp
            .path()
            .join(format!("synthetic.sqlite{suffix}"))
            .exists());
    }
}

#[cfg(windows)]
#[test]
fn learning_import_pinned_snapshot_reads_wal_and_rejects_linked_source() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'wal')", [])
        .unwrap();
    let before = [
        fs::read(&path).unwrap(),
        fs::read(temp.path().join("synthetic.db-wal")).unwrap(),
        fs::read(temp.path().join("synthetic.db-shm")).unwrap(),
    ];
    let snapshot =
        super::import_snapshot::snapshot_pinned_synthetic(&path, Duration::from_secs(2), || {
            assert!(fs::rename(&path, temp.path().join("renamed.db")).is_err());
            assert!(fs::rename(
                temp.path().join("synthetic.db-wal"),
                temp.path().join("renamed.wal")
            )
            .is_err());
        })
        .unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        before,
        [
            fs::read(&path).unwrap(),
            fs::read(temp.path().join("synthetic.db-wal")).unwrap(),
            fs::read(temp.path().join("synthetic.db-shm")).unwrap()
        ]
    );
    fs::hard_link(&path, temp.path().join("other.db")).unwrap();
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {}
    )
    .is_err());
    drop(writer);
}

#[cfg(windows)]
#[test]
fn learning_import_pinned_wal_snapshot_rejects_rollback_journal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    writer
        .execute("INSERT INTO video_notes VALUES(1,'wal')", [])
        .unwrap();
    let journal = temp.path().join("synthetic.db-journal");
    fs::write(&journal, b"journal").unwrap();
    let mut called = false;
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {
            called = true;
        }
    )
    .is_err());
    assert!(!called);
    fs::remove_file(&journal).unwrap();
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {}
    )
    .is_ok());
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {
            fs::write(&journal, b"journal").unwrap();
        }
    )
    .is_err());
    assert_eq!(fs::read(&journal).unwrap(), b"journal");
    drop(writer);
}

#[cfg(windows)]
#[test]
fn learning_import_pinned_snapshot_rejects_changes_and_missing_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.db");
    let writer = source(&path);
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &path,
        Duration::from_secs(2),
        || {
            writer
                .execute("INSERT INTO video_notes VALUES(1,'changed')", [])
                .unwrap();
        }
    )
    .is_err());
    drop(writer);
    let empty = temp.path().join("missing.db");
    fs::write(&empty, b"not a database").unwrap();
    assert!(super::import_snapshot::snapshot_pinned_synthetic(
        &empty,
        Duration::from_secs(2),
        || {}
    )
    .is_err());
    assert!(!temp.path().join("missing.db-wal").exists());
    assert!(!temp.path().join("missing.db-shm").exists());
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

    #[cfg(windows)]
    {
        // Task12c14: copy-out under shared leases, then SHM-safe snapshot on the copy only.
        assert!(shm.is_file());
        let shm_before = fs::read(&shm).unwrap();
        let wal_live = fs::read(&wal).unwrap();
        let db_live = fs::read(&path).unwrap();
        assert!(snapshot_synthetic(&path, Duration::from_secs(2)).is_err());
        let copy_db = temp.path().join("sidecar-copy.db");
        let live_snapshot =
            snapshot_synthetic_via_sidecar_copy(&path, &copy_db, Duration::from_secs(2)).unwrap();
        assert_eq!(
            live_snapshot
                .query_row("SELECT count(*) FROM video_notes", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            2
        );
        assert_eq!(fs::read(&path).unwrap(), db_live);
        assert_eq!(fs::read(&wal).unwrap(), wal_live);
        assert_eq!(fs::read(&shm).unwrap(), shm_before);
        drop(live_snapshot);
        writer
            .execute(
                "INSERT INTO video_notes VALUES(3,'writer still active')",
                [],
            )
            .unwrap();
    }
    #[cfg(not(windows))]
    {
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
/// Task12c14: while a writer holds SHM writable, copy complete sidecars under shared-read
/// leases and snapshot only the copy — source SHM/WAL/DB bytes stay unchanged.
#[cfg(windows)]
#[test]
fn learning_import_windows_sidecar_copy_out_snapshots_wal_without_mutating_source() {
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
        .execute("INSERT INTO video_notes VALUES(1,'committed')", [])
        .unwrap();
    let (wal, shm) = sidecar_paths(&path);
    let shm_before = fs::read(&shm).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    let db_before = fs::read(&path).unwrap();
    assert!(snapshot_synthetic(&path, Duration::from_secs(2)).is_err());
    let copy_db = temp.path().join("out").join("copy.db");
    let snapshot =
        snapshot_synthetic_via_sidecar_copy(&path, &copy_db, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
    assert_eq!(fs::read(&shm).unwrap(), shm_before);
    writer
        .execute("INSERT INTO video_notes VALUES(2,'still writable')", [])
        .unwrap();
    drop(snapshot);
    drop(writer);
}

/// Missing sidecars refuse copy-out without creating files or opening SQLite on the source.
#[cfg(windows)]
#[test]
fn learning_import_windows_sidecar_copy_out_requires_complete_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("incomplete.db");
    {
        let writer = source(&path);
        writer
            .set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )
            .unwrap();
        writer
            .execute("INSERT INTO video_notes VALUES(1,'x')", [])
            .unwrap();
    }
    let (wal, shm) = sidecar_paths(&path);
    assert!(wal.exists() && shm.exists());
    fs::remove_file(&shm).unwrap();
    let copy_db = temp.path().join("copy.db");
    assert!(snapshot_synthetic_via_sidecar_copy(&path, &copy_db, Duration::from_secs(2)).is_err());
    assert!(!shm.exists());
    assert!(!copy_db.exists());
    let mut missing_shm = copy_db.as_os_str().to_os_string();
    missing_shm.push("-shm");
    assert!(!std::path::PathBuf::from(missing_shm).exists());
}
/// Task12c15: mutating a sidecar after the pre-copy fingerprint must fail closed and leave
/// no dest files (torn / mid-write detection via SHA-256+size).
#[cfg(windows)]
#[test]
fn learning_import_windows_sidecar_copy_rejects_torn_mid_write_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("torn.db");
    {
        let writer = source(&path);
        writer
            .set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )
            .unwrap();
        writer
            .execute("INSERT INTO video_notes VALUES(1,'quiet')", [])
            .unwrap();
    }
    let (wal, shm) = sidecar_paths(&path);
    let shm_before = fs::read(&shm).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    let db_before = fs::read(&path).unwrap();
    let copy_db = temp.path().join("dest").join("copy.db");
    // Deterministic torn window: flip SHM after pre-copy fingerprint, before byte copies.
    let err = copy_complete_sidecars_for_snapshot_with_interference(&path, &copy_db, |src| {
        let mut shm = src.as_os_str().to_os_string();
        shm.push("-shm");
        let shm_path = std::path::PathBuf::from(shm);
        let mut bytes = fs::read(&shm_path).unwrap();
        if let Some(b) = bytes.last_mut() {
            *b ^= 0xff;
        } else {
            bytes.push(1);
        }
        fs::write(&shm_path, bytes).unwrap();
    });
    assert!(err.is_err());
    assert!(!copy_db.exists());
    let mut missing_wal = copy_db.as_os_str().to_os_string();
    missing_wal.push("-wal");
    let mut missing_shm = copy_db.as_os_str().to_os_string();
    missing_shm.push("-shm");
    assert!(!std::path::PathBuf::from(missing_wal).exists());
    assert!(!std::path::PathBuf::from(missing_shm).exists());
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
    assert_ne!(fs::read(&shm).unwrap(), shm_before);
}

/// Quiet-point copy still succeeds under the consistency gate and snapshots WAL rows.
#[cfg(windows)]
#[test]
fn learning_import_windows_sidecar_copy_accepts_stable_fingerprint_set() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("stable.db");
    let writer = source(&path);
    writer
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    writer
        .execute("INSERT INTO video_notes VALUES(1,'stable')", [])
        .unwrap();
    let (wal, shm) = sidecar_paths(&path);
    let before = (
        fs::read(&path).unwrap(),
        fs::read(&wal).unwrap(),
        fs::read(&shm).unwrap(),
    );
    let copy_db = temp.path().join("ok.db");
    let snapshot =
        snapshot_synthetic_via_sidecar_copy(&path, &copy_db, Duration::from_secs(2)).unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM video_notes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        (
            fs::read(&path).unwrap(),
            fs::read(&wal).unwrap(),
            fs::read(&shm).unwrap(),
        ),
        before
    );
    drop(snapshot);
    drop(writer);
}
