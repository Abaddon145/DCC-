//! Synthetic course-batch revert. No Vault, user database, or source writes.
use super::course_apply::{cancel_course_batch, revert_course_batch};
use super::course_apply_tests::{commit_course_batch, stage_course_batch};
use super::course_batch::create_course_batch;
use super::import_course_preview::CourseMediaBinding;
use super::{import_ops, source_profiles};
use crate::state::test_state;
use rusqlite::{params, Connection};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

const NOTE_OP: &str = "课程迁入批次不能使用笔记操作";
const COURSE_OP: &str = "笔记迁入批次不能使用课程操作";

fn open_source(dir: &Path) -> Connection {
    let path = dir.join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    Connection::open(path).unwrap()
}

fn source_bytes(dir: &Path) -> Vec<u8> {
    std::fs::read(dir.join("source.sqlite")).unwrap()
}

fn bind(id: &str, media: &str) -> CourseMediaBinding {
    CourseMediaBinding {
        source_item_id: id.into(),
        media_id: media.into(),
    }
}

fn insert_media(db: &Connection, duration: Option<i64>) -> String {
    let media = uuid::Uuid::new_v4().to_string();
    let file = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','媒体原名',?2,'now','now')",
        params![media, file],
    )
    .unwrap();
    db.execute(
        "INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,duration_ms,created_at) VALUES(?1,?2,'main','video.mp4','video.mp4',?3,?4,'now')",
        params![file, media, format!("media-library/video/{media}/video.mp4"), duration],
    )
    .unwrap();
    media
}

fn fresh_source(dir: &Path) -> Connection {
    let source = open_source(dir);
    source
        .execute_batch(
            "DELETE FROM playback_progress; DELETE FROM series_items; DELETE FROM series;
             INSERT INTO series(id,title,description,sort_order,created_at,updated_at) VALUES('series-solo','独立系列','说明',1,1,1);
             INSERT INTO series_items(id,series_id,title,sort_order,duration_ms,created_at,updated_at) VALUES('solo-a','series-solo','课次甲',0,5000,1,1);
             INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('solo-a',22000,5000,0,1,1);",
        )
        .unwrap();
    source
}

fn course_batch(
    state: &crate::state::AppState,
    scope: &source_profiles::LibraryScope,
    profile: &source_profiles::SourceProfile,
    hash: &str,
) -> String {
    create_course_batch(state, scope, &profile.id, &profile.access_epoch, hash).unwrap()
}

fn commit_solo(
    state: &crate::state::AppState,
    scope: &source_profiles::LibraryScope,
    profile: &source_profiles::SourceProfile,
    source: &Connection,
    hash: &str,
) -> (String, String, String, String) {
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let batch = course_batch(state, scope, profile, hash);
    let bindings = [bind("solo-a", &media)];
    stage_course_batch(state, scope, &batch, source, &bindings).unwrap();
    commit_course_batch(state, scope, &batch, source, &bindings).unwrap();
    let course = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT id FROM learning_courses WHERE media_entry_id=?1",
                [&media],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    let series = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT id FROM learning_series WHERE name='独立系列'",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    (batch, media, course, series)
}

#[test]
fn learning_import_course_revert_rejects_malformed_saved_report() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = fresh_source(temp.path());
    let (batch, _, _, _) = commit_solo(&state, &scope, &profile, &source, &"a".repeat(64));
    revert_course_batch(&state, &scope, &profile.access_epoch, &batch).unwrap();
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_import_reverts SET report_json='{}' WHERE batch_id=?1",
                [&batch],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        revert_course_batch(&state, &scope, &profile.access_epoch, &batch).unwrap_err(),
        "迁入撤回记录损坏"
    );
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_import_reverts WHERE batch_id=?1 AND report_json='{}'",
            &batch
        ),
        1
    );
}

fn count(state: &crate::state::AppState, sql: &str, id: &str) -> i64 {
    state
        .with_library(|db, _| {
            db.query_row(sql, [id], |row| row.get(0))
                .map_err(|error| error.to_string())
        })
        .unwrap()
}

fn entry_id(state: &crate::state::AppState, batch: &str, kind: &str, source_id: &str) -> String {
    state
        .with_library(|db, _| {
            db.query_row(
                "SELECT id FROM learning_course_import_entries WHERE batch_id=?1 AND entry_kind=?2 AND source_id=?3",
                params![batch, kind, source_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap()
}

fn outcome(report: &Value, entry: &str, target: Option<&str>) -> String {
    if let Some(row) = report["kept"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["entryId"] == entry))
    {
        return row["outcome"].as_str().unwrap().to_string();
    }
    if let Some(target) = target {
        if report["reverted"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == target)
        {
            return "reverted".into();
        }
    }
    panic!("missing revert outcome for {entry}");
}

fn origin_rows(state: &crate::state::AppState, kind: &str, target: &str) -> i64 {
    state
        .with_library(|db, _| {
            db.query_row(
                "SELECT count(*) FROM learning_origin_map WHERE target_kind=?1 AND target_id=?2",
                params![kind, target],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap()
}

#[test]
fn learning_import_course_revert_removes_unchanged_created_course_and_series() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = fresh_source(temp.path());
    let (batch, media, course, series) =
        commit_solo(&state, &scope, &profile, &source, &"a".repeat(64));
    let before = source_bytes(temp.path());
    let report = revert_course_batch(&state, &scope, &profile.access_epoch, &batch).unwrap();
    let details = super::course_batch::list_course_entries(
        &state,
        &state.active_library_id().unwrap(),
        &batch,
        0,
    )
    .unwrap();
    assert_eq!(details.revert_report.as_ref(), Some(&report));
    assert!(!details.revert_damaged);
    let listed =
        super::course_batch::list_course_batches(&state, &state.active_library_id().unwrap())
            .unwrap();
    let summary = listed.iter().find(|item| item.id == batch).unwrap();
    assert!(summary.reverted);
    assert!(!summary.revert_damaged);
    assert_eq!(report["batchId"], batch);
    assert_eq!(report["archivesKept"], true);
    assert_eq!(report["kept"], serde_json::json!([]));
    let mut reverted = report["reverted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    reverted.sort();
    let mut expected = vec![course.clone(), series.clone()];
    expected.sort();
    assert_eq!(reverted, expected);
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_courses WHERE id=?1",
            &course
        ),
        0
    );
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_series WHERE id=?1",
            &series
        ),
        0
    );
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM media_entries WHERE id=?1 AND deleted_at IS NULL",
            &media
        ),
        1
    );
    assert_eq!(origin_rows(&state, "course", &course), 0);
    assert_eq!(origin_rows(&state, "series", &series), 0);
    assert_eq!(
        state
            .with_library(|db, _| {
                db.query_row("SELECT count(*) FROM learning_note_archives", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|error| error.to_string())
            })
            .unwrap(),
        0
    );
    assert_eq!(source_bytes(temp.path()), before);
    let again = revert_course_batch(&state, &scope, &profile.access_epoch, &batch).unwrap();
    assert_eq!(again, report);
    let reused = course_batch(&state, &scope, &profile, &"a".repeat(64));
    assert_ne!(reused, batch);
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_import_batches WHERE id=?1 AND status='pending'",
            &reused
        ),
        1
    );
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_import_batches WHERE id=?1 AND status='committed'",
            &batch
        ),
        1
    );
}

#[test]
fn learning_import_course_revert_runs_through_shared_task_with_epoch_check() {
    use super::import_runner::{self, ImportTaskRequest};
    use crate::tasks::TaskRequest;
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = fresh_source(temp.path());
    let (batch, _media, course, series) =
        commit_solo(&state, &scope, &profile, &source, &"a".repeat(64));
    let stale = ImportTaskRequest::RevertCourse {
        batch_id: batch.clone(),
        expected_epoch: uuid::Uuid::new_v4().to_string(),
    };
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::LearningImport(stale.clone()))
        .unwrap();
    assert!(import_runner::run_local(&state, &task, &stale).is_err());
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_courses WHERE id=?1",
            &course
        ),
        1
    );
    let request = ImportTaskRequest::RevertCourse {
        batch_id: batch.clone(),
        expected_epoch: profile.access_epoch.clone(),
    };
    let (task, _) = state
        .tasks
        .submit(library, TaskRequest::LearningImport(request.clone()))
        .unwrap();
    let value = import_runner::run_local(&state, &task, &request).unwrap();
    assert_eq!(value["status"], "reverted");
    assert_eq!(value["batchId"], batch);
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_courses WHERE id=?1",
            &course
        ),
        0
    );
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_series WHERE id=?1",
            &series
        ),
        0
    );
}

#[test]
fn learning_import_course_revert_keeps_edited_course_and_removes_unchanged_series() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = fresh_source(temp.path());
    let (batch, _media, course, series) =
        commit_solo(&state, &scope, &profile, &source, &"b".repeat(64));
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_courses SET title_override='用户改过', position_ms=4242, progress_revision=9 WHERE id=?1",
                [&course],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    let report = revert_course_batch(&state, &scope, &profile.access_epoch, &batch).unwrap();
    let course_entry = entry_id(&state, &batch, "course", "solo-a");
    let series_entry = entry_id(&state, &batch, "series", "series-solo");
    let details = super::course_batch::list_course_entries(
        &state,
        &state.active_library_id().unwrap(),
        &batch,
        0,
    )
    .unwrap();
    assert_eq!(details.revert_report.as_ref(), Some(&report));
    assert!(!details.revert_damaged);
    assert_eq!(
        outcome(&report, &course_entry, Some(&course)),
        "kept_modified"
    );
    assert_eq!(outcome(&report, &series_entry, Some(&series)), "reverted");
    let row: (String, i64, i64) = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT title_override, position_ms, progress_revision FROM learning_courses WHERE id=?1",
                [&course],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(row, ("用户改过".into(), 4242, 9));
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_series WHERE id=?1",
            &series
        ),
        0
    );
    assert_eq!(origin_rows(&state, "course", &course), 1);
    assert_eq!(origin_rows(&state, "series", &series), 0);
}

#[test]
fn learning_import_course_revert_keeps_renamed_series_and_its_course() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = fresh_source(temp.path());
    let (batch, _media, course, series) =
        commit_solo(&state, &scope, &profile, &source, &"c".repeat(64));
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_series SET name='用户系列', revision=2 WHERE id=?1",
                [&series],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    let report = revert_course_batch(&state, &scope, &profile.access_epoch, &batch).unwrap();
    assert_eq!(
        outcome(
            &report,
            &entry_id(&state, &batch, "series", "series-solo"),
            Some(&series)
        ),
        "kept_modified"
    );
    assert_eq!(
        outcome(
            &report,
            &entry_id(&state, &batch, "course", "solo-a"),
            Some(&course)
        ),
        "kept_modified"
    );
    let member: String = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT course_id FROM learning_series_courses WHERE series_id=?1",
                [&series],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(member, course);
    assert_eq!(origin_rows(&state, "series", &series), 1);
    assert_eq!(origin_rows(&state, "course", &course), 1);
    let name: String = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT name FROM learning_series WHERE id=?1",
                [&series],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(name, "用户系列");
}

#[test]
fn learning_import_course_revert_keeps_reused_course_from_later_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = fresh_source(temp.path());
    let (first, media, course, series) =
        commit_solo(&state, &scope, &profile, &source, &"d".repeat(64));
    let second = course_batch(&state, &scope, &profile, &"e".repeat(64));
    let bindings = [bind("solo-a", &media)];
    stage_course_batch(&state, &scope, &second, &source, &bindings).unwrap();
    commit_course_batch(&state, &scope, &second, &source, &bindings).unwrap();
    let later = revert_course_batch(&state, &scope, &profile.access_epoch, &second).unwrap();
    assert_eq!(
        outcome(
            &later,
            &entry_id(&state, &second, "course", "solo-a"),
            Some(&course)
        ),
        "kept_reused"
    );
    let earlier = revert_course_batch(&state, &scope, &profile.access_epoch, &first).unwrap();
    assert_eq!(
        outcome(
            &earlier,
            &entry_id(&state, &first, "course", "solo-a"),
            Some(&course)
        ),
        "kept_reused"
    );
    assert_eq!(
        outcome(
            &earlier,
            &entry_id(&state, &first, "series", "series-solo"),
            Some(&series)
        ),
        "kept_reused"
    );
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_courses WHERE id=?1",
            &course
        ),
        1
    );
    assert_eq!(origin_rows(&state, "course", &course), 1);
    assert_eq!(origin_rows(&state, "series", &series), 1);
}

#[test]
fn learning_import_course_revert_keeps_mapped_conflict_and_missing_targets() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    {
        let source = open_source(temp.path());
        source
        .execute_batch(
            "DELETE FROM playback_progress; DELETE FROM series_items; DELETE FROM series;
             INSERT INTO series(id,title,sort_order,created_at,updated_at) VALUES('series-mix','混合系列',1,1,1);
             INSERT INTO series_items(id,series_id,title,sort_order,duration_ms,created_at,updated_at) VALUES('map-item','series-mix','已有课',0,5000,1,1);
             INSERT INTO series_items(id,series_id,title,sort_order,duration_ms,created_at,updated_at) VALUES('conflict-item','series-mix','冲突课',1,5000,1,1);
             INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('map-item',22000,5000,0,1,1);
             INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('conflict-item',1000,5000,0,1,1);",
        )
        .unwrap();
        let (mapped_media, conflict_media, mapped_course, conflict_course) = state
        .with_library(|db, _| {
            let mapped_media = insert_media(db, Some(5000));
            let conflict_media = insert_media(db, Some(5000));
            let mapped_course = uuid::Uuid::new_v4().to_string();
            let conflict_course = uuid::Uuid::new_v4().to_string();
            db.execute(
                "INSERT INTO learning_courses(id,media_entry_id,title_override,position_ms,duration_ms,rate,completed,progress_revision,created_at,updated_at) VALUES(?1,?2,'用户标题',22000,5000,1,0,4,'now','now')",
                params![mapped_course, mapped_media],
            )
            .map_err(|error| error.to_string())?;
            db.execute(
                "INSERT INTO learning_courses(id,media_entry_id,title_override,position_ms,duration_ms,rate,completed,progress_revision,created_at,updated_at) VALUES(?1,?2,'保持进度',9000,5000,1,0,3,'now','now')",
                params![conflict_course, conflict_media],
            )
            .map_err(|error| error.to_string())?;
            Ok((mapped_media, conflict_media, mapped_course, conflict_course))
        })
        .unwrap();
        let bindings = [
            bind("map-item", &mapped_media),
            bind("conflict-item", &conflict_media),
        ];
        let batch = course_batch(&state, &scope, &profile, &"f".repeat(64));
        stage_course_batch(&state, &scope, &batch, &source, &bindings).unwrap();
        commit_course_batch(&state, &scope, &batch, &source, &bindings).unwrap();
        let report = revert_course_batch(&state, &scope, &profile.access_epoch, &batch).unwrap();
        assert_eq!(
            outcome(
                &report,
                &entry_id(&state, &batch, "course", "map-item"),
                Some(&mapped_course)
            ),
            "kept_reused"
        );
        assert_eq!(
            outcome(
                &report,
                &entry_id(&state, &batch, "course", "conflict-item"),
                Some(&conflict_course)
            ),
            "kept_conflict"
        );
        assert_eq!(
            outcome(
                &report,
                &entry_id(&state, &batch, "series", "series-mix"),
                None
            ),
            "kept_conflict"
        );
        let mapped: (String, i64, i64) = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT title_override, position_ms, progress_revision FROM learning_courses WHERE id=?1",
                [&mapped_course],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
        assert_eq!(mapped, ("用户标题".into(), 22000, 4));
        let conflict: (i64, i64) = state
            .with_library(|db, _| {
                db.query_row(
                    "SELECT position_ms, progress_revision FROM learning_courses WHERE id=?1",
                    [&conflict_course],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(conflict, (9000, 3));
        assert_eq!(origin_rows(&state, "course", &mapped_course), 1);
        assert_eq!(origin_rows(&state, "course", &conflict_course), 0);
        assert_eq!(
            count(
                &state,
                "SELECT count(*) FROM learning_series WHERE name=?1",
                "混合系列"
            ),
            0
        );
    }

    let source = fresh_source(temp.path());
    let (created, _media, course, series) =
        commit_solo(&state, &scope, &profile, &source, &"1".repeat(64));
    state
        .with_library(|db, _| {
            db.execute("DELETE FROM learning_courses WHERE id=?1", [&course])
                .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    let missing = revert_course_batch(&state, &scope, &profile.access_epoch, &created).unwrap();
    assert_eq!(
        outcome(
            &missing,
            &entry_id(&state, &created, "course", "solo-a"),
            Some(&course)
        ),
        "kept_missing"
    );
    assert_eq!(
        outcome(
            &missing,
            &entry_id(&state, &created, "series", "series-solo"),
            Some(&series)
        ),
        "kept_modified"
    );
    assert_eq!(origin_rows(&state, "course", &course), 1);
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_series WHERE id=?1",
            &series
        ),
        1
    );
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_courses WHERE id=?1",
            &course
        ),
        0
    );
}

#[test]
fn learning_import_course_revert_keeps_course_with_user_links() {
    for usage in ["note", "excerpt"] {
        let temp = tempfile::tempdir().unwrap();
        let state = test_state(&temp.path().join("library"));
        let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
        let profile = source_profiles::create(&state, &scope).unwrap();
        let source = fresh_source(temp.path());
        let (batch, media, course, series) =
            commit_solo(&state, &scope, &profile, &source, &"2".repeat(64));
        state
            .with_library(|db, _| {
                match usage {
                    "note" => {
                        let note = uuid::Uuid::new_v4().to_string();
                        db.execute(
                            "INSERT INTO knowledge_notes(id,title,document_json,plain_text,tags_json,favorite,revision,created_at,updated_at) VALUES(?1,'用户笔记','{}','','[]',0,1,'now','now')",
                            [&note],
                        )
                        .map_err(|error| error.to_string())?;
                        db.execute(
                            "INSERT INTO learning_course_notes(course_id,note_id) VALUES(?1,?2)",
                            params![course, note],
                        )
                        .map_err(|error| error.to_string())?;
                    }
                    "excerpt" => {
                        let file: String = db
                            .query_row(
                                "SELECT primary_file_id FROM media_entries WHERE id=?1",
                                [&media],
                                |row| row.get(0),
                            )
                            .map_err(|error| error.to_string())?;
                        db.execute(
                            "INSERT INTO learning_excerpts(id,library_id,root_hash,course_id,media_id,source_file_id,payload_json,payload_sha256,status,created_at) VALUES(?1,?2,?3,?4,?5,?6,'{}',?7,'ready','now')",
                            params![
                                uuid::Uuid::new_v4().to_string(),
                                scope.library_id(),
                                scope.root_hash(),
                                course,
                                media,
                                file,
                                "ab".repeat(32)
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            })
            .unwrap();
        let report = revert_course_batch(&state, &scope, &profile.access_epoch, &batch).unwrap();
        assert_eq!(
            outcome(
                &report,
                &entry_id(&state, &batch, "course", "solo-a"),
                Some(&course)
            ),
            "kept_modified",
            "{usage}"
        );
        assert_eq!(
            count(
                &state,
                "SELECT count(*) FROM learning_courses WHERE id=?1",
                &course
            ),
            1,
            "{usage}"
        );
        assert_eq!(
            count(
                &state,
                "SELECT count(*) FROM learning_series WHERE id=?1",
                &series
            ),
            0,
            "{usage}"
        );
        if usage == "note" {
            assert_eq!(
                count(
                    &state,
                    "SELECT count(*) FROM learning_course_notes WHERE course_id=?1",
                    &course
                ),
                1
            );
        } else {
            assert_eq!(
                count(
                    &state,
                    "SELECT count(*) FROM learning_excerpts WHERE course_id=?1",
                    &course
                ),
                1
            );
        }
    }
}

#[test]
fn learning_import_course_revert_rejects_note_and_unfinished_batches() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let pending = course_batch(&state, &scope, &profile, &"3".repeat(64));
    assert_eq!(
        revert_course_batch(&state, &scope, &profile.access_epoch, &pending).unwrap_err(),
        "只能撤回已提交的迁入批次"
    );
    let source = fresh_source(temp.path());
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let staged = course_batch(&state, &scope, &profile, &"4".repeat(64));
    stage_course_batch(&state, &scope, &staged, &source, &[bind("solo-a", &media)]).unwrap();
    assert_eq!(
        cancel_course_batch(&state, &scope, &staged).unwrap().status,
        "cancelled"
    );
    assert_eq!(
        revert_course_batch(&state, &scope, &profile.access_epoch, &staged).unwrap_err(),
        "只能撤回已提交的迁入批次"
    );
    let (batch, _media, _course, _series) =
        commit_solo(&state, &scope, &profile, &source, &"5".repeat(64));
    let stale =
        revert_course_batch(&state, &scope, &uuid::Uuid::new_v4().to_string(), &batch).unwrap_err();
    assert!(stale.contains("来源状态"), "{stale}");
    assert_eq!(
        revert_course_batch(&state, &scope, "not-an-epoch", &batch).unwrap_err(),
        "来源状态无效，请重新读取批次"
    );
    assert_eq!(
        import_ops::revert_batch(&state, &scope, &profile.access_epoch, &batch).unwrap_err(),
        NOTE_OP
    );

    let bytes = b"# still a note\n";
    let note_batch = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &format!("{:x}", Sha256::digest(b"note-request")),
    )
    .unwrap();
    let item = import_ops::add_item(
        &state,
        &scope,
        &note_batch.id,
        import_ops::NewImportItem {
            vault: "vault-revert".into(),
            relative_path: "keep.md".into(),
            source_sha256: format!("{:x}", Sha256::digest(bytes)),
            title: "笔记".into(),
            document: serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"合成"}]}]}),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    import_ops::stage_item_bytes(&state, &scope, &note_batch.id, &item.id, bytes).unwrap();
    import_ops::commit_batch(&state, &scope, &note_batch.id).unwrap();
    let note_id = import_ops::get_item(&state, &scope, &item.id)
        .unwrap()
        .target_id
        .unwrap();
    assert_eq!(
        revert_course_batch(&state, &scope, &profile.access_epoch, &note_batch.id).unwrap_err(),
        COURSE_OP
    );
    assert!(state
        .with_library(|db, _| Ok(crate::knowledge::get(db, &note_id)?.deleted_at))
        .unwrap()
        .is_none());
    let note_report =
        import_ops::revert_batch(&state, &scope, &profile.access_epoch, &note_batch.id).unwrap();
    assert_eq!(
        note_report["reverted"],
        serde_json::json!([note_id.clone()])
    );
    assert!(state
        .with_library(|db, _| Ok(crate::knowledge::get(db, &note_id)?.deleted_at))
        .unwrap()
        .is_some());
    assert_eq!(
        count(
            &state,
            "SELECT count(*) FROM learning_courses WHERE media_entry_id=?1",
            &_media
        ),
        1
    );
}
