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
fn learning_import_course_preview_limits_to_explicit_complete_series() {
    let (_dir, source) = source();
    let (_target_dir, target, _) = target();
    source.execute("INSERT INTO series(id,title,created_at,updated_at,sort_order) VALUES('series-2','第二系列',1,1,1)",[]).unwrap();
    source
        .execute(
            "UPDATE series_items SET series_id='series-2' WHERE id='course-2'",
            [],
        )
        .unwrap();
    let profile = uuid::Uuid::new_v4().to_string();
    let epoch = uuid::Uuid::new_v4().to_string();
    target.execute("INSERT INTO learning_source_profiles(id,application,access_epoch,created_at) VALUES(?1,'yingliu',?2,'2026-01-01T00:00:00Z')",params![profile,epoch]).unwrap();
    let selected = preview_courses_from_profile_selected(
        &source,
        &target,
        &[],
        &profile,
        &["series-1".into()],
    )
    .unwrap();
    assert_eq!(selected.series.len(), 1);
    assert_eq!(selected.series[0].id, "series-1");
    assert_eq!(selected.series[0].items.len(), 1);
    source
        .execute(
            "UPDATE series SET description=X'80' WHERE id='series-2'",
            [],
        )
        .unwrap();
    assert!(preview_courses_from_profile_selected(
        &source,
        &target,
        &[],
        &profile,
        &["series-1".into()]
    )
    .is_ok());
    source
        .execute("UPDATE series SET description=NULL WHERE id='series-2'", [])
        .unwrap();
    assert!(preview_courses_from_profile_selected(
        &source,
        &target,
        &[],
        &profile,
        &["missing".into()]
    )
    .is_err());
    assert!(preview_courses_from_profile_selected(&source, &target, &[], &profile, &[]).is_err());
    let tx = source.unchecked_transaction().unwrap();
    for n in 0..10001 {
        tx.execute("INSERT INTO series_items(id,series_id,title,sort_order,created_at,updated_at) VALUES(?1,'series-1','extra',?2,1,1)",params![format!("large-{n}"),n+2]).unwrap();
    }
    tx.commit().unwrap();
    let small = preview_courses_from_profile_selected(
        &source,
        &target,
        &[],
        &profile,
        &["series-2".into()],
    )
    .unwrap();
    assert_eq!(small.series.len(), 1);
    assert_eq!(small.series[0].id, "series-2");
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
fn learning_import_course_preview_rejects_views_and_generated_required_columns() {
    let (_source_dir, source_db) = source();
    let (_target_dir, target, _) = target();
    source_db.execute_batch("ALTER TABLE series RENAME TO old_series; CREATE VIEW series AS SELECT * FROM old_series;").unwrap();
    assert!(preview_courses(&source_db, &target, &[]).is_err());

    let (_other_dir, generated) = source();
    generated.execute_batch("ALTER TABLE series RENAME COLUMN title TO original_title; ALTER TABLE series ADD COLUMN title TEXT GENERATED ALWAYS AS (original_title) VIRTUAL;").unwrap();
    assert!(preview_courses(&generated, &target, &[]).is_err());

    let (_virtual_dir, virtual_source) = source();
    virtual_source.execute_batch("ALTER TABLE series RENAME TO old_series; CREATE VIRTUAL TABLE series USING fts5(id,title,description,sort_order); INSERT INTO series SELECT id,title,description,sort_order FROM old_series;").unwrap();
    assert!(preview_courses(&virtual_source, &target, &[]).is_err());

    let (_extra_dir, extra) = source();
    extra
        .execute_batch("ALTER TABLE series ADD COLUMN extra TEXT;")
        .unwrap();
    assert!(preview_courses(&extra, &target, &[]).is_ok());
}

#[test]
fn learning_import_course_preview_uses_main_tables_when_temp_names_shadow_them() {
    let (_source_dir, source) = source();
    let (_target_dir, target, _) = target();
    source.execute_batch("CREATE TEMP TABLE series(id TEXT,title TEXT,description TEXT,sort_order INTEGER); INSERT INTO series VALUES('wrong','wrong',NULL,0);").unwrap();
    let preview = preview_courses(&source, &target, &[]).unwrap();
    assert_eq!(preview.series[0].id, "series-1");
}

#[test]
fn learning_import_course_preview_bounds_source_without_truncating() {
    let (_source_dir, source) = source();
    let (_target_dir, target, _) = target();
    let target_changes = target.total_changes();
    let large_but_allowed = "中".repeat(350_000);
    source
        .execute(
            "UPDATE series SET description=?1 WHERE id='series-1'",
            [&large_but_allowed],
        )
        .unwrap();
    let preview = preview_courses(&source, &target, &[]).unwrap();
    assert!(preview.series[0].description.as_deref() == Some(large_but_allowed.as_str()));
    let oversized = "中".repeat(750_000);
    source
        .execute(
            "UPDATE series SET description=?1 WHERE id='series-1'",
            [&oversized],
        )
        .unwrap();
    let Err(error) = preview_courses(&source, &target, &[]) else {
        panic!("oversized source was accepted")
    };
    assert_eq!(error, "课程来源过大，当前版本无法一次预检");
    assert!(source.is_autocommit());
    assert_eq!(target.total_changes(), target_changes);
    assert_eq!(
        source
            .query_row(
                "SELECT description FROM series WHERE id='series-1'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        oversized
    );
    source
        .execute(
            "UPDATE series SET description='safe' WHERE id='series-1'",
            [],
        )
        .unwrap();
    source.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<11000) INSERT INTO series(id,title,sort_order,created_at,updated_at) SELECT 'extra-'||x,'x',x,1,1 FROM n;").unwrap();
    let Err(error) = preview_courses(&source, &target, &[]) else {
        panic!("oversized row count was accepted")
    };
    assert_eq!(error, "课程来源过大，当前版本无法一次预检");
    assert!(source.is_autocommit());
}

#[test]
fn learning_import_course_preview_rejects_duplicate_series_and_progress_identities() {
    let (_series_dir, duplicate_series) = source();
    let (_target_dir, target, _) = target();
    drop_source_triggers(&duplicate_series);
    duplicate_series.execute_batch("CREATE TABLE series_copy AS SELECT * FROM series; DROP TABLE series; ALTER TABLE series_copy RENAME TO series; INSERT INTO series(id,title,sort_order,created_at,updated_at) VALUES('empty','first',10,1,1),('empty','second',11,1,1);").unwrap();
    let series_result = preview_courses(&duplicate_series, &target, &[]);

    let (_progress_dir, duplicate_progress) = source();
    drop_source_triggers(&duplicate_progress);
    duplicate_progress.execute_batch("CREATE TABLE progress_copy AS SELECT * FROM playback_progress; DROP TABLE playback_progress; ALTER TABLE progress_copy RENAME TO playback_progress; INSERT INTO playback_progress SELECT * FROM playback_progress WHERE item_id='course-1';").unwrap();
    assert!(preview_courses(&duplicate_progress, &target, &[]).is_err());
    assert!(duplicate_progress.is_autocommit());
    assert!(series_result.is_err());
    assert!(duplicate_series.is_autocommit());

    let (_items_dir, duplicate_items) = source();
    drop_source_triggers(&duplicate_items);
    duplicate_items.execute_batch("CREATE TABLE item_copy AS SELECT * FROM series_items; DROP TABLE series_items; ALTER TABLE item_copy RENAME TO series_items; INSERT INTO series_items SELECT * FROM series_items WHERE id='course-1';").unwrap();
    assert!(preview_courses(&duplicate_items, &target, &[]).is_err());
    assert!(duplicate_items.is_autocommit());
}

#[test]
fn learning_import_course_preview_uses_binary_progress_identity() {
    let (_source_dir, source) = source();
    let (_target_dir, target, _) = target();
    drop_source_triggers(&source);
    source.execute_batch("CREATE TABLE progress_copy(item_id TEXT COLLATE NOCASE,position_ms INTEGER,duration_ms INTEGER,playback_rate REAL,completed INTEGER,last_played_at INTEGER,updated_at INTEGER,last_sequence INTEGER); INSERT INTO progress_copy SELECT item_id,position_ms,duration_ms,playback_rate,completed,last_played_at,updated_at,last_sequence FROM playback_progress; DROP TABLE playback_progress; ALTER TABLE progress_copy RENAME TO playback_progress; INSERT INTO series_items(id,series_id,title,sort_order,created_at,updated_at) VALUES('CASE-A','series-1','upper',3,1,1),('case-a','series-1','lower',4,1,1); INSERT INTO playback_progress(item_id,position_ms,playback_rate,completed,updated_at,last_sequence) VALUES('CASE-A',1234,1,0,1,1);").unwrap();
    let preview = preview_courses(&source, &target, &[]).unwrap();
    let items = &preview.series[0].items;
    assert_eq!(
        items
            .iter()
            .find(|item| item.id == "CASE-A")
            .unwrap()
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        1234
    );
    assert!(items
        .iter()
        .find(|item| item.id == "case-a")
        .unwrap()
        .source_progress
        .is_none());
}

#[test]
fn learning_import_course_preview_uses_binary_series_membership() {
    let (_source_dir, source) = source();
    let (_target_dir, target, _) = target();
    drop_source_triggers(&source);
    source.execute_batch("CREATE TABLE item_copy(id TEXT,series_id TEXT COLLATE NOCASE,title TEXT,duration_ms INTEGER,sort_order INTEGER); INSERT INTO item_copy SELECT id,series_id,title,duration_ms,sort_order FROM series_items; DROP TABLE series_items; ALTER TABLE item_copy RENAME TO series_items; INSERT INTO series(id,title,sort_order,created_at,updated_at) VALUES('CASE-S','upper',10,1,1),('case-s','lower',11,1,1); INSERT INTO series_items(id,series_id,title,sort_order) VALUES('upper-item','CASE-S','upper',1),('lower-item','case-s','lower',1);").unwrap();
    let preview = preview_courses(&source, &target, &[]).unwrap();
    let upper = preview
        .series
        .iter()
        .find(|series| series.id == "CASE-S")
        .unwrap();
    let lower = preview
        .series
        .iter()
        .find(|series| series.id == "case-s")
        .unwrap();
    assert_eq!(
        upper
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["upper-item"]
    );
    assert_eq!(
        lower
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["lower-item"]
    );
}

fn drop_source_triggers(source: &Connection) {
    source.execute_batch("PRAGMA foreign_keys=OFF; DROP TRIGGER IF EXISTS learning_item_insert; DROP TRIGGER IF EXISTS learning_item_delete; DROP TRIGGER IF EXISTS learning_item_move; DROP TRIGGER IF EXISTS learning_progress_insert; DROP TRIGGER IF EXISTS learning_progress_update; DROP TRIGGER IF EXISTS learning_progress_delete;").unwrap();
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
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    let (dir, mut source) = source();
    let mode: String = source
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    let path = dir.path().join("source.sqlite");
    let called = Arc::new(AtomicBool::new(false));
    let callback_called = called.clone();
    source
        .progress_handler(
            120,
            Some(move || {
                if !callback_called.swap(true, Ordering::SeqCst) {
                    let writer = Connection::open(&path).unwrap();
                    writer
                    .execute(
                        "UPDATE playback_progress SET position_ms=21000 WHERE item_id='course-1'",
                        [],
                    )
                    .unwrap();
                }
                false
            }),
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
    let pinned = super::import_snapshot::snapshot_pinned_synthetic(
        &source_path,
        Duration::from_secs(2),
        || {},
    )
    .unwrap();
    let pinned_result = preview_courses(&pinned, &target, &[binding("course-1", &media)]).unwrap();
    assert_eq!(pinned_result.source_fingerprint, result.source_fingerprint);
    assert_eq!(pinned_result.target_fingerprint, result.target_fingerprint);
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
    let fresh = super::import_snapshot::snapshot_pinned_synthetic(
        &source_path,
        Duration::from_secs(2),
        || {},
    )
    .unwrap();
    assert!(recheck_preview(
        &fresh,
        &target,
        &[binding("course-1", &media)],
        &pinned_result
    )
    .is_err());
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

#[cfg(windows)]
#[test]
fn learning_import_course_preview_requires_current_sqlite_selection() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_profiles, source_sqlite};
    use std::time::Duration;
    let (source_dir, writer) = source();
    let path = source_dir.path().join("source.sqlite");
    let mode: String = writer
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    writer
        .execute(
            "UPDATE playback_progress SET position_ms=22000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    let state = crate::state::test_state(&target_dir.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let binding = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
    let profile = binding.profile_id().to_string();
    let epoch = binding.epoch().to_string();
    binding.check(&state).unwrap();
    let choices = SyntheticSqliteSelections::default();
    let ticket = choices
        .select(&state, &library, &profile, &epoch, || Some(path))
        .unwrap()
        .unwrap();
    let preview = choices
        .inspect(&state, &ticket, Duration::from_secs(2), |snapshot| {
            source_profiles::scoped(&state, &scope, |db| {
                preview_courses_from_profile(snapshot, db, &[], &profile)
            })
        })
        .unwrap();
    assert_eq!(
        preview.series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        22000
    );
    assert!(choices
        .inspect(&state, &ticket, Duration::from_secs(2), |snapshot| {
            let preview = source_profiles::scoped(&state, &scope, |db| {
                preview_courses_from_profile(snapshot, db, &[], &profile)
            })?;
            choices.select(&state, &library, &profile, &epoch, || None)?;
            Ok(preview)
        })
        .is_err());
    assert!(choices
        .snapshot(&state, &ticket, Duration::from_secs(2), || {})
        .is_err());
}
