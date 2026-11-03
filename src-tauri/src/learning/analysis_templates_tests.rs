use crate::state::test_state;

#[test]
fn custom_templates_survive_backup_restore() {
    let dir = tempfile::tempdir().unwrap();
    let source = test_state(&dir.path().join("source"));
    let target = test_state(&dir.path().join("target"));
    let source_id = source.active_library_id().unwrap();
    let saved =
        super::analysis_templates::save(&source, &source_id, None, None, "材质", "关注节点")
            .unwrap();
    let backup = dir.path().join("templates.dccassetlib");
    source
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    target
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    let target_id = target.active_library_id().unwrap();
    let list = super::analysis_templates::list(&target, &target_id).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, saved.id);
    assert_eq!(list[0].focus, "关注节点");
}

#[test]
fn custom_template_schema_upgrade_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(&dir.path().join("library"));
    state.with_library(|db,_| {
        db.execute_batch("DROP TABLE learning_analysis_templates; UPDATE schema_meta SET value='40' WHERE key='schema_version'; CREATE TRIGGER reject_v41 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='41' BEGIN SELECT RAISE(ABORT,'rollback'); END;").unwrap();
        assert!(super::migrate(db).is_err());
        assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE name='learning_analysis_templates'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'",[],|r|r.get::<_,String>(0)).unwrap(),"40");
        db.execute_batch("DROP TRIGGER reject_v41").unwrap();
        super::migrate(db)?;
        super::migrate(db)?;
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'",[],|r|r.get::<_,String>(0)).unwrap(),"41");
        Ok(())
    }).unwrap();
}

#[test]
fn custom_template_versions_prevent_stale_edits_and_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(&dir.path().join("library"));
    let library = state.active_library_id().unwrap();
    let first =
        super::analysis_templates::save(&state, &library, None, None, "材质拆解", "关注节点及参数")
            .unwrap();
    assert_eq!(first.version, 1);
    let second = super::analysis_templates::save(
        &state,
        &library,
        Some(&first.id),
        Some(1),
        "材质拆解",
        "关注节点及适用场景",
    )
    .unwrap();
    assert_eq!(second.version, 2);
    assert!(super::analysis_templates::save(
        &state,
        &library,
        Some(&first.id),
        Some(1),
        "旧编辑",
        "不能覆盖"
    )
    .is_err());
    let list = super::analysis_templates::list(&state, &library).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].focus, "关注节点及适用场景");
    assert!(super::analysis_templates::remove(&state, &library, &first.id, 1).is_err());
    super::analysis_templates::remove(&state, &library, &first.id, 2).unwrap();
    assert!(super::analysis_templates::list(&state, &library)
        .unwrap()
        .is_empty());
}
