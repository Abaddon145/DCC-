use super::{course_batch::create_course_batch, import_ops, source_profiles};
use crate::state::test_state;
use sha2::{Digest, Sha256};

#[test]
fn course_membership_is_derived_and_explicit_learning_membership_survives_removal() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    state.with_library(|db, _| {
        db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('video','video','New','file','now','now')", []).unwrap();
        db.execute("INSERT INTO learning_courses(id,media_entry_id,created_at,updated_at) VALUES('course','video','now','now')", []).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_effective_memberships WHERE media_entry_id='video' AND workspace='learning'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        db.execute("DELETE FROM learning_courses WHERE id='course'", []).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_effective_memberships WHERE media_entry_id='video' AND workspace='learning'", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        db.execute("INSERT INTO media_workspace_memberships(media_entry_id,workspace) VALUES('video','learning')", []).unwrap();
        db.execute("INSERT INTO learning_courses(id,media_entry_id,created_at,updated_at) VALUES('course','video','now','now')", []).unwrap();
        db.execute("DELETE FROM learning_courses WHERE id='course'", []).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_effective_memberships WHERE media_entry_id='video' AND workspace='learning'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        Ok(())
    }).unwrap();
}

#[test]
fn new_media_acquires_materials_membership_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    state.with_library(|db, _| {
        db.execute_batch("BEGIN; INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('m1','video','New','file','now','now');").unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id='m1' AND workspace='materials'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        db.execute_batch("ROLLBACK").unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id='m1'", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        Ok(())
    }).unwrap();
}

#[test]
fn learning_membership_upgrade_preserves_existing_visibility_and_course_progress() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    state.with_library(|db, _| {
        db.execute_batch("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('legacy-video','video','Legacy','legacy-file','now','now'); INSERT INTO learning_courses(id,media_entry_id,position_ms,created_at,updated_at) VALUES('legacy-course','legacy-video',1234,'now','now'); DROP TABLE IF EXISTS media_workspace_memberships; UPDATE schema_meta SET value='39' WHERE key='schema_version';").unwrap();
        super::migrate(db)?;
        let memberships: String = db.query_row("SELECT group_concat(workspace, ',') FROM (SELECT workspace FROM media_workspace_memberships WHERE media_entry_id='legacy-video' ORDER BY workspace)", [], |row| row.get(0)).unwrap();
        assert_eq!(memberships, "learning,materials");
        assert_eq!(db.query_row("SELECT position_ms FROM learning_courses WHERE id='legacy-course'", [], |row| row.get::<_, i64>(0)).unwrap(), 1234);
        super::migrate(db)?;
        assert_eq!(db.query_row("SELECT count(*) FROM media_workspace_memberships WHERE media_entry_id='legacy-video'", [], |row| row.get::<_, i64>(0)).unwrap(), 2);
        Ok(())
    }).unwrap();
}

#[test]
fn learning_membership_upgrade_rolls_back_when_version_update_fails() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    state.with_library(|db, _| {
        db.execute_batch("DROP TABLE media_workspace_memberships; UPDATE schema_meta SET value='39' WHERE key='schema_version'; CREATE TRIGGER reject_v40 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='40' BEGIN SELECT RAISE(ABORT,'rollback'); END;").unwrap();
        assert!(super::migrate(db).is_err());
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |row| row.get::<_, String>(0)).unwrap(), "39");
        assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='media_workspace_memberships'", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        Ok(())
    }).unwrap();
}

#[test]
fn learning_import_course_revert_capacity_upgrade_is_atomic_and_preserves_reports() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let batch = create_course_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"a".repeat(64),
    )
    .unwrap();
    state.with_library(|db,_|{
        db.execute_batch("DROP INDEX learning_course_import_entries_target; ALTER TABLE learning_import_reverts RENAME TO test_v39; CREATE TABLE learning_import_reverts(batch_id TEXT PRIMARY KEY REFERENCES learning_import_batches(id) ON DELETE CASCADE,original_request_sha256 TEXT NOT NULL CHECK(length(original_request_sha256)=64),reverted_at TEXT NOT NULL,report_json TEXT NOT NULL CHECK(length(CAST(report_json AS BLOB))<=65536)); INSERT INTO learning_import_reverts SELECT * FROM test_v39; DROP TABLE test_v39;").unwrap();
        let report=serde_json::json!({"batchId":batch,"reverted":[],"kept":[],"archivesKept":true}).to_string();
        db.execute("INSERT INTO learning_import_reverts(batch_id,original_request_sha256,reverted_at,report_json) VALUES(?1,?2,'now',?3)",rusqlite::params![batch,"a".repeat(64),report]).unwrap();
        db.execute_batch("UPDATE schema_meta SET value='38' WHERE key='schema_version'; CREATE TRIGGER reject_v39 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='39' BEGIN SELECT RAISE(ABORT,'rollback'); END;").unwrap();
        assert!(super::migrate(db).is_err());
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'",[],|r|r.get::<_,String>(0)).unwrap(),"38");
        assert_eq!(db.query_row("SELECT report_json FROM learning_import_reverts WHERE batch_id=?1",[&batch],|r|r.get::<_,String>(0)).unwrap(),report);
        db.execute_batch("DROP TRIGGER reject_v39").unwrap();super::migrate(db)?;super::migrate(db)?;
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'",[],|r|r.get::<_,String>(0)).unwrap(),"40");
        assert_eq!(db.query_row("SELECT report_json FROM learning_import_reverts WHERE batch_id=?1",[&batch],|r|r.get::<_,String>(0)).unwrap(),report);
        let large=serde_json::json!({"data":"x".repeat(70000)}).to_string();
        db.execute("UPDATE learning_import_reverts SET report_json=?1 WHERE batch_id=?2",rusqlite::params![large,batch]).unwrap();
        let plan:String=db.query_row("EXPLAIN QUERY PLAN SELECT 1 FROM learning_course_import_entries WHERE entry_kind=?1 AND target_id=?2 AND id<>?3 AND status='committed'",rusqlite::params!["course",uuid::Uuid::new_v4().to_string(),uuid::Uuid::new_v4().to_string()],|row|row.get(3)).unwrap();
        assert!(plan.contains("learning_course_import_entries_target"),"{plan}");
        Ok(())
    }).unwrap();
}

#[test]
fn learning_import_course_batch_list_isolates_invalid_source_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let good = source_profiles::create(&state, &scope).unwrap();
    let bad = source_profiles::create(&state, &scope).unwrap();
    let good_batch = create_course_batch(
        &state,
        &scope,
        &good.id,
        &good.access_epoch,
        &"a".repeat(64),
    )
    .unwrap();
    let bad_batch =
        create_course_batch(&state, &scope, &bad.id, &bad.access_epoch, &"b".repeat(64)).unwrap();
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_source_profiles SET access_epoch=?1 WHERE id=?2",
                rusqlite::params!["x".repeat(36), bad.id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    let rows = super::course_batch::list_course_batches(&state, &library).unwrap();
    let valid = rows.iter().find(|row| row.id == good_batch).unwrap();
    let damaged = rows.iter().find(|row| row.id == bad_batch).unwrap();
    let valid_json = serde_json::to_value(valid).unwrap();
    let damaged_json = serde_json::to_value(damaged).unwrap();
    assert_eq!(valid_json["currentSourceEpoch"], good.access_epoch);
    assert!(damaged_json["currentSourceEpoch"].is_null());
}

#[test]
fn learning_import_course_batch_list_has_a_bounded_first_page() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    state.with_library(|db, _| {
        for index in 0..205u128 {
            db.execute("INSERT INTO learning_import_batches(id,library_id,root_hash,source_profile_id,source_epoch,request_sha256,status,created_at,batch_kind) VALUES(?1,?2,?3,?4,?5,?6,'pending','2026-01-01T00:00:00Z','courses')", rusqlite::params![uuid::Uuid::from_u128(index + 1).to_string(),library,scope.root_hash(),profile.id,profile.access_epoch,format!("{index:064x}")]).unwrap();
        }
        Ok(())
    }).unwrap();
    let rows = super::course_batch::list_course_batches(&state, &library).unwrap();
    assert_eq!(rows.len(), 101);
    let cursor = |row: &super::course_batch::CourseBatchSummary| {
        serde_json::from_value::<super::course_batch::CourseBatchCursor>(
            serde_json::json!({"createdAt":row.created_at,"id":row.id}),
        )
        .unwrap()
    };
    let second_cursor = cursor(&rows[99]);
    state.with_library(|db, _| {
        db.execute("INSERT INTO learning_import_batches(id,library_id,root_hash,source_profile_id,source_epoch,request_sha256,status,created_at,batch_kind) VALUES(?1,?2,?3,?4,?5,?6,'pending','2026-02-01T00:00:00Z','courses')",rusqlite::params![uuid::Uuid::from_u128(999).to_string(),library,scope.root_hash(),profile.id,profile.access_epoch,"f".repeat(64)]).unwrap();
        Ok(())
    }).unwrap();
    let second =
        super::course_batch::list_course_batches_page(&state, &library, Some(&second_cursor))
            .unwrap();
    assert_eq!(second.len(), 101);
    let third =
        super::course_batch::list_course_batches_page(&state, &library, Some(&cursor(&second[99])))
            .unwrap();
    assert_eq!(third.len(), 5);
    let ids = rows[..100]
        .iter()
        .chain(second[..100].iter())
        .chain(third.iter())
        .map(|row| row.id.clone())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 205);
    assert!(!ids.contains(&uuid::Uuid::from_u128(999).to_string()));
    let invalid = serde_json::from_value::<super::course_batch::CourseBatchCursor>(
        serde_json::json!({"createdAt":"2026-01-01","id":"invalid"}),
    )
    .unwrap();
    assert!(
        super::course_batch::list_course_batches_page(&state, &library, Some(&invalid)).is_err()
    );
    assert!(super::course_batch::list_course_batches_page(
        &state,
        &uuid::Uuid::new_v4().to_string(),
        Some(&second_cursor)
    )
    .is_err());
}

#[test]
fn learning_import_course_batch_entries_are_scoped_and_paginated() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let course = create_course_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"a".repeat(64),
    )
    .unwrap();
    let note = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"b".repeat(64),
    )
    .unwrap();
    state.with_library(|db, _| {
        for n in 0..101 {
            db.execute("INSERT INTO learning_course_import_entries(id,batch_id,entry_kind,source_id,status,conflict_code,receipt_json,created_at) VALUES(?1,?2,'course',?3,'conflict','unresolved','{}','now')", rusqlite::params![uuid::Uuid::new_v4().to_string(),course,format!("item-{n:03}")]).unwrap();
        }
        Ok(())
    }).unwrap();
    let first = super::course_batch::list_course_entries(&state, &library, &course, 0).unwrap();
    assert_eq!(first.items.len(), 100);
    assert!(first.has_more);
    assert!(first.revert_report.is_none());
    assert!(!first.revert_damaged);
    assert_eq!(first.items[0].source_id, "item-000");
    assert_eq!(first.items[0].conflict_code.as_deref(), Some("unresolved"));
    let last = super::course_batch::list_course_entries(&state, &library, &course, 100).unwrap();
    assert_eq!(last.items.len(), 1);
    assert!(!last.has_more);
    assert_eq!(last.items[0].source_id, "item-100");
    assert!(super::course_batch::list_course_entries(&state, &library, &note.id, 0).is_err());
    assert!(super::course_batch::list_course_entries(
        &state,
        &library,
        &uuid::Uuid::new_v4().to_string(),
        0
    )
    .is_err());
    assert!(super::course_batch::list_course_entries(
        &state,
        &uuid::Uuid::new_v4().to_string(),
        &course,
        0
    )
    .is_err());
}

#[test]
fn learning_import_course_batch_list_is_scoped_and_excludes_notes() {
    use super::course_batch::list_course_batches;
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let course = create_course_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"a".repeat(64),
    )
    .unwrap();
    let note = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"b".repeat(64),
    )
    .unwrap();
    let rows = list_course_batches(&state, &library).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, course);
    assert_eq!(rows[0].status, "pending");
    assert!(!rows[0].staged);
    assert_eq!((rows[0].committed, rows[0].conflicts), (0, 0));
    assert!(!rows[0].reverted);
    assert_eq!(
        rows[0].current_source_epoch.as_deref(),
        Some(profile.access_epoch.as_str())
    );
    assert!(rows.iter().all(|b| b.id != note.id));
    assert!(list_course_batches(&state, &uuid::Uuid::new_v4().to_string()).is_err());
    state.with_library(|db,_| {
        db.execute("INSERT INTO learning_course_import_stage(batch_id,source_fingerprint,target_fingerprint,payload_sha256,payload_json,staged_at) VALUES(?1,?2,?2,?2,'{}','now')", rusqlite::params![course,"a".repeat(64)]).unwrap();
        db.execute("INSERT INTO learning_course_import_entries(id,batch_id,entry_kind,source_id,status,target_id,receipt_json,created_at) VALUES(?1,?2,'course','one','committed',?3,'{}','now')", rusqlite::params![uuid::Uuid::new_v4().to_string(),course,uuid::Uuid::new_v4().to_string()]).unwrap();
        db.execute("INSERT INTO learning_course_import_entries(id,batch_id,entry_kind,source_id,status,conflict_code,receipt_json,created_at) VALUES(?1,?2,'series','two','conflict','CHANGED','{}','now')", rusqlite::params![uuid::Uuid::new_v4().to_string(),course]).unwrap();
        let report = serde_json::json!({"batchId":course,"reverted":[],"kept":[],"archivesKept":true}).to_string();
        db.execute("INSERT INTO learning_import_reverts(batch_id,original_request_sha256,reverted_at,report_json) VALUES(?1,?2,'now',?3)",rusqlite::params![course,"a".repeat(64),report]).unwrap();
        Ok(())
    }).unwrap();
    let rows = list_course_batches(&state, &library).unwrap();
    assert!(rows[0].staged);
    assert_eq!((rows[0].committed, rows[0].conflicts), (1, 1));
    assert!(rows[0].reverted);
    assert!(!rows[0].revert_damaged);
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_import_reverts SET report_json='{}' WHERE batch_id=?1",
                [&course],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    let rows = list_course_batches(&state, &library).unwrap();
    assert!(!rows[0].reverted);
    assert!(rows[0].revert_damaged);
    state.with_library(|db,_| {
        let bad = serde_json::json!({"batchId":course,"reverted":[null],"kept":[],"archivesKept":true}).to_string();
        db.execute("UPDATE learning_import_reverts SET report_json=?1 WHERE batch_id=?2",rusqlite::params![bad,course]).unwrap();
        Ok(())
    }).unwrap();
    assert!(list_course_batches(&state, &library).unwrap()[0].revert_damaged);
    let details = super::course_batch::list_course_entries(&state, &library, &course, 0).unwrap();
    assert!(details.revert_damaged);
    assert!(details.revert_report.is_none());
}

const NOTE_OP: &str = "课程迁入批次不能使用笔记操作";

fn note_input(path: &str, bytes: &[u8]) -> import_ops::NewImportItem {
    import_ops::NewImportItem {
        vault: "vault".into(),
        relative_path: path.into(),
        source_sha256: format!("{:x}", Sha256::digest(bytes)),
        title: "note".into(),
        document: serde_json::json!({"type":"doc","content":[]}),
        expected_target_id: None,
        expected_target_revision: None,
        expected_target_sha256: None,
    }
}

fn batch_field(state: &crate::state::AppState, id: &str, column: &str) -> String {
    let sql = match column {
        "batch_kind" => "SELECT batch_kind FROM learning_import_batches WHERE id=?1",
        "status" => "SELECT status FROM learning_import_batches WHERE id=?1",
        _ => panic!("unknown batch column"),
    };
    state
        .with_library(|db, _| {
            db.query_row(sql, [id], |r| r.get(0))
                .map_err(|e| e.to_string())
        })
        .unwrap()
}

#[test]
fn learning_import_course_batch_isolated_from_markdown_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let course_request = "a".repeat(64);
    let course_id = create_course_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &course_request,
    )
    .unwrap();
    assert_eq!(
        create_course_batch(
            &state,
            &scope,
            &profile.id,
            &profile.access_epoch,
            &course_request
        )
        .unwrap(),
        course_id
    );
    assert_eq!(batch_field(&state, &course_id, "batch_kind"), "courses");
    assert_eq!(batch_field(&state, &course_id, "status"), "pending");
    let bytes = b"hello note";
    assert_eq!(
        import_ops::commit_batch(&state, &scope, &course_id).unwrap_err(),
        NOTE_OP
    );
    assert_eq!(
        import_ops::add_item(&state, &scope, &course_id, note_input("note.md", bytes)).unwrap_err(),
        NOTE_OP
    );
    assert_eq!(
        import_ops::stage_item_bytes(
            &state,
            &scope,
            &course_id,
            &uuid::Uuid::new_v4().to_string(),
            bytes
        )
        .unwrap_err(),
        NOTE_OP
    );
    assert_eq!(
        import_ops::cancel_batch(&state, &scope, &course_id).unwrap_err(),
        NOTE_OP
    );
    assert_eq!(
        import_ops::revert_batch(&state, &scope, &profile.access_epoch, &course_id).unwrap_err(),
        NOTE_OP
    );
    assert_eq!(
        import_ops::list_items(&state, &scope, &course_id).unwrap_err(),
        NOTE_OP
    );
    assert_eq!(
        import_ops::create_batch(
            &state,
            &scope,
            &profile.id,
            &profile.access_epoch,
            &course_request
        )
        .unwrap_err(),
        NOTE_OP
    );
    assert!(import_ops::list_batches(&state, &scope)
        .unwrap()
        .iter()
        .all(|batch| batch.id != course_id));

    let note_request = "b".repeat(64);
    let note = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &note_request,
    )
    .unwrap();
    assert_eq!(batch_field(&state, &note.id, "batch_kind"), "notes");
    assert_eq!(
        create_course_batch(
            &state,
            &scope,
            &profile.id,
            &profile.access_epoch,
            &note_request
        )
        .unwrap_err(),
        "迁入请求已被其他批次使用"
    );
    let staged =
        import_ops::add_item(&state, &scope, &note.id, note_input("note.md", bytes)).unwrap();
    import_ops::stage_item_bytes(&state, &scope, &note.id, &staged.id, bytes).unwrap();
    let listed = import_ops::list_batches(&state, &scope).unwrap();
    assert!(listed.iter().any(|batch| batch.id == note.id));
    assert!(listed.iter().all(|batch| batch.id != course_id));
    assert_eq!(
        import_ops::cancel_batch(&state, &scope, &note.id)
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(batch_field(&state, &course_id, "status"), "pending");
    assert_eq!(batch_field(&state, &course_id, "batch_kind"), "courses");
    let reused = create_course_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &note_request,
    )
    .unwrap();
    assert_eq!(batch_field(&state, &reused, "batch_kind"), "courses");

    let commit_request = "d".repeat(64);
    let commit_bytes = b"committed note";
    let live = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &commit_request,
    )
    .unwrap();
    assert_eq!(
        create_course_batch(
            &state,
            &scope,
            &profile.id,
            &profile.access_epoch,
            &commit_request
        )
        .unwrap_err(),
        "迁入请求已被其他批次使用"
    );
    let live_item = import_ops::add_item(
        &state,
        &scope,
        &live.id,
        note_input("other.md", commit_bytes),
    )
    .unwrap();
    import_ops::stage_item_bytes(&state, &scope, &live.id, &live_item.id, commit_bytes).unwrap();
    assert_eq!(
        import_ops::commit_batch(&state, &scope, &live.id)
            .unwrap()
            .status,
        "committed"
    );
    assert_eq!(batch_field(&state, &live.id, "batch_kind"), "notes");
    assert_eq!(batch_field(&state, &course_id, "status"), "pending");
    import_ops::revert_batch(&state, &scope, &profile.access_epoch, &live.id).unwrap();
    assert_eq!(batch_field(&state, &course_id, "batch_kind"), "courses");
    assert_eq!(batch_field(&state, &course_id, "status"), "pending");
    assert_eq!(
        import_ops::commit_batch(&state, &scope, &course_id).unwrap_err(),
        NOTE_OP
    );
    let freed = create_course_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &commit_request,
    )
    .unwrap();
    assert_eq!(batch_field(&state, &freed, "batch_kind"), "courses");
    assert!(create_course_batch(&state, &scope, &profile.id, "wrong", &"c".repeat(64)).is_err());
}

#[test]
fn learning_import_course_batch_schema_upgrade_rolls_back_and_preserves_note_batch() {
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
    state.with_library(|db,_| {
        db.execute_batch("ALTER TABLE learning_import_batches DROP COLUMN batch_kind; UPDATE schema_meta SET value='35' WHERE key='schema_version'; CREATE TRIGGER reject_v36 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='36' BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
        assert!(super::migrate(db).is_err());
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'",[],|r| r.get::<_,String>(0)).unwrap(),"35");
        assert_eq!(db.query_row("SELECT count(*) FROM pragma_table_info('learning_import_batches') WHERE name='batch_kind'",[],|r| r.get::<_,i64>(0)).unwrap(),0);
        db.execute_batch("DROP TRIGGER reject_v36").unwrap();
        super::migrate(db)?;
        super::migrate(db)?;
        assert_eq!(db.query_row("SELECT batch_kind FROM learning_import_batches WHERE id=?1",[&note.id],|r| r.get::<_,String>(0)).unwrap(),"notes");
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'",[],|r| r.get::<_,String>(0)).unwrap(),"40");
        Ok(())
    }).unwrap();
    assert_eq!(
        import_ops::cancel_batch(&state, &scope, &note.id)
            .unwrap()
            .status,
        "cancelled"
    );
}

#[test]
fn learning_import_course_batch_backup_keeps_kind_and_invalidates_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let request = "c".repeat(64);
    let id =
        create_course_batch(&state, &scope, &profile.id, &profile.access_epoch, &request).unwrap();
    let archive = temp.path().join("backup.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &archive))
        .unwrap();
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &archive))
        .unwrap();
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let restored = source_profiles::get(&state, &scope, &profile.id).unwrap();
    assert_ne!(restored.access_epoch, profile.access_epoch);
    assert_eq!(
        super::course_batch::list_course_batches(&state, scope.library_id()).unwrap()[0]
            .current_source_epoch
            .as_deref(),
        Some(restored.access_epoch.as_str())
    );
    assert!(
        create_course_batch(&state, &scope, &profile.id, &profile.access_epoch, &request).is_err()
    );
    assert_eq!(
        import_ops::cancel_batch(&state, &scope, &id).unwrap_err(),
        NOTE_OP
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT batch_kind FROM learning_import_batches WHERE id=?1",
                    [&id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "courses"
            );
            Ok(())
        })
        .unwrap();
}
