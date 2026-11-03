use super::*;
use rusqlite::OpenFlags;
use tempfile::tempdir;

fn old_library(path: &Path) {
    ensure_library(path, "旧库", false).unwrap();
    let db = db::open_database(&path.join("library.db")).unwrap();
    db.execute_batch("DROP TABLE IF EXISTS learning_origin_map; DROP TABLE IF EXISTS learning_source_vault_bindings; DROP TABLE learning_source_profiles; UPDATE schema_meta SET value='30' WHERE key='schema_version';").unwrap();
    db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
}

fn current_version_record(path: &Path) -> AppSettings {
    let mut settings = AppSettings::default();
    settings.upgrade_backups.insert(
        upgrade_backup::library_key(path).unwrap(),
        UpgradeBackupRecord {
            last_success_app_version: env!("CARGO_PKG_VERSION").into(),
            ..Default::default()
        },
    );
    settings
}

fn schema(path: &Path) -> String {
    Connection::open_with_flags(path.join("library.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn upgrade_schema_guard_backs_up_old_schema_even_when_app_version_matches() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    old_library(&path);
    let mut settings = current_version_record(&path);
    let active = open_library(
        &path,
        &dir.path().join("settings.json"),
        &mut settings,
        &mut None,
    )
    .unwrap();
    assert_eq!(schema(&path), "41");
    let record = settings
        .upgrade_backups
        .get(&upgrade_backup::library_key(&path).unwrap())
        .unwrap();
    assert_eq!(record.snapshots.len(), 1);
    let folder = upgrade_backup::status(&path, 5, record)
        .unwrap()
        .backup_directory;
    let snapshot = Connection::open_with_flags(
        Path::new(&folder).join(&record.snapshots[0].name),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        snapshot
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "30"
    );
    drop(active);
}

#[test]
fn upgrade_schema_guard_stops_upgrade_when_backup_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    old_library(&path);
    let before = fs::read(path.join("library.db")).unwrap();
    fs::write(dir.path().join("升级前自动备份"), b"not a directory").unwrap();
    let mut settings = current_version_record(&path);
    assert!(open_library(
        &path,
        &dir.path().join("settings.json"),
        &mut settings,
        &mut None
    )
    .is_err());
    assert_eq!(schema(&path), "30");
    assert_eq!(fs::read(path.join("library.db")).unwrap(), before);
}

#[test]
fn upgrade_schema_guard_failed_switch_keeps_the_current_library() {
    let dir = tempdir().unwrap();
    let state = test_state(&dir.path().join("current"));
    let original_id = state.active_library_id().unwrap();
    let target_parent = dir.path().join("old-parent");
    fs::create_dir(&target_parent).unwrap();
    let target = target_parent.join("library");
    old_library(&target);
    fs::write(target_parent.join("升级前自动备份"), b"not a directory").unwrap();
    assert!(state
        .change_library(StorageChangeRequest {
            mode: "open".into(),
            path: target.to_string_lossy().into_owned(),
            name: None,
        })
        .is_err());
    assert_eq!(state.active_library_id().unwrap(), original_id);
    assert_eq!(schema(&target), "30");
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "41"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn upgrade_schema_guard_startup_does_not_replace_selected_library_after_backup_failure() {
    let dir = tempdir().unwrap();
    let local = dir.path().join("local");
    let config = local.join("DCCAssetLibrary");
    fs::create_dir_all(&config).unwrap();
    let old_parent = dir.path().join("old-parent");
    fs::create_dir(&old_parent).unwrap();
    let path = old_parent.join("library");
    old_library(&path);
    fs::write(old_parent.join("升级前自动备份"), b"not a directory").unwrap();
    let mut settings = current_version_record(&path);
    settings.active_library = path.to_string_lossy().into_owned();
    let settings_path = config.join(SETTINGS_FILE);
    write_settings(&settings_path, &settings).unwrap();
    assert!(AppState::initialize_in(&local).is_err());
    assert_eq!(
        read_settings(&settings_path).unwrap().active_library,
        path.to_string_lossy()
    );
    assert_eq!(schema(&path), "30");
    assert!(
        !config.join("library.db").exists(),
        "must not silently select a new default library"
    );
}

#[test]
fn upgrade_schema_guard_startup_preserves_selection_when_migration_fails_after_backup() {
    let dir = tempdir().unwrap();
    let local = dir.path().join("local");
    let config = local.join("DCCAssetLibrary");
    fs::create_dir_all(&config).unwrap();
    let path = dir.path().join("old-library");
    old_library(&path);
    let db = Connection::open(path.join("library.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_upgrade BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='31' BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
    drop(db);
    let mut settings = current_version_record(&path);
    settings.active_library = path.to_string_lossy().into_owned();
    let settings_path = config.join(SETTINGS_FILE);
    write_settings(&settings_path, &settings).unwrap();
    assert!(AppState::initialize_in(&local).is_err());
    let saved = read_settings(&settings_path).unwrap();
    assert_eq!(saved.active_library, path.to_string_lossy());
    assert_eq!(
        saved
            .upgrade_backups
            .get(&upgrade_backup::library_key(&path).unwrap())
            .unwrap()
            .snapshots
            .len(),
        1
    );
    assert_eq!(schema(&path), "30");
    assert!(!config.join("library.db").exists());
}

#[test]
fn upgrade_schema_guard_missing_selected_library_still_uses_existing_fallback() {
    let dir = tempdir().unwrap();
    let local = dir.path().join("local");
    let config = local.join("DCCAssetLibrary");
    fs::create_dir_all(&config).unwrap();
    let settings = AppSettings {
        active_library: dir.path().join("missing").to_string_lossy().into_owned(),
        ..Default::default()
    };
    write_settings(&config.join(SETTINGS_FILE), &settings).unwrap();
    let state = AppState::initialize_in(&local).unwrap();
    assert_eq!(state.active.lock().unwrap().base_dir, config);
    assert!(state
        .startup_warning
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .contains("已回到默认库"));
}

#[test]
fn upgrade_schema_guard_rejects_malformed_version_without_writing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    old_library(&path);
    let db = Connection::open(path.join("library.db")).unwrap();
    db.execute(
        "UPDATE schema_meta SET value='invalid' WHERE key='schema_version'",
        [],
    )
    .unwrap();
    db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
    drop(db);
    let before = fs::read(path.join("library.db")).unwrap();
    assert!(db::open_database(&path.join("library.db")).is_err());
    assert_eq!(schema(&path), "invalid");
    assert_eq!(fs::read(path.join("library.db")).unwrap(), before);
}

#[test]
fn upgrade_schema_guard_rejects_duplicate_versions_without_writing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("duplicate.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE schema_meta(key TEXT, value TEXT); INSERT INTO schema_meta VALUES('schema_version','30'),('schema_version','99'); CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES('kept');").unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    assert!(db::open_database(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn upgrade_schema_guard_failed_backup_preserves_live_wal() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    old_library(&path);
    let writer = Connection::open(path.join("library.db")).unwrap();
    writer
        .query_row("PRAGMA wal_autocheckpoint=0", [], |_| Ok(()))
        .unwrap();
    writer.execute_batch("CREATE TABLE schema_guard_pending(value TEXT); INSERT INTO schema_guard_pending VALUES('kept');").unwrap();
    let main_before = fs::read(path.join("library.db")).unwrap();
    let wal_before = fs::read(path.join("library.db-wal")).unwrap();
    fs::write(dir.path().join("升级前自动备份"), b"not a directory").unwrap();
    let mut settings = current_version_record(&path);
    assert!(open_library(
        &path,
        &dir.path().join("settings.json"),
        &mut settings,
        &mut None
    )
    .is_err());
    assert_eq!(
        writer
            .query_row("SELECT value FROM schema_guard_pending", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "kept"
    );
    assert_eq!(
        writer
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "30"
    );
    assert_eq!(fs::read(path.join("library.db")).unwrap(), main_before);
    assert_eq!(fs::read(path.join("library.db-wal")).unwrap(), wal_before);
}

#[test]
fn upgrade_schema_guard_stops_if_record_cannot_be_saved_and_keeps_old_snapshot() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    old_library(&path);
    let mut settings = AppSettings::default();
    settings.auto_backup_keep = 1;
    let mut record = UpgradeBackupRecord::default();
    let status =
        upgrade_backup::run_upgrade_backup(&path, env!("CARGO_PKG_VERSION"), 1, &mut record)
            .unwrap();
    let old_snapshot = Path::new(&status.backup_directory).join(&record.snapshots[0].name);
    settings
        .upgrade_backups
        .insert(upgrade_backup::library_key(&path).unwrap(), record);
    let settings_path = dir.path().join("unwritable-settings");
    fs::create_dir(&settings_path).unwrap();
    assert!(open_library(&path, &settings_path, &mut settings, &mut None).is_err());
    assert_eq!(schema(&path), "30");
    assert!(
        old_snapshot.is_file(),
        "existing recorded backup must survive until the new record is durable"
    );
}

#[test]
fn upgrade_schema_guard_snapshots_committed_wal_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    old_library(&path);
    let writer = Connection::open(path.join("library.db")).unwrap();
    writer
        .query_row("PRAGMA wal_autocheckpoint=0", [], |_| Ok(()))
        .unwrap();
    writer.execute_batch("CREATE TABLE schema_guard_wal(value TEXT); INSERT INTO schema_guard_wal VALUES('wal-only');").unwrap();
    assert!(path.join("library.db-wal").metadata().unwrap().len() > 0);
    let mut settings = current_version_record(&path);
    let active = open_library(
        &path,
        &dir.path().join("settings.json"),
        &mut settings,
        &mut None,
    )
    .unwrap();
    let record = settings
        .upgrade_backups
        .get(&upgrade_backup::library_key(&path).unwrap())
        .unwrap();
    let folder = upgrade_backup::status(&path, 5, record)
        .unwrap()
        .backup_directory;
    let snapshot = Connection::open_with_flags(
        Path::new(&folder).join(&record.snapshots[0].name),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT value FROM schema_guard_wal", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "wal-only"
    );
    assert_eq!(
        snapshot
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "30"
    );
    drop(active);
    drop(writer);
}

#[test]
fn upgrade_schema_guard_rejects_future_schema_before_writing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    ensure_library(&path, "较新库", false).unwrap();
    let db = db::open_database(&path.join("library.db")).unwrap();
    db.execute_batch("UPDATE schema_meta SET value='99' WHERE key='schema_version';")
        .unwrap();
    db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
    drop(db);
    let before = fs::read(path.join("library.db")).unwrap();
    assert!(db::open_database(&path.join("library.db")).is_err());
    assert_eq!(schema(&path), "99");
    assert_eq!(fs::read(path.join("library.db")).unwrap(), before);
}

#[test]
fn upgrade_schema_guard_sees_future_version_in_live_wal() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    ensure_library(&path, "较新库", false).unwrap();
    drop(db::open_database(&path.join("library.db")).unwrap());
    let writer = Connection::open(path.join("library.db")).unwrap();
    writer
        .query_row("PRAGMA wal_autocheckpoint=0", [], |_| Ok(()))
        .unwrap();
    writer
        .execute(
            "UPDATE schema_meta SET value='99' WHERE key='schema_version'",
            [],
        )
        .unwrap();
    let main_before = fs::read(path.join("library.db")).unwrap();
    let wal_before = fs::read(path.join("library.db-wal")).unwrap();
    assert!(db::open_database(&path.join("library.db")).is_err());
    assert_eq!(
        writer
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "99"
    );
    assert_eq!(fs::read(path.join("library.db")).unwrap(), main_before);
    assert_eq!(fs::read(path.join("library.db-wal")).unwrap(), wal_before);
}

#[test]
fn upgrade_schema_guard_interrupted_restore_can_retry_after_backup_failure() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library");
    let state = test_state(&path);
    state.with_library_mut(|db, _| {
        db.execute_batch("DROP TABLE IF EXISTS learning_origin_map; DROP TABLE IF EXISTS learning_source_vault_bindings; DROP TABLE learning_source_profiles; UPDATE schema_meta SET value='30' WHERE key='schema_version';").unwrap();
        db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).unwrap();
        Ok(())
    }).unwrap();
    drop(state.active.lock().unwrap().connection.take());
    let old = path.join(".restore-old-guard-test");
    fs::create_dir(&old).unwrap();
    fs::rename(path.join("library.db"), old.join("library.db")).unwrap();
    fs::write(path.join("library.db"), b"partial restore").unwrap();
    let marker = path.join(crate::backup::RESTORE_MARKER);
    fs::write(
        &marker,
        serde_json::to_vec(
            &serde_json::json!({"rollback":".restore-old-guard-test","originals":["library.db"]}),
        )
        .unwrap(),
    )
    .unwrap();
    let blocker = dir.path().join("升级前自动备份");
    fs::write(&blocker, b"not a directory").unwrap();
    assert!(state.recover_interrupted_restore().is_err());
    assert!(marker.is_file());
    assert_eq!(schema(&path), "30");
    assert!(state.active.lock().unwrap().connection.is_none());
    fs::remove_file(&blocker).unwrap();
    state.recover_interrupted_restore().unwrap();
    assert!(!marker.exists());
    assert_eq!(schema(&path), "41");
}
