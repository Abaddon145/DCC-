use super::import_course_preview::*;
use rusqlite::{params, Connection};

fn source() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let db = Connection::open(path).unwrap();
    (dir, db)
}

fn target() -> (tempfile::TempDir, Connection, String) {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    let media = uuid::Uuid::new_v4().to_string();
    let file = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','video',?2,'now','now')", params![media,file]).unwrap();
    db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4','video.mp4','now')", params![file,media]).unwrap();
    db.execute("INSERT INTO learning_courses(id,media_entry_id,position_ms,duration_ms,rate,completed,progress_revision,created_at,updated_at) VALUES('target-course',?1,9000,60000,1,0,7,'now','now')", [&media]).unwrap();
    db.execute("INSERT INTO learning_series(id,name,revision,created_at,updated_at) VALUES('target-series','existing',3,'now','now')", []).unwrap();
    db.execute("INSERT INTO learning_series_courses(series_id,course_id,position) VALUES('target-series','target-course',0)", []).unwrap();
    (dir, db, media)
}

fn binding(id: &str, media: &str) -> CourseMediaBinding {
    CourseMediaBinding {
        source_item_id: id.into(),
        media_id: media.into(),
    }
}

#[test]
fn learning_import_course_preview_preserves_context_and_progress() {
    let (_source_dir, source) = source();
    let (_target_dir, target, media) = target();
    let result = preview_courses(
        &source,
        &target,
        &[binding("course-1", &media), binding("course-2", &media)],
    )
    .unwrap();
    assert_eq!(result.series[0].id, "series-1");
    assert_eq!(result.series[0].items[0].title, "同内容不同课次");
    assert_eq!(
        result.series[0]
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["course-1", "course-2"]
    );
    assert_eq!(
        result.series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        11000
    );
    assert!(result.series[0].items[1].source_progress.is_none());
    assert_eq!(
        result.series[0].items[0].target.as_ref().unwrap().revision,
        7
    );
    assert_eq!(
        result.series[0].items[0]
            .target
            .as_ref()
            .unwrap()
            .memberships,
        [TargetMembership {
            series_id: "target-series".into(),
            series_revision: 3,
            position: 0
        }]
    );
    assert_eq!(
        result.series[0].items[0]
            .target
            .as_ref()
            .unwrap()
            .progress
            .position_ms,
        9000
    );
    assert!(result
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::RepeatedSeriesMembership
            && c.source_item_ids == ["course-1", "course-2"]));
    assert!(result
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::DifferentTargetProgress));
    assert!(result
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::SharedMedia
            && c.source_item_ids == ["course-1", "course-2"]));
    assert_eq!(
        target
            .query_row("SELECT count(*) FROM learning_courses", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        target
            .query_row(
                "SELECT position_ms FROM learning_courses WHERE id='target-course'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        9000
    );
    assert_eq!(
        source
            .query_row(
                "SELECT position_ms FROM playback_progress WHERE item_id='course-1'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        11000
    );
    assert_eq!(
        source
            .query_row("SELECT count(*) FROM series_items", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn learning_import_course_preview_keeps_source_metadata_and_rechecks_it() {
    let (_source_dir, source) = source();
    let (_target_dir, target, media) = target();
    source
        .execute(
            "UPDATE series SET description='课程说明' WHERE id='series-1'",
            [],
        )
        .unwrap();
    source.execute("UPDATE playback_progress SET last_played_at=123,updated_at=456,last_sequence=789 WHERE item_id='course-1'", []).unwrap();
    source
        .execute(
            "UPDATE series_items SET duration_ms=NULL WHERE id='course-2'",
            [],
        )
        .unwrap();
    let bindings = [binding("course-1", &media)];
    let initial = preview_courses(&source, &target, &bindings).unwrap();
    assert_eq!(initial.series[0].description.as_deref(), Some("课程说明"));
    assert_eq!(initial.series[0].items[0].source_duration_ms, Some(60000));
    assert_eq!(initial.series[0].items[1].source_duration_ms, None);
    assert_eq!(
        initial.series[0].items[0].source_progress_metadata,
        Some(SourceProgressMetadata {
            last_played_at: Some(123),
            updated_at: 456,
            last_sequence: 789
        })
    );
    assert_eq!(initial.series[0].items[1].source_progress_metadata, None);
    source
        .execute(
            "UPDATE playback_progress SET last_sequence=790 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    assert!(recheck_preview(&source, &target, &bindings, &initial)
        .unwrap_err()
        .contains("来源"));
}

#[test]
fn learning_import_course_preview_reports_different_cross_series_progress_and_zero() {
    let (_source_dir, source) = source();
    let (_target_dir, target, media) = target();
    source.execute("INSERT INTO series(id,title,created_at,updated_at,sort_order) VALUES('series-2','第二系列',1,1,1)", []).unwrap();
    source
        .execute(
            "UPDATE series_items SET series_id='series-2' WHERE id='course-2'",
            [],
        )
        .unwrap();
    source.execute("INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('course-2',0,60000,0,1,1)", []).unwrap();
    let result = preview_courses(
        &source,
        &target,
        &[binding("course-1", &media), binding("course-2", &media)],
    )
    .unwrap();
    assert_eq!(
        result
            .series
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["series-1", "series-2"]
    );
    assert_eq!(
        result.series[1].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        0
    );
    assert!(result
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::DifferentSourceProgress
            && c.source_item_ids == ["course-1", "course-2"]));
    assert!(result
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::SharedMedia));
    assert!(!result
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::RepeatedSeriesMembership));
}

#[test]
fn learning_import_course_preview_unresolved_and_invalid_bindings() {
    let (_source_dir, source) = source();
    let (_target_dir, target, media) = target();
    let result = preview_courses(&source, &target, &[binding("course-1", "missing")]).unwrap();
    assert!(result.series[0].items[0].media_id.is_none());
    assert!(result.series[0].items[1].media_id.is_none());
    assert!(preview_courses(
        &source,
        &target,
        &[binding("course-1", &media), binding("course-1", &media)]
    )
    .is_err());
    assert!(preview_courses(&source, &target, &[binding("missing", &media)]).is_err());
    assert!(source.is_autocommit());
    assert!(target.is_autocommit());
    target
        .execute(
            "UPDATE media_entries SET deleted_at='now' WHERE id=?1",
            [&media],
        )
        .unwrap();
    assert!(
        preview_courses(&source, &target, &[binding("course-1", &media)])
            .unwrap()
            .series[0]
            .items[0]
            .media_id
            .is_none()
    );
    target
        .execute(
            "UPDATE media_entries SET deleted_at=NULL,kind='audio' WHERE id=?1",
            [&media],
        )
        .unwrap();
    assert!(
        preview_courses(&source, &target, &[binding("course-1", &media)])
            .unwrap()
            .series[0]
            .items[0]
            .media_id
            .is_none()
    );
}

#[test]
fn learning_import_course_preview_rejects_bad_snapshot() {
    let (_source_dir, source) = source();
    let (_target_dir, target, _) = target();
    assert!(preview_courses(&source, &target, &[]).is_ok());
    source
        .execute(
            "UPDATE playback_progress SET position_ms=-1 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    assert!(preview_courses(&source, &target, &[]).is_err());
    assert!(source.is_autocommit());
    source
        .execute(
            "UPDATE playback_progress SET position_ms=0, playback_rate=8 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    assert!(preview_courses(&source, &target, &[]).is_err());
    source
        .execute(
            "UPDATE playback_progress SET playback_rate=1 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    source.execute("PRAGMA user_version=7", []).unwrap();
    assert!(preview_courses(&source, &target, &[]).is_err());
    source.execute("PRAGMA user_version=8", []).unwrap();
    source.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    source
        .execute(
            "UPDATE series_items SET series_id=?1 WHERE id='course-1'",
            params!["missing"],
        )
        .unwrap();
    assert!(preview_courses(&source, &target, &[]).is_err());
}

#[test]
fn learning_import_course_preview_fingerprints_track_source_and_target_independently() {
    let (_source_dir, source) = source();
    let (_target_dir, target, media) = target();
    let bindings = [
        binding("course-1", &media),
        binding("course-2", "missing-a"),
    ];
    let first = preview_courses(&source, &target, &bindings).unwrap();
    let again = preview_courses(&source, &target, &bindings).unwrap();
    assert_eq!(first.source_fingerprint.len(), 64);
    assert_eq!(first.target_fingerprint.len(), 64);
    assert_eq!(first.source_fingerprint, again.source_fingerprint);
    assert_eq!(first.target_fingerprint, again.target_fingerprint);
    assert_eq!(
        first.series[0].items[1].requested_media_id.as_deref(),
        Some("missing-a")
    );

    target.execute("UPDATE learning_courses SET position_ms=12345,progress_revision=8 WHERE id='target-course'", []).unwrap();
    let target_changed = preview_courses(&source, &target, &bindings).unwrap();
    assert_eq!(first.source_fingerprint, target_changed.source_fingerprint);
    assert_ne!(first.target_fingerprint, target_changed.target_fingerprint);

    source
        .execute(
            "UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    let source_changed = preview_courses(&source, &target, &bindings).unwrap();
    assert_ne!(
        target_changed.source_fingerprint,
        source_changed.source_fingerprint
    );
    assert_eq!(
        target_changed.target_fingerprint,
        source_changed.target_fingerprint
    );

    target
        .execute(
            "UPDATE learning_series SET revision=4 WHERE id='target-series'",
            [],
        )
        .unwrap();
    let membership_changed = preview_courses(&source, &target, &bindings).unwrap();
    assert_ne!(
        source_changed.target_fingerprint,
        membership_changed.target_fingerprint
    );
    let unbound = preview_courses(&source, &target, &[binding("course-1", &media)]).unwrap();
    let different_missing = preview_courses(
        &source,
        &target,
        &[
            binding("course-1", &media),
            binding("course-2", "missing-b"),
        ],
    )
    .unwrap();
    assert_ne!(
        unbound.target_fingerprint,
        different_missing.target_fingerprint
    );
    assert_ne!(
        membership_changed.target_fingerprint,
        different_missing.target_fingerprint
    );
    target
        .execute(
            "UPDATE media_entries SET deleted_at='now' WHERE id=?1",
            [&media],
        )
        .unwrap();
    let unavailable = preview_courses(&source, &target, &bindings).unwrap();
    assert_ne!(
        membership_changed.target_fingerprint,
        unavailable.target_fingerprint
    );
}

#[test]
fn learning_import_course_preview_recheck_rejects_changed_fields_and_bindings() {
    let (_source_dir, source) = source();
    let (_target_dir, target, media) = target();
    let bindings = [binding("course-1", &media), binding("course-2", "missing")];
    let expected = preview_courses(&source, &target, &bindings).unwrap();
    let checked = recheck_preview(&source, &target, &bindings, &expected).unwrap();
    assert_eq!(checked.source_fingerprint, expected.source_fingerprint);
    assert_eq!(checked.target_fingerprint, expected.target_fingerprint);
    let changed_binding = [binding("course-1", &media), binding("course-2", "other")];
    assert!(
        recheck_preview(&source, &target, &changed_binding, &expected)
            .unwrap_err()
            .contains("目标")
    );

    target
        .execute(
            "UPDATE learning_series SET revision=4 WHERE id='target-series'",
            [],
        )
        .unwrap();
    assert!(recheck_preview(&source, &target, &bindings, &expected)
        .unwrap_err()
        .contains("目标"));
    let after_target_change = preview_courses(&source, &target, &bindings).unwrap();
    source
        .execute(
            "UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    assert!(
        recheck_preview(&source, &target, &bindings, &after_target_change)
            .unwrap_err()
            .contains("来源")
    );
    source.execute("PRAGMA user_version=7", []).unwrap();
    assert!(recheck_preview(&source, &target, &bindings, &after_target_change).is_err());
    assert!(source.is_autocommit());
    assert!(target.is_autocommit());
    assert_eq!(
        target
            .query_row(
                "SELECT position_ms FROM learning_courses WHERE id='target-course'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        9000
    );
}

#[test]
fn learning_import_course_preview_uses_one_source_read_view() {
    use rusqlite::functions::FunctionFlags;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    let (dir, source) = source();
    let mode: String = source
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    source.execute_batch("ALTER TABLE series RENAME TO series_data; CREATE VIEW series AS SELECT id, title || update_during_preview() AS title, description, sort_order FROM series_data;").unwrap();
    let path = dir.path().join("source.sqlite");
    let called = Arc::new(AtomicBool::new(false));
    let callback_called = called.clone();
    source
        .create_scalar_function(
            "update_during_preview",
            0,
            FunctionFlags::SQLITE_UTF8,
            move |_| {
                if !callback_called.swap(true, Ordering::SeqCst) {
                    let writer = Connection::open(&path)?;
                    writer.execute(
                        "UPDATE playback_progress SET position_ms=21000 WHERE item_id='course-1'",
                        [],
                    )?;
                }
                Ok("")
            },
        )
        .unwrap();
    let (_target_dir, target, media) = target();
    let result = preview_courses(&source, &target, &[binding("course-1", &media)]).unwrap();
    assert!(called.load(Ordering::SeqCst));
    assert_eq!(
        result.series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        11000
    );
    assert_eq!(
        source
            .query_row(
                "SELECT position_ms FROM playback_progress WHERE item_id='course-1'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        21000
    );
}

#[cfg(windows)]
#[test]
fn learning_import_course_preview_reads_wal_snapshot_without_changing_source() {
    use rusqlite::config::DbConfig;
    use std::{fs, time::Duration};

    let (source_dir, writer) = source();
    let source_path = source_dir.path().join("source.sqlite");
    let wal_path = source_dir.path().join("source.sqlite-wal");
    let shm_path = source_dir.path().join("source.sqlite-shm");
    let mode: String = writer
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    let autocheckpoint: i64 = writer
        .query_row("PRAGMA wal_autocheckpoint=0", [], |r| r.get(0))
        .unwrap();
    assert_eq!(autocheckpoint, 0);
    writer
        .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .unwrap();
    writer
        .execute(
            "UPDATE playback_progress SET position_ms=21000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();

    let before = (
        fs::read(&source_path).unwrap(),
        fs::read(&wal_path).unwrap(),
        fs::read(&shm_path).unwrap(),
    );
    let main_copy = source_dir.path().join("main-only.sqlite");
    fs::write(&main_copy, &before.0).unwrap();
    let main_only = Connection::open(main_copy).unwrap();
    assert_eq!(
        main_only
            .query_row(
                "SELECT position_ms FROM playback_progress WHERE item_id='course-1'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        11000
    );

    let snapshot = super::import_snapshot::snapshot_synthetic_via_sidecar_copy(
        &source_path,
        &source_dir.path().join("snapshot-copy.sqlite"),
        Duration::from_secs(2),
    )
    .unwrap();
    let (_target_dir, target, media) = target();
    let result = preview_courses(&snapshot, &target, &[binding("course-1", &media)]).unwrap();
    assert_eq!(
        result.series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        21000
    );
    assert_eq!(
        result.series[0].items[0]
            .target
            .as_ref()
            .unwrap()
            .progress
            .position_ms,
        9000
    );
    assert!(result
        .conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::DifferentTargetProgress));
    assert_eq!(
        (
            fs::read(&source_path).unwrap(),
            fs::read(&wal_path).unwrap(),
            fs::read(&shm_path).unwrap()
        ),
        before
    );

    writer
        .execute(
            "UPDATE playback_progress SET position_ms=25000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    let old_result = preview_courses(&snapshot, &target, &[binding("course-1", &media)]).unwrap();
    assert_eq!(
        old_result.series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        21000
    );
}
