use super::{
    course_import, import_access, import_course_preview::CourseMediaBinding, source_profiles,
};
use crate::state::test_state;
use rusqlite::params;

#[test]
fn learning_import_production_sqlite_rechecks_source_links_before_commit() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let snapshot = super::course_snapshot::open(&path).unwrap();
    std::fs::hard_link(&path, temp.path().join("alias.sqlite")).unwrap();
    assert!(snapshot.recheck().is_err());
}

#[test]
fn learning_import_production_sqlite_rejects_unrevertible_batch_at_preview() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = temp.path().join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let source = rusqlite::Connection::open(&path).unwrap();
    source.execute("INSERT INTO series(id,title,created_at,updated_at,sort_order) VALUES('series-2','独立系列',1,1,2)",[]).unwrap();
    source.execute("INSERT INTO series_items(id,series_id,title,sort_order,created_at,updated_at) VALUES('single','series-2','单门',1,1,1)",[]).unwrap();
    source.execute("INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('single',1000,60000,0,1,1)",[]).unwrap();
    source
        .execute(
            "UPDATE playback_progress SET position_ms=-1 WHERE item_id='course-1'",
            [],
        )
        .unwrap();
    let tx = source.unchecked_transaction().unwrap();
    for n in 0..10001 {
        tx.execute("INSERT INTO series_items(id,series_id,title,sort_order,created_at,updated_at) VALUES(?1,'series-1','course',?2,1,1)",params![format!("extra-{n}"),n]).unwrap();
    }
    tx.commit().unwrap();
    drop(source);
    let selected =
        import_access::select_sqlite_with_picker(&state, &library, || Some(path.clone()))
            .unwrap()
            .unwrap();
    assert!(
        course_import::preview_selected(&state, &library, &selected.selection_id, vec![]).is_err()
    );
    let scope = source_profiles::scope(&state, &library).unwrap();
    assert!(super::course_batch::list_course_batches(&state, &library)
        .unwrap()
        .is_empty());
    let catalog = course_import::list_series(&state, &library, &selected.selection_id).unwrap();
    assert!(catalog
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"] == "series-2" && s["courseCount"] == 1));
    let media = uuid::Uuid::new_v4().to_string();
    let file = uuid::Uuid::new_v4().to_string();
    state.with_library(|db,_|{db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','sample',?2,'now','now')",params![media,file]).unwrap();db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','sample.mp4','sample.mp4','sample.mp4','now')",params![file,media]).unwrap();Ok(())}).unwrap();
    let preview = course_import::preview_selected_subset(
        &state,
        &library,
        &selected.selection_id,
        vec![CourseMediaBinding {
            source_item_id: "single".into(),
            media_id: media.clone(),
        }],
        Some(vec!["series-2".into()]),
    )
    .unwrap();
    assert_eq!(preview["series"].as_array().unwrap().len(), 1);
    {
        let source = rusqlite::Connection::open(&path).unwrap();
        source
            .execute("UPDATE series SET title='changed' WHERE id='series-2'", [])
            .unwrap();
    }
    assert!(course_import::commit_selected(
        &state,
        &library,
        &selected.selection_id,
        preview["previewId"].as_str().unwrap(),
        || false
    )
    .is_err());
    let source_before = std::fs::read(&path).unwrap();
    let preview = course_import::preview_selected_subset(
        &state,
        &library,
        &selected.selection_id,
        vec![CourseMediaBinding {
            source_item_id: "single".into(),
            media_id: media,
        }],
        Some(vec!["series-2".into()]),
    )
    .unwrap();
    let result = course_import::commit_selected(
        &state,
        &library,
        &selected.selection_id,
        preview["previewId"].as_str().unwrap(),
        || false,
    )
    .unwrap();
    assert_eq!(result["status"], "committed");
    assert_eq!(result["receipt"]["coursesCommitted"], 1);
    assert_eq!(result["receipt"]["seriesCommitted"], 1);
    let report = super::course_apply::revert_course_batch(
        &state,
        &scope,
        &selected.epoch,
        result["batchId"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(report["reverted"].as_array().unwrap().len(), 2);
    assert_eq!(std::fs::read(&path).unwrap(), source_before);
}

#[test]
fn learning_import_production_sqlite_completes_series_after_later_binding() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = temp.path().join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let source = rusqlite::Connection::open(&path).unwrap();
    source.execute("INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('course-2',1000,60000,0,1,1)",[]).unwrap();
    drop(source);
    let selection = import_access::select_sqlite_with_picker(&state, &library, || Some(path))
        .unwrap()
        .unwrap();
    let mut bindings = Vec::new();
    state.with_library(|db,_|{for id in ["course-1","course-2"]{let media=uuid::Uuid::new_v4().to_string();let file=uuid::Uuid::new_v4().to_string();db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','sample',?2,'now','now')",params![media,file]).unwrap();db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','sample.mp4','sample.mp4',?3,'now')",params![file,media,format!("{file}.mp4")]).unwrap();bindings.push(CourseMediaBinding{source_item_id:id.into(),media_id:media});}Ok(())}).unwrap();
    let first = course_import::preview_selected_subset(
        &state,
        &library,
        &selection.selection_id,
        vec![bindings[0].clone()],
        Some(vec!["series-1".into()]),
    )
    .unwrap();
    let first_result = course_import::commit_selected(
        &state,
        &library,
        &selection.selection_id,
        first["previewId"].as_str().unwrap(),
        || false,
    )
    .unwrap();
    assert_eq!(first_result["receipt"]["coursesCommitted"], 1);
    assert_eq!(first_result["receipt"]["seriesConflicts"], 1);
    let second = course_import::preview_selected_subset(
        &state,
        &library,
        &selection.selection_id,
        bindings,
        Some(vec!["series-1".into()]),
    )
    .unwrap();
    let result = course_import::commit_selected(
        &state,
        &library,
        &selection.selection_id,
        second["previewId"].as_str().unwrap(),
        || false,
    )
    .unwrap();
    assert_eq!(result["receipt"]["coursesCommitted"], 2);
    assert_eq!(result["receipt"]["seriesCommitted"], 1);
    let scope = source_profiles::scope(&state, &library).unwrap();
    let first_report = super::course_apply::revert_course_batch(
        &state,
        &scope,
        &selection.epoch,
        first_result["batchId"].as_str().unwrap(),
    )
    .unwrap();
    assert!(first_report["kept"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["targetKind"] == "course"
            && matches!(
                item["outcome"].as_str(),
                Some("kept_reused" | "kept_modified")
            )));
    assert_eq!(
        state
            .with_library(|db, _| Ok(db
                .query_row("SELECT count(*) FROM learning_courses", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()))
            .unwrap(),
        2
    );
    assert_eq!(
        state
            .with_library(|db, _| Ok(db
                .query_row("SELECT count(*) FROM learning_series", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()))
            .unwrap(),
        1
    );
    super::course_apply::revert_course_batch(
        &state,
        &scope,
        &selection.epoch,
        result["batchId"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        state
            .with_library(|db, _| Ok(db
                .query_row("SELECT count(*) FROM learning_courses", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()))
            .unwrap(),
        1
    );
}

#[test]
fn learning_import_production_sqlite_preview_warns_about_invalid_series() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = temp.path().join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let source = rusqlite::Connection::open(&path).unwrap();
    source
        .execute(
            "UPDATE series SET title=' invalid ' WHERE id='series-1'",
            [],
        )
        .unwrap();
    drop(source);
    let selected = import_access::select_sqlite_with_picker(&state, &library, || Some(path))
        .unwrap()
        .unwrap();
    let preview =
        course_import::preview_selected(&state, &library, &selected.selection_id, vec![]).unwrap();
    let series = preview["series"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "series-1")
        .unwrap();
    assert!(series["conflictCodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "invalid_series"));
}

#[test]
fn learning_import_production_sqlite_reverts_full_capacity_with_all_targets_kept() {
    exercise_large_revert(397, 400, true);
}

#[test]
fn learning_import_production_sqlite_reverts_large_single_series() {
    exercise_large_revert(597, 600, true);
}

#[test]
fn learning_import_production_sqlite_removes_unchanged_large_single_series() {
    exercise_large_revert(597, 600, false);
}

fn exercise_large_revert(extra: usize, expected: usize, keep: bool) {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = temp.path().join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let mut source = rusqlite::Connection::open(&path).unwrap();
    let tx = source.transaction().unwrap();
    tx.execute("INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('course-2',1000,60000,0,1,1)",[]).unwrap();
    for n in 0..extra {
        let id = format!("extra-{n}");
        tx.execute("INSERT INTO series_items(id,series_id,title,sort_order,created_at,updated_at) VALUES(?1,'series-1','course',?2,1,1)",params![id,(n+2) as i64]).unwrap();
        tx.execute("INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES(?1,1000,60000,0,1,1)",[id]).unwrap();
    }
    tx.commit().unwrap();
    drop(source);
    let selected = import_access::select_sqlite_with_picker(&state, &library, || Some(path))
        .unwrap()
        .unwrap();
    let mut bindings = Vec::new();
    state.with_library(|db,_|{
        for id in ["course-1".to_string(),"course-2".to_string()].into_iter().chain((0..extra).map(|n|format!("extra-{n}"))) {
            let media=uuid::Uuid::new_v4().to_string();let file=uuid::Uuid::new_v4().to_string();
            db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','synthetic',?2,'now','now')",params![media,file]).unwrap();
            db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','synthetic.mp4','synthetic.mp4',?3,'now')",params![file,media,format!("synthetic-{file}.mp4")]).unwrap();
            bindings.push(CourseMediaBinding{source_item_id:id,media_id:media});
        }
        Ok(())
    }).unwrap();
    let preview =
        course_import::preview_selected(&state, &library, &selected.selection_id, bindings)
            .unwrap();
    let result = course_import::commit_selected(
        &state,
        &library,
        &selected.selection_id,
        preview["previewId"].as_str().unwrap(),
        || false,
    )
    .unwrap();
    assert_eq!(
        result["receipt"]["coursesCommitted"].as_u64(),
        Some((expected - 1) as u64)
    );
    assert_eq!(result["receipt"]["seriesCommitted"], 1);
    if keep {
        state
            .with_library(|db, _| {
                db.execute("UPDATE learning_courses SET title_override='user edit'", [])
                    .unwrap();
                db.execute("UPDATE learning_series SET name='user edit'", [])
                    .unwrap();
                Ok(())
            })
            .unwrap();
    }
    let scope = source_profiles::scope(&state, &library).unwrap();
    let batch_id = result["batchId"].as_str().unwrap();
    let report = if expected > 400 && keep {
        let request = super::import_runner::ImportTaskRequest::RevertCourse {
            batch_id: batch_id.into(),
            expected_epoch: selected.epoch.clone(),
        };
        let (task, _) = state
            .tasks
            .submit(
                library.clone(),
                crate::tasks::TaskRequest::LearningImport(request.clone()),
            )
            .unwrap();
        let receipt = super::import_runner::run_local(&state, &task, &request).unwrap();
        assert_eq!(receipt["batchId"], batch_id);
        assert_eq!(receipt["summary"]["revertedCount"], 0);
        assert_eq!(receipt["summary"]["keptCount"], 600);
        assert_eq!(receipt["summary"]["archivesKept"], true);
        assert!(receipt.get("report").is_none());
        let journal = temp.path().join("library/task-journal.json");
        assert!(std::fs::metadata(&journal).unwrap().len() < 4096);
        assert_eq!(
            crate::tasks::TaskRegistry::load(journal)
                .get(&task.id)
                .unwrap()
                .result,
            Some(receipt)
        );
        super::course_batch::list_course_entries(&state, &library, batch_id, 0)
            .unwrap()
            .revert_report
            .unwrap()
    } else {
        super::course_apply::revert_course_batch(&state, &scope, &selected.epoch, batch_id).unwrap()
    };
    if keep {
        assert_eq!(report["kept"].as_array().unwrap().len(), expected)
    } else {
        assert_eq!(report["reverted"].as_array().unwrap().len(), expected);
        assert_eq!(
            state
                .with_library(|db, _| Ok(db
                    .query_row("SELECT count(*) FROM learning_courses", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap()))
                .unwrap(),
            0
        );
        assert_eq!(
            state
                .with_library(|db, _| Ok(db
                    .query_row("SELECT count(*) FROM learning_series", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap()))
                .unwrap(),
            0
        );
    }
    if expected > 400 && keep {
        assert!(report.to_string().len() > 65536);
        let replay = super::course_apply::revert_course_batch(
            &state,
            &scope,
            &selected.epoch,
            result["batchId"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(replay, report);
        let archive = temp.path().join("large-revert-backup.dccassetlib");
        state
            .with_library(|db, base| crate::backup::export_library(db, base, &archive))
            .unwrap();
        let restored = test_state(&temp.path().join("restored"));
        restored
            .with_active_mut(|active| crate::backup::restore_library(active, &archive))
            .unwrap();
        let restored_library = restored.active_library_id().unwrap();
        let detail = super::course_batch::list_course_entries(
            &restored,
            &restored_library,
            result["batchId"].as_str().unwrap(),
            0,
        )
        .unwrap();
        assert_eq!(detail.revert_report.as_ref(), Some(&report));
        assert!(!detail.revert_damaged);
    }
}

#[test]
fn learning_import_production_sqlite_preview_commit_revert_loop() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = temp.path().join("source.sqlite");
    let original = include_bytes!("fixtures/yingliu-migration-sample.sqlite");
    std::fs::write(&path, original).unwrap();
    {
        let source = rusqlite::Connection::open(&path).unwrap();
        source.execute("INSERT INTO playback_progress(item_id,position_ms,duration_ms,completed,playback_rate,updated_at) VALUES('course-2',1000,60000,0,1,1)",[]).unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let selected =
        import_access::select_sqlite_with_picker(&state, &library, || Some(path.clone()))
            .unwrap()
            .unwrap();
    let media = uuid::Uuid::new_v4().to_string();
    let file = uuid::Uuid::new_v4().to_string();
    let media2 = uuid::Uuid::new_v4().to_string();
    let file2 = uuid::Uuid::new_v4().to_string();
    state.with_library(|db,_| {
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','synthetic',?2,'now','now')",params![media,file]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','synthetic.mp4','synthetic.mp4','synthetic.mp4','now')",params![file,media]).unwrap();
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','synthetic2',?2,'now','now')",params![media2,file2]).unwrap();
        db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,created_at) VALUES(?1,?2,'main','synthetic2.mp4','synthetic2.mp4','synthetic2.mp4','now')",params![file2,media2]).unwrap();
        Ok(())
    }).unwrap();
    let preview = course_import::preview_selected(
        &state,
        &library,
        &selected.selection_id,
        vec![
            CourseMediaBinding {
                source_item_id: "course-1".into(),
                media_id: media.clone(),
            },
            CourseMediaBinding {
                source_item_id: "course-2".into(),
                media_id: media2.clone(),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        state
            .with_library(|db, _| Ok(db
                .query_row("SELECT count(*) FROM learning_courses", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()))
            .unwrap(),
        0
    );
    let preview_id = preview["previewId"].as_str().unwrap();
    let (task, _) =
        course_import::queue_selected(&state, &library, &selected.selection_id, preview_id)
            .unwrap();
    let (duplicate, _) =
        course_import::queue_selected(&state, &library, &selected.selection_id, preview_id)
            .unwrap();
    assert_eq!(task.id, duplicate.id);
    assert!(crate::tasks::ensure_generic_ipc_payload(&task.payload).is_err());
    let crate::tasks::TaskRequest::LearningImport(request) = &task.payload else {
        panic!("wrong task")
    };
    let result = super::import_runner::run_local(&state, &task, request).unwrap();
    assert_eq!(result["status"], "committed");
    assert_eq!(result["receipt"]["coursesCommitted"], 2);
    assert!(course_import::commit_selected(
        &state,
        &library,
        &selected.selection_id,
        preview["previewId"].as_str().unwrap(),
        || false
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    {
        let source = rusqlite::Connection::open(&path).unwrap();
        source
            .execute(
                "UPDATE series SET title='changed series' WHERE id='series-1'",
                [],
            )
            .unwrap();
        source
            .execute(
                "UPDATE series_items SET title='changed course' WHERE id='course-1'",
                [],
            )
            .unwrap();
    }
    let changed = course_import::preview_selected(
        &state,
        &library,
        &selected.selection_id,
        vec![
            CourseMediaBinding {
                source_item_id: "course-1".into(),
                media_id: media.clone(),
            },
            CourseMediaBinding {
                source_item_id: "course-2".into(),
                media_id: media2.clone(),
            },
        ],
    )
    .unwrap();
    let series = changed["series"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "series-1")
        .unwrap();
    assert!(series["conflictCodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "source_changed"));
    let item = series["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "course-1")
        .unwrap();
    assert!(item["conflictCodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "source_changed"));
    let scope = source_profiles::scope(&state, &library).unwrap();
    let report = super::course_apply::revert_course_batch(
        &state,
        &scope,
        &selected.epoch,
        result["batchId"].as_str().unwrap(),
    )
    .unwrap();
    assert!(!report["reverted"].as_array().unwrap().is_empty());
    assert!(!temp.path().join("source.sqlite-wal").exists());
    assert!(!temp.path().join("source.sqlite-shm").exists());
}

#[test]
fn learning_import_production_sqlite_rejects_changed_source_and_revoked_selection() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = temp.path().join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let selected =
        import_access::select_sqlite_with_picker(&state, &library, || Some(path.clone()))
            .unwrap()
            .unwrap();
    let first =
        course_import::preview_selected(&state, &library, &selected.selection_id, vec![]).unwrap();
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer
        .execute("UPDATE series SET title='changed' WHERE id='series-1'", [])
        .unwrap();
    drop(writer);
    assert!(course_import::commit_selected(
        &state,
        &library,
        &selected.selection_id,
        first["previewId"].as_str().unwrap(),
        || false
    )
    .is_err());
    let second =
        course_import::preview_selected(&state, &library, &selected.selection_id, vec![]).unwrap();
    assert!(
        import_access::release_sqlite_selection(&state, &library, &selected.selection_id).unwrap()
    );
    assert!(course_import::queue_selected(
        &state,
        &library,
        &selected.selection_id,
        second["previewId"].as_str().unwrap()
    )
    .is_err());
    assert_eq!(
        state
            .with_library(|db, _| Ok(db
                .query_row("SELECT count(*) FROM learning_courses", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()))
            .unwrap(),
        0
    );
}

#[test]
fn learning_import_production_sqlite_reads_offline_wal_without_modifying_source() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let path = temp.path().join("source.sqlite");
    std::fs::write(
        &path,
        include_bytes!("fixtures/yingliu-migration-sample.sqlite"),
    )
    .unwrap();
    let writer = rusqlite::Connection::open(&path).unwrap();
    let _: String = writer
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    writer
        .execute_batch(
            "PRAGMA wal_autocheckpoint=0; UPDATE series SET title='WAL-only' WHERE id='series-1';",
        )
        .unwrap();
    assert!(
        import_access::select_sqlite_with_picker(&state, &library, || Some(path.clone())).is_err()
    );
    let names = ["source.sqlite", "source.sqlite-wal", "source.sqlite-shm"];
    let before: Vec<_> = names
        .iter()
        .map(|name| std::fs::read(temp.path().join(name)).unwrap())
        .collect();
    drop(writer);
    for (name, bytes) in names.iter().zip(&before) {
        std::fs::write(temp.path().join(name), bytes).unwrap()
    }
    let selected =
        import_access::select_sqlite_with_picker(&state, &library, || Some(path.clone()))
            .unwrap()
            .unwrap();
    let preview =
        course_import::preview_selected(&state, &library, &selected.selection_id, vec![]).unwrap();
    assert!(preview["series"]
        .as_array()
        .unwrap()
        .iter()
        .any(|series| series["title"] == "WAL-only"));
    for (name, bytes) in names.iter().zip(before) {
        assert_eq!(std::fs::read(temp.path().join(name)).unwrap(), bytes)
    }
}
