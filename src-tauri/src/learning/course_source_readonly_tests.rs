use super::course_snapshot::{
    open_isolated_readonly, SYNTHETIC_COURSE_MARKER, SYNTHETIC_COURSE_MARKER_BODY,
};
use super::course_source_readonly::{
    approve_marked_course_preflight, approve_synthetic_course_preflight,
    commit_synthetic_course_while_copying, current_marked_course_source,
    expire_synthetic_course_preflight, forget_synthetic_course_process_memory,
    list_marked_course_series, preview_synthetic_course_source, queue_synthetic_course_commit,
    release_synthetic_course_source, select_marked_course_with_picker,
    select_synthetic_course_source,
};
use super::import_course_preview::CourseMediaBinding;
use super::import_runner::ImportTaskRequest;
use crate::state::test_state;
use crate::tasks::TaskRequest;
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
        let write = fs::write(&path, b"torn");
        if cfg!(windows) {
            assert!(
                write.is_err(),
                "deny-write lease must block an in-window database writer"
            );
        } else {
            write.expect("linux has no deny-write lease; the torn write must land");
        }
    });
    if cfg!(windows) {
        let snapshot = torn.expect("unchanged bytes stay readable while the writer is blocked");
        let version: i64 = snapshot
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 8);
        drop(snapshot);
        assert_eq!(fs::read(&path).unwrap(), FIXTURE);
        assert_no_sidecars(&path);
    } else {
        assert!(torn.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"torn");
    }
}

#[test]
fn synthetic_course_gate_rejects_marker_or_sidecar_that_appears_and_disappears() {
    let temp = tempfile::tempdir().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let before = fs::read(&path).unwrap();
    let marker = temp.path().join(SYNTHETIC_COURSE_MARKER);
    let marker_before = fs::read(&marker).unwrap();
    let [wal, shm, journal] = sidecars(&path);
    let transient = open_isolated_readonly(&path, Duration::from_secs(2), || {
        fs::write(&wal, b"transient-wal").unwrap();
        fs::write(&shm, vec![7u8; 32]).unwrap();
        fs::write(&journal, b"transient-journal").unwrap();
        fs::remove_file(&wal).unwrap();
        fs::remove_file(&shm).unwrap();
        fs::remove_file(&journal).unwrap();
        fs::remove_file(&marker).unwrap();
        fs::write(&marker, marker_before.as_slice()).unwrap();
    });
    let error = transient.unwrap_err();
    assert!(
        error.contains("无法只读隔离"),
        "transient names were accepted: {error}"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read(&marker).unwrap(), marker_before);
    assert_no_sidecars(&path);

    let mut marker_file = fs::OpenOptions::new().write(true).open(&marker).unwrap();
    let rewritten = open_isolated_readonly(&path, Duration::from_secs(2), || {
        use std::io::Write;
        marker_file.write_all(marker_before.as_slice()).unwrap();
        marker_file.sync_all().unwrap();
    });
    let rewritten_error = rewritten.unwrap_err();
    assert!(
        rewritten_error.contains("无法只读隔离"),
        "rewritten marker was accepted: {rewritten_error}"
    );
    drop(marker_file);
    assert_eq!(fs::read(&marker).unwrap(), marker_before);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);

    let snapshot = open_isolated_readonly(&path, Duration::from_secs(2), || {}).unwrap();
    let version: i64 = snapshot
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 8);
    drop(snapshot);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);
}

/// Endpoint file and directory stamps match the pre-rewrite identity. The gate
/// still fails because the parent watch saw the in-place write.
#[cfg(target_os = "linux")]
#[test]
fn synthetic_course_gate_rejects_inplace_rewrite_when_mtime_is_restored() {
    use std::cell::Cell;
    use std::io::{Seek, SeekFrom, Write};
    use std::os::unix::fs::MetadataExt;

    let temp = tempfile::tempdir().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let before = fs::read(&path).unwrap();
    let restored = Cell::new(false);
    let error = open_isolated_readonly(&path, Duration::from_secs(2), || {
        let file_before = fs::symlink_metadata(&path).unwrap();
        let dir_before = fs::symlink_metadata(temp.path()).unwrap();
        let mut flipped = before.clone();
        flipped[100] ^= 0xff;
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.write_all(&flipped).unwrap();
        file.sync_all().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&before).unwrap();
        file.sync_all().unwrap();
        drop(file);
        restore_mtime(&path, file_before.mtime(), file_before.mtime_nsec());
        let file_after = fs::symlink_metadata(&path).unwrap();
        let dir_after = fs::symlink_metadata(temp.path()).unwrap();
        assert_eq!(file_after.ino(), file_before.ino());
        assert_eq!(file_after.len(), file_before.len());
        assert_eq!(file_after.nlink(), file_before.nlink());
        assert_eq!(file_after.mtime(), file_before.mtime());
        assert_eq!(file_after.mtime_nsec(), file_before.mtime_nsec());
        assert_eq!(dir_after.dev(), dir_before.dev());
        assert_eq!(dir_after.ino(), dir_before.ino());
        assert_eq!(dir_after.len(), dir_before.len());
        assert_eq!(dir_after.mtime(), dir_before.mtime());
        assert_eq!(dir_after.mtime_nsec(), dir_before.mtime_nsec());
        assert_eq!(dir_after.ctime(), dir_before.ctime());
        assert_eq!(dir_after.ctime_nsec(), dir_before.ctime_nsec());
        assert_eq!(fs::read(&path).unwrap(), before);
        restored.set(true);
    })
    .unwrap_err();
    assert!(
        restored.get(),
        "gate failed before the restored rewrite: {error}"
    );
    assert!(error.contains("无法只读隔离"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);
}

#[cfg(target_os = "linux")]
fn restore_mtime(path: &Path, sec: i64, nsec: i64) {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    let mut raw = Vec::with_capacity(bytes.len() + 1);
    raw.extend_from_slice(bytes);
    raw.push(0);
    let times = [
        libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
        libc::timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        },
    ];
    let rc = unsafe {
        libc::utimensat(
            libc::AT_FDCWD,
            raw.as_ptr().cast::<libc::c_char>(),
            times.as_ptr(),
            0,
        )
    };
    assert_eq!(rc, 0, "{}", std::io::Error::last_os_error());
}

/// Directory creation and last-write times are put back after `-wal` / `-shm` /
/// `-journal` appear and disappear. Windows stamps would match; the watch must
/// still fail closed. Not compiled on Linux.
#[cfg(windows)]
#[test]
fn windows_directory_watch_rejects_sidecar_churn_when_directory_times_are_restored() {
    use std::cell::Cell;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::Storage::FileSystem::{
        SetFileTime, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
    };

    let temp = tempfile::tempdir().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let before = fs::read(&path).unwrap();
    let [wal, shm, journal] = sidecars(&path);
    let restored = Cell::new(false);
    let error = open_isolated_readonly(&path, Duration::from_secs(2), || {
        let dir_before = fs::symlink_metadata(temp.path()).unwrap();
        let created = dir_before.creation_time();
        let written = dir_before.last_write_time();
        fs::write(&wal, b"transient-wal").unwrap();
        fs::write(&shm, vec![7u8; 32]).unwrap();
        fs::write(&journal, b"transient-journal").unwrap();
        fs::remove_file(&wal).unwrap();
        fs::remove_file(&shm).unwrap();
        fs::remove_file(&journal).unwrap();
        let filetime = |value: u64| FILETIME {
            dwLowDateTime: value as u32,
            dwHighDateTime: (value >> 32) as u32,
        };
        let created_time = filetime(created);
        let written_time = filetime(written);
        let mut options = fs::OpenOptions::new();
        options
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
        let directory = options.open(temp.path()).unwrap();
        let set = unsafe {
            SetFileTime(
                directory.as_raw_handle(),
                &created_time,
                std::ptr::null(),
                &written_time,
            )
        };
        assert_ne!(set, 0, "{}", std::io::Error::last_os_error());
        drop(directory);
        let dir_after = fs::symlink_metadata(temp.path()).unwrap();
        assert_eq!(dir_after.creation_time(), created);
        assert_eq!(dir_after.last_write_time(), written);
        assert_eq!(fs::read(&path).unwrap(), before);
        restored.set(true);
    })
    .unwrap_err();
    assert!(
        restored.get(),
        "gate failed before the restored sidecar churn: {error}"
    );
    assert!(error.contains("无法只读隔离"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);
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

fn courses(state: &crate::state::AppState) -> i64 {
    state
        .with_library(|db, _| {
            db.query_row("SELECT count(*) FROM learning_courses", [], |row| {
                row.get(0)
            })
            .map_err(|error| error.to_string())
        })
        .unwrap()
}

fn batches(state: &crate::state::AppState) -> i64 {
    state
        .with_library(|db, _| {
            db.query_row("SELECT count(*) FROM learning_import_batches", [], |row| {
                row.get(0)
            })
            .map_err(|error| error.to_string())
        })
        .unwrap()
}

fn insert_video(state: &crate::state::AppState) -> String {
    let media = uuid::Uuid::new_v4().to_string();
    let file = uuid::Uuid::new_v4().to_string();
    state
        .with_library(|db, _| {
            db.execute(
                "INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','synthetic',?2,'now','now')",
                rusqlite::params![media, file],
            )
            .unwrap();
            db.execute(
                "INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main',?3,'synthetic.mp4',?3,'now')",
                rusqlite::params![file, media, format!("media-library/video/{media}/video.mp4")],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    media
}

fn bind(item: &str, media: &str) -> CourseMediaBinding {
    CourseMediaBinding {
        source_item_id: item.into(),
        media_id: media.into(),
    }
}

fn run_task(
    state: &crate::state::AppState,
    task: &crate::tasks::LibraryTask,
) -> Result<serde_json::Value, String> {
    let TaskRequest::LearningImport(request) = &task.payload else {
        return Err("任务类型无效".into());
    };
    super::import_runner::run_local(state, task, request)
}

#[test]
fn synthetic_course_preflight_does_not_write_targets_and_rejects_path_media() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let before = fs::read(&path).unwrap();
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    assert_eq!(
        preview_synthetic_course_source(&state, &library, &ticket.selection_id)
            .unwrap()
            .preview()
            .series[0]
            .id,
        "series-1"
    );
    assert!(queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        &uuid::Uuid::new_v4().to_string()
    )
    .is_err());
    let path_media = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", "C:/videos/lesson.mp4")],
    )
    .unwrap_err();
    assert!(path_media.contains("绑定"), "{path_media}");
    let missing = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &uuid::Uuid::new_v4().to_string())],
    )
    .unwrap_err();
    assert!(missing.contains("目标媒体"), "{missing}");
    assert_eq!(courses(&state), 0);
    assert_eq!(batches(&state), 0);
    let media = insert_video(&state);
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    assert_eq!(preview["selectionId"], ticket.selection_id);
    assert_eq!(preview["series"][0]["title"], "合成系列");
    let course = preview["series"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "course-1")
        .unwrap();
    assert_eq!(course["mediaId"], media);
    assert_eq!(course["sourceProgress"]["positionMs"], 11000);
    assert!(course["targetProgress"].is_null());
    let other = preview["series"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "course-2")
        .unwrap();
    let codes = other["conflictCodes"].as_array().unwrap();
    assert!(codes.iter().any(|code| code == "unresolved"));
    assert!(codes.iter().any(|code| code == "progress_absent"));
    let expires = preview["expiresAt"].as_i64().unwrap();
    let now = chrono::Utc::now().timestamp();
    assert!(expires > now && expires - now <= 900);
    assert_eq!(courses(&state), 0);
    assert_eq!(batches(&state), 0);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);
    let stale = preview["previewId"].as_str().unwrap().to_string();
    assert!(approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &uuid::Uuid::new_v4().to_string())]
    )
    .is_err());
    assert!(queue_synthetic_course_commit(&state, &library, &ticket.selection_id, &stale).is_err());
    assert_eq!(courses(&state), 0);
}

#[test]
fn synthetic_course_commit_keeps_source_bytes_and_reuses_revert() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let before = fs::read(&path).unwrap();
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let media = insert_video(&state);
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    let preview_id = preview["previewId"].as_str().unwrap();
    let (task, launch) =
        queue_synthetic_course_commit(&state, &library, &ticket.selection_id, preview_id).unwrap();
    assert!(launch);
    assert!(crate::tasks::ensure_generic_ipc_payload(&task.payload).is_err());
    let (duplicate, again) =
        queue_synthetic_course_commit(&state, &library, &ticket.selection_id, preview_id).unwrap();
    assert_eq!(task.id, duplicate.id);
    assert!(!again);
    let result = run_task(&state, &task).unwrap();
    assert_eq!(result["status"], "committed");
    assert_eq!(result["receipt"]["coursesCommitted"], 1);
    assert_eq!(result["receipt"]["courseConflicts"], 1);
    let stored = state.tasks.get(&task.id).unwrap();
    assert_eq!(stored.result.unwrap()["batchId"], result["batchId"]);
    let position: i64 = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT position_ms FROM learning_courses WHERE media_entry_id=?1",
                [&media],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(position, 11000);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);
    assert!(run_task(&state, &task).is_err());
    assert!(
        queue_synthetic_course_commit(&state, &library, &ticket.selection_id, preview_id).is_err()
    );
    assert_eq!(courses(&state), 1);
    let scope = super::source_profiles::scope(&state, &library).unwrap();
    let report = super::course_apply::revert_course_batch(
        &state,
        &scope,
        &ticket.epoch,
        result["batchId"].as_str().unwrap(),
    )
    .unwrap();
    assert!(!report["reverted"].as_array().unwrap().is_empty());
    assert_eq!(courses(&state), 0);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);
}

#[test]
fn synthetic_course_commit_reads_wal_without_changing_source_bytes() {
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
    let [wal, shm, _] = sidecars(&path);
    let db_before = fs::read(&path).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    let shm_before = fs::read(&shm).unwrap();
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let media = insert_video(&state);
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("wal-course", &media)],
    )
    .unwrap();
    assert_eq!(
        preview["series"][0]["items"][0]["sourceProgress"]["positionMs"],
        4242
    );
    let (task, _) = queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap(),
    )
    .unwrap();
    let result = run_task(&state, &task).unwrap();
    assert_eq!(result["status"], "committed");
    assert_eq!(result["receipt"]["coursesCommitted"], 1);
    let position: i64 = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT position_ms FROM learning_courses WHERE media_entry_id=?1",
                [&media],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(position, 4242);
    assert_eq!(fs::read(&path).unwrap(), db_before);
    assert_eq!(fs::read(&wal).unwrap(), wal_before);
    assert_eq!(fs::read(&shm).unwrap(), shm_before);
}

#[test]
fn synthetic_course_commit_rejects_torn_copy_and_source_or_target_changes() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let media = insert_video(&state);
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    let preview_id = preview["previewId"].as_str().unwrap().to_string();
    let (task, _) =
        queue_synthetic_course_commit(&state, &library, &ticket.selection_id, &preview_id).unwrap();
    let torn = commit_synthetic_course_while_copying(
        &state,
        &library,
        &ticket.selection_id,
        &preview_id,
        &task.id,
        || false,
        || {
            let write = fs::write(&path, b"torn");
            if cfg!(windows) {
                assert!(
                    write.is_err(),
                    "deny-write lease must block an in-window database writer"
                );
            } else {
                write.expect("linux has no deny-write lease; the torn write must land");
            }
        },
    );
    if cfg!(windows) {
        assert!(
            torn.is_ok(),
            "unchanged bytes can still commit while the writer is blocked: {torn:?}"
        );
        assert_eq!(fs::read(&path).unwrap(), FIXTURE);
        assert_no_sidecars(&path);
    } else {
        assert!(torn.is_err(), "{torn:?}");
        assert_eq!(courses(&state), 0);
        assert_eq!(fs::read(&path).unwrap(), b"torn");
    }
    assert!(run_task(&state, &task).is_err());

    let fresh = tempfile::tempdir().unwrap();
    let state = test_state(&fresh.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(fresh.path(), "synthetic.sqlite");
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let media = insert_video(&state);
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    let (task, _) = queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap(),
    )
    .unwrap();
    {
        let source = Connection::open(&path).unwrap();
        source
            .execute("UPDATE series SET title='changed' WHERE id='series-1'", [])
            .unwrap();
    }
    let changed = run_task(&state, &task).unwrap_err();
    assert!(changed.contains("来源"), "{changed}");
    assert_eq!(courses(&state), 0);
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let media = insert_video(&state);
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    let (task, _) = queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap(),
    )
    .unwrap();
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE media_entries SET deleted_at='gone' WHERE id=?1",
                [&media],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    let target = run_task(&state, &task).unwrap_err();
    assert!(target.contains("目标"), "{target}");
    assert_eq!(courses(&state), 0);
    assert_eq!(batches(&state), 0);
}

#[test]
fn synthetic_course_commit_requires_fresh_preflight_after_revoke_switch_restart_expiry_and_cancel()
{
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let media = insert_video(&state);
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    let preview_id = preview["previewId"].as_str().unwrap().to_string();
    let stray = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(ImportTaskRequest::CommitCourse {
                selection_id: ticket.selection_id.clone(),
                preview_id: preview_id.clone(),
            }),
        )
        .unwrap();
    let rejected = run_task(&state, &stray.0).unwrap_err();
    assert!(rejected.contains("预检"), "{rejected}");
    assert_eq!(courses(&state), 0);
    state.tasks.cancel(&stray.0.id).unwrap();
    let (queued, _) =
        queue_synthetic_course_commit(&state, &library, &ticket.selection_id, &preview_id).unwrap();
    assert_ne!(queued.id, stray.0.id);
    state.tasks.cancel(&queued.id).unwrap();
    assert!(run_task(&state, &queued).is_err());
    assert_eq!(courses(&state), 0);

    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    expire_synthetic_course_preflight(&state, &library, preview["previewId"].as_str().unwrap())
        .unwrap();
    assert!(queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap()
    )
    .is_err());

    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    let (task, _) = queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap(),
    )
    .unwrap();
    state.tasks.cancel(&task.id).unwrap();
    assert!(run_task(&state, &task).is_err());
    assert!(queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap()
    )
    .is_err());
    assert_eq!(courses(&state), 0);

    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    assert!(release_synthetic_course_source(&state, &library, &ticket.selection_id).unwrap());
    assert!(queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap()
    )
    .is_err());

    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    let replaced = select_synthetic_course_source(&state, &library, &path).unwrap();
    assert!(queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap()
    )
    .is_err());
    assert_ne!(replaced.selection_id, ticket.selection_id);

    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &replaced.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    assert!(queue_synthetic_course_commit(
        &state,
        &library,
        &replaced.selection_id,
        preview["previewId"].as_str().unwrap()
    )
    .is_err());

    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    let ticket = select_synthetic_course_source(&state, &library, &path).unwrap();
    let preview = approve_synthetic_course_preflight(
        &state,
        &library,
        &ticket.selection_id,
        vec![bind("course-1", &media)],
    )
    .unwrap();
    let (task, _) = queue_synthetic_course_commit(
        &state,
        &library,
        &ticket.selection_id,
        preview["previewId"].as_str().unwrap(),
    )
    .unwrap();
    forget_synthetic_course_process_memory(&state).unwrap();
    assert!(run_task(&state, &task).unwrap_err().contains("预检"));
    assert_eq!(courses(&state), 0);
    assert_eq!(fs::read(&path).unwrap(), FIXTURE);
    assert_no_sidecars(&path);
}

#[test]
fn marked_course_picker_wires_preflight_queue_and_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let before = fs::read(&path).unwrap();
    let selected = select_marked_course_with_picker(&state, &library, || Some(path.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(selected.file_name, "synthetic.sqlite");
    assert!(!selected.file_name.contains('/'));
    assert_eq!(selected.library_id, library);
    assert!(super::source_profiles::valid_uuid(&selected.selection_id));
    let cancelled = select_marked_course_with_picker(&state, &library, || None).unwrap();
    assert!(cancelled.is_none());
    assert!(current_marked_course_source(&state, &library)
        .unwrap()
        .is_none());

    let unmarked_dir = temp.path().join("unmarked");
    fs::create_dir(&unmarked_dir).unwrap();
    let unmarked = unmarked_dir.join("vault.sqlite");
    fs::write(&unmarked, FIXTURE).unwrap();
    let refused =
        select_marked_course_with_picker(&state, &library, || Some(unmarked.clone())).unwrap_err();
    assert!(refused.contains("合成标记"), "{refused}");
    assert!(current_marked_course_source(&state, &library)
        .unwrap()
        .is_none());
    assert_no_sidecars(&unmarked);
    let relative = select_marked_course_with_picker(&state, &library, || {
        Some(PathBuf::from("synthetic.sqlite"))
    })
    .unwrap_err();
    assert!(relative.contains("合成标记"), "{relative}");

    let binding = select_marked_course_with_picker(&state, &library, || Some(path.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(
        current_marked_course_source(&state, &library)
            .unwrap()
            .unwrap(),
        binding
    );
    let catalog = list_marked_course_series(&state, &library, &binding.selection_id).unwrap();
    assert_eq!(catalog[0]["id"], "series-1");
    assert_eq!(catalog[0]["title"], "合成系列");
    assert_eq!(catalog[0]["courseCount"], 2_i64);
    assert_eq!(courses(&state), 0);
    assert_no_sidecars(&path);

    let missing = approve_marked_course_preflight(
        &state,
        &library,
        &binding.selection_id,
        Vec::new(),
        Some(vec!["missing-series".into()]),
    )
    .unwrap_err();
    assert!(missing.contains("系列"), "{missing}");
    assert!(queue_synthetic_course_commit(
        &state,
        &library,
        &binding.selection_id,
        &uuid::Uuid::new_v4().to_string()
    )
    .is_err());

    let media = insert_video(&state);
    let first = approve_marked_course_preflight(
        &state,
        &library,
        &binding.selection_id,
        vec![bind("course-1", &media)],
        Some(vec!["series-1".into()]),
    )
    .unwrap();
    let bound = first["series"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "course-1")
        .unwrap();
    assert_eq!(bound["mediaId"], media);
    let listed_again = list_marked_course_series(&state, &library, &binding.selection_id).unwrap();
    assert_eq!(listed_again[0]["courseCount"], 2_i64);
    let rebound = approve_marked_course_preflight(
        &state,
        &library,
        &binding.selection_id,
        Vec::new(),
        Some(vec!["series-1".into()]),
    )
    .unwrap();
    assert_ne!(first["previewId"], rebound["previewId"]);
    let cleared = rebound["series"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "course-1")
        .unwrap();
    assert!(cleared["mediaId"].is_null());
    assert!(queue_synthetic_course_commit(
        &state,
        &library,
        &binding.selection_id,
        first["previewId"].as_str().unwrap()
    )
    .is_err());

    let preview = approve_marked_course_preflight(
        &state,
        &library,
        &binding.selection_id,
        vec![bind("course-1", &media)],
        Some(vec!["series-1".into()]),
    )
    .unwrap();
    let (task, launch) = queue_synthetic_course_commit(
        &state,
        &library,
        &binding.selection_id,
        preview["previewId"].as_str().unwrap(),
    )
    .unwrap();
    assert!(launch);
    match &task.payload {
        TaskRequest::LearningImport(ImportTaskRequest::CommitCourse {
            selection_id,
            preview_id,
        }) => {
            assert_eq!(selection_id, &binding.selection_id);
            assert_eq!(preview_id, preview["previewId"].as_str().unwrap());
        }
        _ => panic!("expected commitCourse"),
    }
    assert!(crate::tasks::ensure_generic_ipc_payload(&task.payload).is_err());
    let result = run_task(&state, &task).unwrap();
    assert_eq!(result["status"], "committed");
    assert_eq!(result["receipt"]["coursesCommitted"], 1);
    assert_eq!(result["receipt"]["courseConflicts"], 1);
    let series_total = result["receipt"]["seriesCommitted"].as_i64().unwrap()
        + result["receipt"]["seriesConflicts"].as_i64().unwrap();
    assert_eq!(series_total, 1);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_no_sidecars(&path);
    let scope = super::source_profiles::scope(&state, &library).unwrap();
    let report = super::course_apply::revert_course_batch(
        &state,
        &scope,
        &binding.epoch,
        result["batchId"].as_str().unwrap(),
    )
    .unwrap();
    assert!(!report["reverted"].as_array().unwrap().is_empty());
    assert_eq!(courses(&state), 0);
    assert!(release_synthetic_course_source(&state, &library, &binding.selection_id).unwrap());
    assert!(current_marked_course_source(&state, &library)
        .unwrap()
        .is_none());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn marked_course_picker_keeps_the_newer_selection_when_an_older_dialog_returns() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = write_fixture(temp.path(), "synthetic.sqlite");
    let newer = std::sync::Mutex::new(None);
    let error = select_marked_course_with_picker(&state, &library, || {
        let binding = select_marked_course_with_picker(&state, &library, || Some(path.clone()))
            .unwrap()
            .unwrap();
        *newer.lock().unwrap() = Some(binding);
        Some(path.clone())
    })
    .unwrap_err();
    assert!(error.contains("已变化"), "{error}");
    assert_eq!(
        current_marked_course_source(&state, &library)
            .unwrap()
            .unwrap(),
        newer.lock().unwrap().clone().unwrap()
    );
    assert_no_sidecars(&path);
}
