use super::{course_batch::create_course_batch, import_ops, source_profiles};
use crate::state::test_state;

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
    assert!(import_ops::commit_batch(&state, &scope, &course_id)
        .unwrap_err()
        .contains("课程"));
    let note_input = import_ops::NewImportItem {
        vault: "vault".into(),
        relative_path: "note.md".into(),
        source_sha256: "a".repeat(64),
        title: "note".into(),
        document: serde_json::json!({"type":"doc","content":[]}),
        expected_target_id: None,
        expected_target_revision: None,
        expected_target_sha256: None,
    };
    assert!(import_ops::add_item(&state, &scope, &course_id, note_input)
        .unwrap_err()
        .contains("课程"));
    assert!(import_ops::stage_item_bytes(
        &state,
        &scope,
        &course_id,
        &uuid::Uuid::new_v4().to_string(),
        b"x"
    )
    .unwrap_err()
    .contains("课程"));
    assert!(import_ops::cancel_batch(&state, &scope, &course_id)
        .unwrap_err()
        .contains("课程"));
    assert!(
        import_ops::revert_batch(&state, &scope, &profile.access_epoch, &course_id)
            .unwrap_err()
            .contains("课程")
    );
    assert!(import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &course_request
    )
    .is_err());
    let note = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"b".repeat(64),
    )
    .unwrap();
    assert_eq!(
        import_ops::cancel_batch(&state, &scope, &note.id)
            .unwrap()
            .status,
        "cancelled"
    );
    assert!(create_course_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &"b".repeat(64)
    )
    .is_ok());
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
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'",[],|r| r.get::<_,String>(0)).unwrap(),"36");
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
    assert!(
        create_course_batch(&state, &scope, &profile.id, &profile.access_epoch, &request).is_err()
    );
    assert!(import_ops::cancel_batch(&state, &scope, &id)
        .unwrap_err()
        .contains("课程"));
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
