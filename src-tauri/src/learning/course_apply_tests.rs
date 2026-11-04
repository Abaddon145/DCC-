use super::course_apply::cancel_course_batch;
use super::course_batch::create_course_batch;
use super::import_course_preview::CourseMediaBinding;
use super::{import_ops, source_profiles};
use crate::state::test_state;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

const NOTE_OP: &str = "课程迁入批次不能使用笔记操作";
const COURSE_OP: &str = "笔记迁入批次不能使用课程操作";

#[cfg(windows)]
fn close_source_without_checkpoint(source: Connection) {
    source
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    drop(source);
}

#[cfg(windows)]
#[test]
fn learning_import_course_source_selection_cancellation_cannot_write_batch() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_sqlite};
    use std::time::Duration;
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let source = open_source(temp.path());
    let path = temp.path().join("source.sqlite");
    let mode: String = source
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    source
        .execute(
            "UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    close_source_without_checkpoint(source);
    let bound = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    let choices = SyntheticSqliteSelections::default();
    let choose = || {
        choices
            .select(&state, &library, &profile.id, &profile.access_epoch, || {
                Some(path.clone())
            })
            .unwrap()
            .unwrap()
    };
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let batch = course_batch(&state, &scope, &profile, &"a".repeat(64));
    let ticket = choose();
    let approved = choices
        .course_preview(&state, &ticket, &scope, &bindings, Duration::from_secs(2))
        .unwrap();
    assert!(choices
        .course_write(
            &state,
            &approved,
            Duration::from_secs(2),
            || {
                choices
                    .select(&state, &library, &profile.id, &profile.access_epoch, || {
                        None
                    })
                    .unwrap();
            },
            |snapshot, preview| super::course_apply::stage_course_batch(
                &state, &scope, &batch, snapshot, &bindings, preview
            )
        )
        .is_err());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        0
    );
    let ticket = choose();
    let approved = choices
        .course_preview(&state, &ticket, &scope, &bindings, Duration::from_secs(2))
        .unwrap();
    choices
        .course_write(
            &state,
            &approved,
            Duration::from_secs(2),
            || {},
            |snapshot, preview| {
                assert!(Connection::open(&path).and_then(|db| { db.busy_timeout(Duration::ZERO)?; db.execute("UPDATE playback_progress SET position_ms=17000 WHERE item_id='course-1'", []) }).is_err());
                super::course_apply::stage_course_batch(
                    &state, &scope, &batch, snapshot, &bindings, preview,
                )
            },
        )
        .unwrap();
    assert!(choices
        .course_write(
            &state,
            &approved,
            Duration::from_secs(2),
            || {
                choices
                    .select(&state, &library, &profile.id, &profile.access_epoch, || {
                        None
                    })
                    .unwrap();
            },
            |snapshot, preview| super::course_apply::commit_course_batch(
                &state, &scope, &batch, snapshot, &bindings, preview
            )
        )
        .is_err());
    assert!(course_row(&state, &media).is_none());
    let ticket = choose();
    let approved = choices
        .course_preview(&state, &ticket, &scope, &bindings, Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        choices
            .course_write(
                &state,
                &approved,
                Duration::from_secs(2),
                || {},
                |snapshot, preview| super::course_apply::commit_course_batch(
                    &state, &scope, &batch, snapshot, &bindings, preview
                )
            )
            .unwrap()
            .status,
        "committed"
    );
}

#[cfg(windows)]
#[test]
fn learning_import_course_selection_revoke_waits_for_in_progress_stage() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_sqlite};
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let source = open_source(temp.path());
    let path = temp.path().join("source.sqlite");
    let mode: String = source
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    source
        .execute(
            "UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    close_source_without_checkpoint(source);
    let bound = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    let choices = Arc::new(SyntheticSqliteSelections::default());
    let ticket = choices
        .select(&state, &library, &profile.id, &profile.access_epoch, || {
            Some(path)
        })
        .unwrap()
        .unwrap();
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let batch = course_batch(&state, &scope, &profile, &"b".repeat(64));
    let approved = choices
        .course_preview(&state, &ticket, &scope, &bindings, Duration::from_secs(2))
        .unwrap();
    let (start_tx, start_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let other = Arc::clone(&choices);
    let worker = std::thread::spawn(move || {
        start_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        ready_tx.send(()).unwrap();
        other.revoke().unwrap();
        done_tx.send(()).unwrap();
    });
    choices
        .course_write(
            &state,
            &approved,
            Duration::from_secs(2),
            || {},
            |snapshot, preview| {
                start_tx.send(()).unwrap();
                ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
                super::course_apply::stage_course_batch(
                    &state, &scope, &batch, snapshot, &bindings, preview,
                )
            },
        )
        .unwrap();
    done_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    worker.join().unwrap();
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        1
    );
    assert!(choices
        .snapshot(&state, &ticket, Duration::from_secs(2), || {})
        .is_err());
}

#[cfg(windows)]
#[test]
fn learning_import_course_source_changes_before_guard_are_not_staged() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_sqlite};
    use std::time::Duration;
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let source = open_source(temp.path());
    let path = temp.path().join("source.sqlite");
    let mode: String = source
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    source
        .execute(
            "UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    close_source_without_checkpoint(source);
    let bound = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    let choices = SyntheticSqliteSelections::default();
    let ticket = choices
        .select(&state, &library, &profile.id, &profile.access_epoch, || {
            Some(path.clone())
        })
        .unwrap()
        .unwrap();
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let batch = course_batch(&state, &scope, &profile, &"c".repeat(64));
    let approved = choices
        .course_preview(&state, &ticket, &scope, &bindings, Duration::from_secs(2))
        .unwrap();
    assert!(choices
        .course_write(
            &state,
            &approved,
            Duration::from_secs(2),
            || {
                let changed = Connection::open(&path).unwrap();
                changed
                    .execute(
                        "UPDATE playback_progress SET position_ms=13000 WHERE item_id='course-1'",
                        [],
                    )
                    .unwrap();
                close_source_without_checkpoint(changed);
            },
            |snapshot, preview| super::course_apply::stage_course_batch(
                &state, &scope, &batch, snapshot, &bindings, preview
            )
        )
        .is_err());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        0
    );
}

#[cfg(windows)]
#[test]
fn learning_import_course_source_active_writer_prevents_guarded_stage() {
    use super::{import_snapshot::SyntheticSqliteSelections, source_sqlite};
    use std::time::Duration;
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let source = open_source(temp.path());
    let path = temp.path().join("source.sqlite");
    let mode: String = source
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    source
        .execute(
            "UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    let bound = source_sqlite::bind_selected(&state, &scope, &path).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    let choices = SyntheticSqliteSelections::default();
    let ticket = choices
        .select(&state, &library, &profile.id, &profile.access_epoch, || {
            Some(path)
        })
        .unwrap()
        .unwrap();
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let batch = course_batch(&state, &scope, &profile, &"d".repeat(64));
    let approved = choices
        .course_preview(&state, &ticket, &scope, &bindings, Duration::from_secs(2))
        .unwrap();
    assert!(choices
        .course_write(
            &state,
            &approved,
            Duration::from_secs(2),
            || {},
            |snapshot, preview| super::course_apply::stage_course_batch(
                &state, &scope, &batch, snapshot, &bindings, preview
            )
        )
        .is_err());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        0
    );
}

#[cfg(windows)]
#[test]
fn learning_import_course_stage_rejects_changed_source_identity_binding() {
    use super::source_sqlite;
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let source = open_source(temp.path());
    let bound =
        source_sqlite::bind_selected(&state, &scope, &temp.path().join("source.sqlite")).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let approved =
        super::course_apply::preview_course_batch(&state, &scope, &profile.id, &source, &bindings)
            .unwrap();
    let batch = course_batch(&state, &scope, &profile, &"e".repeat(64));
    state.with_library(|db, _| { db.execute("UPDATE learning_source_sqlite_bindings SET file_identity='damaged' WHERE source_profile_id=?1", [&profile.id]).unwrap(); Ok(()) }).unwrap();
    assert!(super::course_apply::stage_course_batch(
        &state, &scope, &batch, &source, &bindings, &approved
    )
    .is_err());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        0
    );
}

#[cfg(windows)]
#[test]
fn learning_import_course_transaction_rechecks_source_identity_binding() {
    use super::source_sqlite;
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let source = open_source(temp.path());
    let bound =
        source_sqlite::bind_selected(&state, &scope, &temp.path().join("source.sqlite")).unwrap();
    let profile = source_profiles::get(&state, &scope, bound.profile_id()).unwrap();
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let approved =
        super::course_apply::preview_course_batch(&state, &scope, &profile.id, &source, &bindings)
            .unwrap();
    let batch = course_batch(&state, &scope, &profile, &"f".repeat(64));
    let trigger = "CREATE TRIGGER change_source_identity AFTER UPDATE OF created_at ON learning_import_batches BEGIN UPDATE learning_source_sqlite_bindings SET file_identity='win2:00000000:0000000000000001:0000000000000001' WHERE source_profile_id=NEW.source_profile_id; END;";
    state
        .with_library(|db, _| {
            db.execute_batch(trigger).unwrap();
            Ok(())
        })
        .unwrap();
    assert!(super::course_apply::stage_course_batch(
        &state, &scope, &batch, &source, &bindings, &approved
    )
    .is_err());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        0
    );
    assert!(bound.check(&state).is_ok());
    state
        .with_library(|db, _| {
            db.execute_batch("DROP TRIGGER change_source_identity")
                .unwrap();
            Ok(())
        })
        .unwrap();
    super::course_apply::stage_course_batch(&state, &scope, &batch, &source, &bindings, &approved)
        .unwrap();
    state
        .with_library(|db, _| {
            db.execute_batch(trigger).unwrap();
            Ok(())
        })
        .unwrap();
    assert!(super::course_apply::commit_course_batch(
        &state, &scope, &batch, &source, &bindings, &approved
    )
    .is_err());
    assert!(course_row(&state, &media).is_none());
    assert!(bound.check(&state).is_ok());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        1
    );
}

fn preview_for_batch(
    state: &crate::state::AppState,
    scope: &source_profiles::LibraryScope,
    batch: &str,
    source: &Connection,
    bindings: &[CourseMediaBinding],
) -> Result<super::course_apply::CourseStagePreview, String> {
    let profile: String = source_profiles::scoped(state, scope, |db| {
        db.query_row(
            "SELECT source_profile_id FROM learning_import_batches WHERE id=?1",
            [batch],
            |row| row.get(0),
        )
        .map_err(|_| "迁入批次不存在".into())
    })?;
    super::course_apply::preview_course_batch(state, scope, &profile, source, bindings)
}

pub(super) fn stage_course_batch(
    state: &crate::state::AppState,
    scope: &source_profiles::LibraryScope,
    batch: &str,
    source: &Connection,
    bindings: &[CourseMediaBinding],
) -> Result<(), String> {
    let approved = preview_for_batch(state, scope, batch, source, bindings)?;
    super::course_apply::stage_course_batch(state, scope, batch, source, bindings, &approved)
}

pub(super) fn commit_course_batch(
    state: &crate::state::AppState,
    scope: &source_profiles::LibraryScope,
    batch: &str,
    source: &Connection,
    bindings: &[CourseMediaBinding],
) -> Result<super::course_apply::CourseApplyReceipt, String> {
    let approved = preview_for_batch(state, scope, batch, source, bindings)?;
    super::course_apply::commit_course_batch(state, scope, batch, source, bindings, &approved)
}

#[test]
fn learning_import_course_stage_rejects_source_or_target_changes_after_preview() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = open_source(temp.path());
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let approved =
        super::course_apply::preview_course_batch(&state, &scope, &profile.id, &source, &bindings)
            .unwrap();
    assert_eq!(
        approved.preview().series[0].items[0]
            .source_progress
            .as_ref()
            .unwrap()
            .position_ms,
        11000
    );
    let first = course_batch(&state, &scope, &profile, &"a".repeat(64));
    source
        .execute(
            "UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    assert_eq!(
        super::course_apply::stage_course_batch(
            &state, &scope, &first, &source, &bindings, &approved
        )
        .unwrap_err(),
        "课程来源已变化，请重新预检"
    );
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &first
        ),
        0
    );

    let approved =
        super::course_apply::preview_course_batch(&state, &scope, &profile.id, &source, &bindings)
            .unwrap();
    let second = course_batch(&state, &scope, &profile, &"b".repeat(64));
    state
        .with_library(|db, _| {
            insert_course(db, &media, "用户已有课程", 9000, 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        super::course_apply::stage_course_batch(
            &state, &scope, &second, &source, &bindings, &approved
        )
        .unwrap_err(),
        "目标课程或媒体绑定已变化，请重新预检"
    );
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &second
        ),
        0
    );
}

#[test]
fn learning_import_course_stage_rejects_preview_from_another_source_profile() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let first = source_profiles::create(&state, &scope).unwrap();
    let second = source_profiles::create(&state, &scope).unwrap();
    let source = open_source(temp.path());
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let approved =
        super::course_apply::preview_course_batch(&state, &scope, &first.id, &source, &bindings)
            .unwrap();
    let batch = course_batch(&state, &scope, &second, &"c".repeat(64));
    assert!(super::course_apply::stage_course_batch(
        &state, &scope, &batch, &source, &bindings, &approved
    )
    .is_err());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        0
    );
}

#[test]
fn learning_import_course_stage_rejects_preview_from_an_old_library_session() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = open_source(temp.path());
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let approved =
        super::course_apply::preview_course_batch(&state, &scope, &profile.id, &source, &bindings)
            .unwrap();
    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    let new_scope = source_profiles::scope(&state, &library).unwrap();
    let batch = course_batch(&state, &new_scope, &profile, &"d".repeat(64));
    assert!(super::course_apply::stage_course_batch(
        &state, &new_scope, &batch, &source, &bindings, &approved
    )
    .is_err());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        0
    );
}

#[test]
fn learning_import_course_commit_requires_preview_from_current_library_session() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = open_source(temp.path());
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let bindings = [bind("course-1", &media)];
    let approved =
        super::course_apply::preview_course_batch(&state, &scope, &profile.id, &source, &bindings)
            .unwrap();
    let batch = course_batch(&state, &scope, &profile, &"e".repeat(64));
    super::course_apply::stage_course_batch(&state, &scope, &batch, &source, &bindings, &approved)
        .unwrap();
    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    let new_scope = source_profiles::scope(&state, &library).unwrap();
    assert!(super::course_apply::commit_course_batch(
        &state, &new_scope, &batch, &source, &bindings, &approved
    )
    .is_err());
    assert!(course_row(&state, &media).is_none());
    let current = super::course_apply::preview_course_batch(
        &state,
        &new_scope,
        &profile.id,
        &source,
        &bindings,
    )
    .unwrap();
    assert_eq!(
        super::course_apply::commit_course_batch(
            &state, &new_scope, &batch, &source, &bindings, &current
        )
        .unwrap()
        .status,
        "committed"
    );
}

fn open_source(dir: &std::path::Path) -> Connection {
    let path = dir.join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    Connection::open(path).unwrap()
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

fn insert_course(
    db: &Connection,
    media: &str,
    title: &str,
    position: i64,
    revision: i64,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO learning_courses(id,media_entry_id,title_override,position_ms,duration_ms,rate,completed,progress_revision,created_at,updated_at) VALUES(?1,?2,?3,?4,60000,1,0,?5,'now','now')",
        params![id, media, title, position, revision],
    )
    .unwrap();
    id
}

fn course_batch(
    state: &crate::state::AppState,
    scope: &source_profiles::LibraryScope,
    profile: &source_profiles::SourceProfile,
    hash: &str,
) -> String {
    create_course_batch(state, scope, &profile.id, &profile.access_epoch, hash).unwrap()
}

fn scalar(state: &crate::state::AppState, sql: &str, id: &str) -> i64 {
    state
        .with_library(|db, _| {
            db.query_row(sql, [id], |row| row.get(0))
                .map_err(|error| error.to_string())
        })
        .unwrap()
}

fn count_all(state: &crate::state::AppState, sql: &str) -> i64 {
    state
        .with_library(|db, _| {
            db.query_row(sql, [], |row| row.get(0))
                .map_err(|error| error.to_string())
        })
        .unwrap()
}

fn entry(
    state: &crate::state::AppState,
    batch: &str,
    kind: &str,
    source_id: &str,
) -> (String, Option<String>, Option<String>, Value) {
    state
        .with_library(|db, _| {
            let (status, code, target, receipt): (String, Option<String>, Option<String>, String) = db
                .query_row(
                    "SELECT status,conflict_code,target_id,receipt_json FROM learning_course_import_entries WHERE batch_id=?1 AND entry_kind=?2 AND source_id=?3",
                    params![batch, kind, source_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(|error| error.to_string())?;
            let receipt = serde_json::from_str(&receipt).map_err(|_| "回执无法读取".to_string())?;
            Ok((status, code, target, receipt))
        })
        .unwrap()
}

fn course_row(
    state: &crate::state::AppState,
    media: &str,
) -> Option<(String, i64, Option<i64>, i64, String)> {
    state
        .with_library(|db, _| {
            db.query_row(
                "SELECT id,position_ms,duration_ms,progress_revision,title_override FROM learning_courses WHERE media_entry_id=?1",
                [media],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(|error| error.to_string())
        })
        .unwrap()
}

fn origin_key(state: &crate::state::AppState, kind: &str, target: &str) -> String {
    state
        .with_library(|db, _| {
            db.query_row(
                "SELECT source_key_json FROM learning_origin_map WHERE target_kind=?1 AND target_id=?2",
                params![kind, target],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
        })
        .unwrap()
}

#[test]
fn learning_import_course_apply_commits_unmapped_progress_and_withholds_partial_series() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = open_source(temp.path());
    source
        .execute(
            "INSERT INTO series(id,title,description,sort_order,created_at,updated_at) VALUES('series-solo','独立系列','说明',1,1,1)",
            [],
        )
        .unwrap();
    source
        .execute(
            "INSERT INTO series_items(id,series_id,title,sort_order,duration_ms,created_at,updated_at) VALUES('solo-a','series-solo','课次甲',0,5000,1,1)",
            [],
        )
        .unwrap();
    source
        .execute(
            "INSERT INTO series_items(id,series_id,title,sort_order,duration_ms,created_at,updated_at) VALUES('solo-b','series-solo','课次乙',1,7000,1,1)",
            [],
        )
        .unwrap();
    source
        .execute(
            "INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('solo-a',22000,5000,0,1,1)",
            [],
        )
        .unwrap();
    source
        .execute(
            "INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('solo-b',33000,7000,0,1,1)",
            [],
        )
        .unwrap();
    let (media_a, media_b, media_c) = state
        .with_library(|db, _| {
            Ok((
                insert_media(db, None),
                insert_media(db, None),
                insert_media(db, None),
            ))
        })
        .unwrap();
    let bindings = [
        bind("course-1", &media_a),
        bind("solo-b", &media_c),
        bind("solo-a", &media_b),
    ];
    let batch = course_batch(&state, &scope, &profile, &"a".repeat(64));
    stage_course_batch(&state, &scope, &batch, &source, &bindings).unwrap();
    stage_course_batch(&state, &scope, &batch, &source, &bindings).unwrap();
    let receipt = commit_course_batch(&state, &scope, &batch, &source, &bindings).unwrap();
    assert_eq!(receipt.status, "committed");
    assert_eq!(
        (
            receipt.courses_committed,
            receipt.course_conflicts,
            receipt.series_committed,
            receipt.series_conflicts
        ),
        (3, 1, 1, 1)
    );
    let entries = super::course_batch::list_course_entries(
        &state,
        &state.active_library_id().unwrap(),
        &batch,
        0,
    )
    .unwrap();
    assert_eq!(entries.items.len(), 6);
    assert!(!entries.has_more);
    assert!(entries
        .items
        .iter()
        .any(|entry| entry.source_id == "course-2"
            && entry.status == "conflict"
            && entry.conflict_code.as_deref() == Some("unresolved")));
    assert!(entries.items.iter().any(|entry| entry.source_id == "solo-a"
        && entry.status == "committed"
        && entry.target_id.is_some()));
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &batch
        ),
        0
    );
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_import_items WHERE batch_id=?1",
            &batch
        ),
        0
    );
    let course_1 = course_row(&state, &media_a).unwrap();
    let solo_a = course_row(&state, &media_b).unwrap();
    let solo_b = course_row(&state, &media_c).unwrap();
    assert_eq!(
        (course_1.1, course_1.2, course_1.3, course_1.4.as_str()),
        (11000, Some(60000), 1, "同内容不同课次")
    );
    assert_eq!(
        (solo_a.1, solo_a.2, solo_a.3, solo_a.4.as_str()),
        (22000, Some(5000), 1, "课次甲")
    );
    assert_eq!(
        (solo_b.1, solo_b.2, solo_b.3, solo_b.4.as_str()),
        (33000, Some(7000), 1, "课次乙")
    );
    assert!(origin_key(&state, "course", &course_1.0).contains("\"course-1\""));
    assert!(origin_key(&state, "course", &solo_a.0).contains("\"solo-a\""));
    assert!(origin_key(&state, "course", &solo_b.0).contains("\"solo-b\""));
    let (series_id, description, revision) = state
        .with_library(|db, _| {
            db.query_row(
                "SELECT id,description,revision FROM learning_series WHERE name='独立系列'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!((description.as_str(), revision), ("说明", 1));
    assert!(origin_key(&state, "series", &series_id).contains("\"series-solo\""));
    let members = state
        .with_library(|db, _| {
            let mut statement = db
                .prepare("SELECT course_id FROM learning_series_courses WHERE series_id=?1 ORDER BY position")
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([&series_id], |row| row.get::<_, String>(0))
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(members, vec![solo_a.0.clone(), solo_b.0.clone()]);
    assert_eq!(
        count_all(
            &state,
            "SELECT count(*) FROM learning_series WHERE name='合成系列'"
        ),
        0
    );
    let withheld = entry(&state, &batch, "series", "series-1");
    assert_eq!(withheld.0, "conflict");
    assert_eq!(withheld.1.as_deref(), Some("membership_withheld"));
    assert_eq!(
        entry(&state, &batch, "course", "course-2").1.as_deref(),
        Some("unresolved")
    );
    assert_eq!(
        source
            .query_row(
                "SELECT position_ms FROM playback_progress WHERE item_id='course-1'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        11000
    );

    source
        .execute(
            "UPDATE playback_progress SET position_ms=99999 WHERE item_id='solo-a'",
            [],
        )
        .unwrap();
    let replay = commit_course_batch(&state, &scope, &batch, &source, &bindings).unwrap();
    assert_eq!(replay, receipt);
    assert_eq!(course_row(&state, &media_b).unwrap().1, 22000);

    let again = course_batch(&state, &scope, &profile, &"b".repeat(64));
    let fresh = [
        bind("solo-a", &media_b),
        bind("solo-b", &media_c),
        bind("course-1", &media_a),
    ];
    stage_course_batch(&state, &scope, &again, &source, &fresh).unwrap();
    let second = commit_course_batch(&state, &scope, &again, &source, &fresh).unwrap();
    assert_eq!(second.status, "committed");
    assert_eq!(course_row(&state, &media_b).unwrap().1, 22000);
    assert_eq!(course_row(&state, &media_b).unwrap().3, 1);
    assert_eq!(
        state
            .with_library(|db, _| {
                db.query_row(
                    "SELECT revision FROM learning_series WHERE id=?1",
                    [&series_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap(),
        1
    );
    let changed = entry(&state, &again, "course", "solo-a");
    assert_eq!(changed.1.as_deref(), Some("source_changed"));
    assert_eq!(changed.3["sourcePositionMs"], 99999);
    assert_eq!(changed.3["targetPositionMs"], 22000);
    assert_eq!(changed.2.as_deref(), Some(solo_a.0.as_str()));
    assert_eq!(
        entry(&state, &again, "series", "series-solo").0,
        "committed"
    );
    assert_eq!(entry(&state, &again, "course", "course-1").0, "committed");
}

#[test]
fn learning_import_course_apply_does_not_coalesce_or_overwrite_divergent_progress() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = open_source(temp.path());
    source
        .execute(
            "INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('course-2',5000,60000,0,1,1)",
            [],
        )
        .unwrap();
    let (shared, existing) = state
        .with_library(|db, _| {
            let shared = insert_media(db, Some(60000));
            let existing = insert_media(db, Some(60000));
            insert_course(db, &existing, "", 9000, 7);
            Ok((shared, existing))
        })
        .unwrap();
    let shared_bindings = [bind("course-1", &shared), bind("course-2", &shared)];
    let first = course_batch(&state, &scope, &profile, &"c".repeat(64));
    stage_course_batch(&state, &scope, &first, &source, &shared_bindings).unwrap();
    let receipt = commit_course_batch(&state, &scope, &first, &source, &shared_bindings).unwrap();
    assert_eq!(receipt.courses_committed, 0);
    assert!(receipt.course_conflicts >= 2);
    assert!(course_row(&state, &shared).is_none());
    assert_eq!(
        entry(&state, &first, "course", "course-1").1.as_deref(),
        Some("different_source_progress")
    );
    assert_eq!(
        entry(&state, &first, "course", "course-1").3["sourcePositionMs"],
        11000
    );
    assert_eq!(
        entry(&state, &first, "course", "course-2").3["sourcePositionMs"],
        5000
    );
    assert!(entry(&state, &first, "course", "course-1")
        .3
        .get("targetPositionMs")
        .is_none());
    assert_eq!(
        count_all(&state, "SELECT count(*) FROM learning_origin_map"),
        0
    );

    source
        .execute(
            "UPDATE playback_progress SET position_ms=11000,duration_ms=60000,playback_rate=1,completed=0 WHERE item_id='course-2'",
            [],
        )
        .unwrap();
    let second = course_batch(&state, &scope, &profile, &"d".repeat(64));
    stage_course_batch(&state, &scope, &second, &source, &shared_bindings).unwrap();
    commit_course_batch(&state, &scope, &second, &source, &shared_bindings).unwrap();
    assert!(course_row(&state, &shared).is_none());
    assert_eq!(
        entry(&state, &second, "course", "course-1").1.as_deref(),
        Some("shared_media")
    );
    assert_eq!(
        entry(&state, &second, "course", "course-2").1.as_deref(),
        Some("shared_media")
    );

    let third = course_batch(&state, &scope, &profile, &"e".repeat(64));
    let divergent = [bind("course-1", &existing)];
    stage_course_batch(&state, &scope, &third, &source, &divergent).unwrap();
    commit_course_batch(&state, &scope, &third, &source, &divergent).unwrap();
    let kept = course_row(&state, &existing).unwrap();
    assert_eq!((kept.1, kept.3, kept.4.as_str()), (9000, 7, ""));
    let conflict = entry(&state, &third, "course", "course-1");
    assert_eq!(conflict.1.as_deref(), Some("divergent_progress"));
    assert_eq!(conflict.3["sourcePositionMs"], 11000);
    assert_eq!(conflict.3["targetPositionMs"], 9000);
    assert!(origin_for_target(&state, &kept.0).is_none());

    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_courses SET title_override='保持标题',position_ms=11000,progress_revision=8 WHERE id=?1",
                [&kept.0],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
    let fourth = course_batch(&state, &scope, &profile, &"f".repeat(64));
    stage_course_batch(&state, &scope, &fourth, &source, &divergent).unwrap();
    commit_course_batch(&state, &scope, &fourth, &source, &divergent).unwrap();
    let compatible = course_row(&state, &existing).unwrap();
    assert_eq!(
        (compatible.1, compatible.3, compatible.4.as_str()),
        (11000, 8, "保持标题")
    );
    assert_eq!(entry(&state, &fourth, "course", "course-1").0, "committed");
    assert_eq!(
        entry(&state, &fourth, "course", "course-1").3["progress"],
        "compatible"
    );
    assert!(origin_key(&state, "course", &compatible.0).contains("\"course-1\""));
    assert_eq!(
        count_all(
            &state,
            "SELECT count(*) FROM learning_series WHERE name='合成系列'"
        ),
        0
    );
}

fn origin_for_target(state: &crate::state::AppState, target: &str) -> Option<String> {
    state
        .with_library(|db, _| {
            db.query_row(
                "SELECT source_key_json FROM learning_origin_map WHERE target_id=?1",
                [target],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())
        })
        .unwrap()
}

#[test]
fn learning_import_course_apply_keeps_absent_progress_distinct_from_zero() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = open_source(temp.path());
    source.execute("DELETE FROM playback_progress", []).unwrap();
    source.execute("DELETE FROM series_items", []).unwrap();
    source.execute("DELETE FROM series", []).unwrap();
    source
        .execute_batch(
            "INSERT INTO series(id,title,sort_order,created_at,updated_at) VALUES('series-zero','零进度系列',0,1,1);
             INSERT INTO series(id,title,sort_order,created_at,updated_at) VALUES('series-absent','缺失进度系列',1,1,1);
             INSERT INTO series(id,title,sort_order,created_at,updated_at) VALUES('series-kept','保留进度系列',2,1,1);
             INSERT INTO series_items(id,series_id,title,sort_order,duration_ms,created_at,updated_at) VALUES('zero-item','series-zero','明确为零',0,1000,1,1);
             INSERT INTO series_items(id,series_id,title,sort_order,duration_ms,created_at,updated_at) VALUES('absent-item','series-absent','没有进度',0,1000,1,1);
             INSERT INTO series_items(id,series_id,title,sort_order,duration_ms,created_at,updated_at) VALUES('kept-item','series-kept','沿用目标',0,1000,1,1);
             INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('zero-item',0,1000,0,1,1);",
        )
        .unwrap();
    let (zero_media, absent_media, kept_media, kept_course) = state
        .with_library(|db, _| {
            let zero_media = insert_media(db, None);
            let absent_media = insert_media(db, None);
            let kept_media = insert_media(db, Some(60000));
            let kept_course = insert_course(db, &kept_media, "用户标题", 9000, 7);
            Ok((zero_media, absent_media, kept_media, kept_course))
        })
        .unwrap();
    let bindings = [
        bind("zero-item", &zero_media),
        bind("absent-item", &absent_media),
        bind("kept-item", &kept_media),
    ];
    let batch = course_batch(&state, &scope, &profile, &"1".repeat(64));
    stage_course_batch(&state, &scope, &batch, &source, &bindings).unwrap();
    commit_course_batch(&state, &scope, &batch, &source, &bindings).unwrap();
    let zero = course_row(&state, &zero_media).unwrap();
    assert_eq!(
        (zero.1, zero.2, zero.3, zero.4.as_str()),
        (0, Some(1000), 1, "明确为零")
    );
    assert_eq!(
        entry(&state, &batch, "course", "zero-item").3["progress"],
        "applied"
    );
    assert!(course_row(&state, &absent_media).is_none());
    let absent = entry(&state, &batch, "course", "absent-item");
    assert_eq!(absent.1.as_deref(), Some("progress_absent"));
    assert!(absent.3.get("sourcePositionMs").is_none());
    assert_eq!(
        entry(&state, &batch, "series", "series-absent")
            .1
            .as_deref(),
        Some("membership_withheld")
    );
    let kept = course_row(&state, &kept_media).unwrap();
    assert_eq!(kept.0, kept_course);
    assert_eq!((kept.1, kept.3, kept.4.as_str()), (9000, 7, "用户标题"));
    assert_eq!(
        entry(&state, &batch, "course", "kept-item").3["progress"],
        "absent"
    );
    assert!(origin_key(&state, "course", &kept.0).contains("\"kept-item\""));
    let kept_series = state
        .with_library(|db, _| {
            let series_id: String = db
                .query_row(
                    "SELECT id FROM learning_series WHERE name='保留进度系列'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let member: String = db
                .query_row(
                    "SELECT course_id FROM learning_series_courses WHERE series_id=?1",
                    [&series_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            Ok((series_id, member))
        })
        .unwrap();
    assert_eq!(kept_series.1, kept_course);
    assert!(origin_key(&state, "series", &kept_series.0).contains("\"series-kept\""));
    assert_eq!(
        count_all(
            &state,
            "SELECT count(*) FROM learning_series WHERE name='缺失进度系列'"
        ),
        0
    );
    assert_eq!(
        count_all(
            &state,
            "SELECT count(*) FROM learning_series WHERE name='零进度系列'"
        ),
        1
    );
}

#[test]
fn learning_import_course_apply_cancel_clears_stage_and_rejects_stale_and_note_ops() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let source = open_source(temp.path());
    let media = state
        .with_library(|db, _| Ok(insert_media(db, None)))
        .unwrap();
    let note = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"a".repeat(64),
    )
    .unwrap();
    let course = course_batch(&state, &scope, &profile, &"b".repeat(64));
    assert_eq!(
        stage_course_batch(
            &state,
            &scope,
            &note.id,
            &source,
            &[bind("course-1", &media)]
        )
        .unwrap_err(),
        COURSE_OP
    );
    assert_eq!(
        import_ops::commit_batch(&state, &scope, &course).unwrap_err(),
        NOTE_OP
    );
    assert!(import_ops::list_batches(&state, &scope)
        .unwrap()
        .iter()
        .all(|batch| batch.id != course));
    let fresh = course_batch(&state, &scope, &profile, &"c".repeat(64));
    assert_eq!(
        commit_course_batch(&state, &scope, &fresh, &source, &[bind("course-1", &media)])
            .unwrap_err(),
        "课程迁入尚未暂存"
    );
    let bindings = [bind("course-1", &media)];
    stage_course_batch(&state, &scope, &course, &source, &bindings).unwrap();
    source
        .execute(
            "UPDATE playback_progress SET position_ms=12000 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    assert_eq!(
        stage_course_batch(&state, &scope, &course, &source, &bindings).unwrap_err(),
        "课程迁入暂存已存在"
    );
    let error = commit_course_batch(&state, &scope, &course, &source, &bindings).unwrap_err();
    assert!(error.contains("来源"), "{error}");
    assert!(course_row(&state, &media).is_none());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &course
        ),
        1
    );
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_import_batches WHERE id=?1 AND status='pending'",
            &course
        ),
        1
    );
    let cancelled = cancel_course_batch(&state, &scope, &course).unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(cancelled.courses_committed, 0);
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_course_import_stage WHERE batch_id=?1",
            &course
        ),
        0
    );
    assert!(course_row(&state, &media).is_none());
    assert_eq!(
        scalar(
            &state,
            "SELECT count(*) FROM learning_import_batches WHERE id=?1 AND status='pending' AND batch_kind='notes'",
            &note.id
        ),
        1
    );
    assert_eq!(
        cancel_course_batch(&state, &scope, &note.id).unwrap_err(),
        COURSE_OP
    );
    assert_eq!(
        import_ops::cancel_batch(&state, &scope, &note.id)
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(
        cancel_course_batch(&state, &scope, &course).unwrap().status,
        "cancelled"
    );
}

#[test]
fn learning_import_course_apply_schema_upgrade_rolls_back_to_v36() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let note = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"a".repeat(64),
    )
    .unwrap();
    let course = course_batch(&state, &scope, &profile, &"b".repeat(64));
    state
        .with_library(|db, _| {
            db.execute_batch("DROP TABLE learning_course_import_entries; DROP TABLE learning_course_import_stage; UPDATE schema_meta SET value='36' WHERE key='schema_version'; CREATE TRIGGER reject_v37 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='37' BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
            assert!(super::migrate(db).is_err());
            assert_eq!(
                db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |row| row.get::<_, String>(0)).unwrap(),
                "36"
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM sqlite_schema WHERE name='learning_course_import_stage'", [], |row| row.get::<_, i64>(0)).unwrap(),
                0
            );
            db.execute_batch("DROP TRIGGER reject_v37").unwrap();
            super::migrate(db)?;
            super::migrate(db)?;
            assert_eq!(
                db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |row| row.get::<_, String>(0)).unwrap(),
                "42"
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM sqlite_schema WHERE name='learning_course_import_stage'", [], |row| row.get::<_, i64>(0)).unwrap(),
                1
            );
            assert_eq!(
                db.query_row("SELECT batch_kind FROM learning_import_batches WHERE id=?1", [&note.id], |row| row.get::<_, String>(0)).unwrap(),
                "notes"
            );
            assert_eq!(
                db.query_row("SELECT batch_kind FROM learning_import_batches WHERE id=?1", [&course], |row| row.get::<_, String>(0)).unwrap(),
                "courses"
            );
            Ok(())
        })
        .unwrap();
}
