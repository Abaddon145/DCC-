use super::course_snapshot::{
    open_isolated_readonly, SYNTHETIC_COURSE_MARKER, SYNTHETIC_COURSE_MARKER_BODY,
};
use super::course_source_readonly::{
    preview_synthetic_course_source, release_synthetic_course_source,
    select_synthetic_course_source,
};
use crate::state::test_state;
use rusqlite::{config::DbConfig, Connection, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const FIXTURE: &[u8] = include_bytes!("fixtures/yingliu-migration-sample.sqlite");

fn mark(dir: &Path) {
    fs::write(
        dir.join(SYNTHETIC_COURSE_MARKER),
        SYNTHETIC_COURSE_MARKER_BODY,
    )
    .unwrap();
}

fn write_fixture(dir: &Path, name: &str) -> PathBuf {
    mark(dir);
    let path = dir.join(name);
    fs::write(&path, FIXTURE).unwrap();
    path
}

fn sidecars(path: &Path) -> [PathBuf; 3] {
    let mut names = Vec::new();
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        names.push(PathBuf::from(name));
    }
    names.try_into().unwrap()
}

fn assert_no_sidecars(path: &Path) {
    for sidecar in sidecars(path) {
        assert!(!sidecar.exists(), "created {}", sidecar.display());
    }
}

#[test]
fn synthetic_course_preview_reads_schema8_fixture_without_opening_source() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let before = fs::read(&path).unwrap();
    let courses_before: i64 = state
        .with_library(|db, _| {
            db.query_row("SELECT count(*) FROM learning_courses", [], |row| {
                row.get(0)
            })
            .map_err(|error| error.to_string())
        })
        .unwrap();
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let preview = preview_synthetic_course_source(&state, &library, &ticket.selection_id).unwrap();
    let preview = preview.preview();
    assert_eq!(preview.series[0].id, "series-1");
    assert_eq!(preview.series[0].title, "合成系列");
    assert_eq!(
        preview.series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        11000
    );
    let listed: String = {
        let snapshot = open_isolated_readonly(&path, Duration::from_secs(2), || {}).unwrap();
        snapshot
            .query_row("PRAGMA database_list", [], |row| row.get(2))
            .unwrap()
    };
    assert!(
        listed.is_empty(),
        "snapshot stayed on the source file: {listed}"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);
    let courses_after: i64 = state
        .with_library(|db, _| {
            db.query_row("SELECT count(*) FROM learning_courses", [], |row| {
                row.get(0)
            })
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(courses_before, courses_after);
}

#[test]
fn synthetic_course_gate_refuses_unmarked_and_relative_paths_before_sqlite() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = temp.path().join("vault.sqlite");
    fs::write(&path, FIXTURE).unwrap();
    let error = select_synthetic_course_source(&state, &library, &path).unwrap_err();
    assert!(error.contains("尚未授权"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), FIXTURE);
    assert_no_sidecars(&path);
    let missing = temp.path().join("missing.sqlite");
    let missing_error = select_synthetic_course_source(&state, &library, &missing).unwrap_err();
    assert!(missing_error.contains("尚未授权"), "{missing_error}");
    assert!(!missing.exists());
    let relative = select_synthetic_course_source(&state, &library, Path::new("synthetic.sqlite"))
        .unwrap_err();
    assert!(relative.contains("尚未授权"), "{relative}");
    fs::write(
        temp.path().join(SYNTHETIC_COURSE_MARKER),
        b"dcc-synthetic-course-source-v0\n",
    )
    .unwrap();
    let wrong = select_synthetic_course_source(&state, &library, &path).unwrap_err();
    assert!(wrong.contains("尚未授权"), "{wrong}");
    assert_eq!(fs::read(&path).unwrap(), FIXTURE);
    assert_no_sidecars(&path);
}

#[test]
fn synthetic_course_gate_rejects_sidecar_symlink_and_hardlink_shapes() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let [wal, shm, journal] = sidecars(&path);
    fs::write(&journal, b"rollback").unwrap();
    assert!(select_synthetic_course_source(&state, &library, &path).is_err());
    assert_eq!(fs::read(&journal).unwrap(), b"rollback");
    fs::remove_file(&journal).unwrap();
    fs::write(&wal, b"only-wal").unwrap();
    assert!(select_synthetic_course_source(&state, &library, &path).is_err());
    assert!(!shm.exists());
    fs::remove_file(&wal).unwrap();
    fs::write(&shm, vec![0u8; 32]).unwrap();
    assert!(select_synthetic_course_source(&state, &library, &path).is_err());
    fs::remove_file(&shm).unwrap();

    let outside = tempfile::tempdir().unwrap();
    let real = outside.path().join("real.sqlite");
    fs::write(&real, FIXTURE).unwrap();
    let link = temp.path().join("linked.sqlite");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&real, &link).unwrap();
    assert!(select_synthetic_course_source(&state, &library, &link).is_err());
    assert_eq!(fs::read(&real).unwrap(), FIXTURE);

    let alias = temp.path().join("alias.sqlite");
    fs::hard_link(&path, &alias).unwrap();
    assert!(select_synthetic_course_source(&state, &library, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), FIXTURE);
    fs::remove_file(&alias).unwrap();
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    assert_eq!(
        preview_synthetic_course_source(&state, &library, &ticket.selection_id)
            .unwrap()
            .preview()
            .series[0]
            .id,
        "series-1"
    );
}

#[test]
fn synthetic_course_gate_rejects_bad_header_non_schema8_timeout_and_torn_reread() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    mark(temp.path());
    let garbage = temp.path().join("garbage.sqlite");
    fs::write(&garbage, b"not a database").unwrap();
    let header = select_synthetic_course_source(&state, &library, &garbage).unwrap_err();
    assert!(header.contains("文件头"), "{header}");
    assert_eq!(fs::read(&garbage).unwrap(), b"not a database");

    let wrong = temp.path().join("schema7.sqlite");
    let db = Connection::open(&wrong).unwrap();
    db.execute_batch(
        "CREATE TABLE schema_version(version INTEGER NOT NULL);
         INSERT INTO schema_version VALUES(7);
         PRAGMA user_version=7;",
    )
    .unwrap();
    drop(db);
    let before = fs::read(&wrong).unwrap();
    let ticket = select_synthetic_course_source(&state, &library, &wrong).unwrap();
    assert!(preview_synthetic_course_source(&state, &library, &ticket.selection_id).is_err());
    assert_eq!(fs::read(&wrong).unwrap(), before);

    let path = temp.path().join("quiet.sqlite");
    fs::write(&path, FIXTURE).unwrap();
    assert!(open_isolated_readonly(&path, Duration::ZERO, || {}).is_err());
    assert_eq!(fs::read(&path).unwrap(), FIXTURE);
    assert_no_sidecars(&path);
    let torn = open_isolated_readonly(&path, Duration::from_secs(2), || {
        fs::write(&path, b"torn").unwrap();
    });
    assert!(torn.is_err());
    assert_eq!(fs::read(&path).unwrap(), b"torn");
}

#[test]
fn synthetic_course_preview_reads_uncheckpointed_wal_and_keeps_source_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    mark(temp.path());
    let path = temp.path().join("wal.sqlite");
    let db = Connection::open(&path).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    db.execute_batch(
        "CREATE TABLE schema_version(version INTEGER NOT NULL);
         INSERT INTO schema_version VALUES(8);
         PRAGMA user_version=8;
         CREATE TABLE series(id TEXT PRIMARY KEY, title TEXT NOT NULL, description TEXT, sort_order INTEGER NOT NULL);
         CREATE TABLE series_items(id TEXT PRIMARY KEY, series_id TEXT NOT NULL, title TEXT NOT NULL, duration_ms INTEGER, sort_order INTEGER NOT NULL);
         CREATE TABLE playback_progress(item_id TEXT PRIMARY KEY, position_ms INTEGER NOT NULL, duration_ms INTEGER, playback_rate REAL NOT NULL, completed INTEGER NOT NULL, last_played_at INTEGER, updated_at INTEGER NOT NULL, last_sequence INTEGER NOT NULL);",
    )
    .unwrap();
    let _: (i64, i64, i64) = db
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    db.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    db.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .unwrap();
    db.execute(
        "INSERT INTO series(id,title,description,sort_order) VALUES('wal-series','仅 WAL','',1)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO series_items(id,series_id,title,duration_ms,sort_order) VALUES('wal-course','wal-series','WAL 课程',1000,1)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO playback_progress(item_id,position_ms,duration_ms,playback_rate,completed,updated_at,last_sequence) VALUES('wal-course',4242,1000,1,0,1,1)",
        [],
    )
    .unwrap();
    drop(db);
    let wal = {
        let mut name = path.as_os_str().to_os_string();
        name.push("-wal");
        PathBuf::from(name)
    };
    let shm = {
        let mut name = path.as_os_str().to_os_string();
        name.push("-shm");
        PathBuf::from(name)
    };
    assert!(!fs::read(&wal).unwrap().is_empty());
    let mut uri = url::Url::from_file_path(&path).unwrap();
    uri.query_pairs_mut()
        .append_pair("mode", "ro")
        .append_pair("immutable", "1");
    let immutable = Connection::open_with_flags(
        uri.as_str(),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    let visible: i64 = immutable
        .query_row(
            "SELECT count(*) FROM series WHERE id='wal-series'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(visible, 0, "main database already contained the WAL row");
    drop(immutable);
    let db_before = fs::read(&path).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    let shm_before = fs::read(&shm).unwrap();
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let preview = preview_synthetic_course_source(&state, &library, &ticket.selection_id)
        .unwrap()
        .preview()
        .clone();
    assert_eq!(preview.series.len(), 1);
    assert_eq!(preview.series[0].id, "wal-series");
    assert_eq!(preview.series[0].items[0].id, "wal-course");
    assert_eq!(
        preview.series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        4242
    );
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
    assert_eq!(fs::read(&shm).unwrap(), shm_before);
}

#[test]
fn synthetic_course_preview_drops_revoked_epoch_and_replaced_selection() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let first = select_synthetic_course_source(&state, &library, &path).unwrap();
    assert!(release_synthetic_course_source(&state, &library, &first.selection_id).unwrap());
    assert!(preview_synthetic_course_source(&state, &library, &first.selection_id).is_err());
    let second = select_synthetic_course_source(&state, &library, &path).unwrap();
    let third = select_synthetic_course_source(&state, &library, &path).unwrap();
    assert!(preview_synthetic_course_source(&state, &library, &second.selection_id).is_err());
    state
        .with_library(|db, _| super::source_profiles::invalidate_epochs(db))
        .unwrap();
    assert!(preview_synthetic_course_source(&state, &library, &third.selection_id).is_err());
    let current = select_synthetic_course_source(&state, &library, &path).unwrap();
    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    assert!(preview_synthetic_course_source(&state, &library, &current.selection_id).is_err());
    assert_eq!(fs::read(&path).unwrap(), FIXTURE);
    assert_no_sidecars(&path);
}
