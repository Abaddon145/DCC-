use super::{
    note_archive::{archive_bytes, read_archive, ArchiveSource},
    source_profiles,
};
use crate::{
    knowledge::{self, NoteInput},
    models::StorageChangeRequest,
    state::{test_state, AppState},
};
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

fn scope(state: &AppState) -> source_profiles::LibraryScope {
    source_profiles::scope(state, &state.active_library_id().unwrap()).unwrap()
}

#[test]
fn learning_source_profiles_migrate_v30_once_and_preserve_existing_notes_and_archives() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    state.with_library_mut(|db, base| {
        db.execute_batch("DROP TABLE IF EXISTS learning_source_profiles; UPDATE schema_meta SET value='30' WHERE key='schema_version';").unwrap();
        let document = json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"原有正文"}]}]});
        let note = knowledge::save(db, NoteInput { id:None, expected_revision:None, notebook_id:None, title:"原笔记".into(), document:document.clone(), tags:vec![], favorite:false, journal_date:None })?;
        let raw = b"\xef\xbb\xbf# original\r\n";
        let sha = format!("{:x}", Sha256::digest(raw));
        let old_profile = uuid::Uuid::new_v4().to_string();
        let archived = archive_bytes(db, base, &ArchiveSource { profile_id:&old_profile, source_key:"old/vault.md", version_key:"v1" }, raw, &sha)?;
        super::migrate(db)?;
        super::migrate(db)?;
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |r| r.get::<_,String>(0)).unwrap(), "31");
        assert_eq!(knowledge::get(db, &note.id)?.document, document);
        assert_eq!(read_archive(db, base, &archived.id)?, raw);
        assert_eq!(db.query_row("SELECT count(*) FROM learning_source_profiles", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
        let mut stmt = db.prepare("PRAGMA table_info(learning_source_profiles)").unwrap();
        let columns: Vec<String> = stmt.query_map([], |r| r.get(1)).unwrap().map(Result::unwrap).collect();
        assert_eq!(columns, ["id", "application", "access_epoch", "created_at"]);
        Ok(())
    }).unwrap();
}

#[test]
fn learning_source_profiles_failed_upgrade_rolls_back_schema_and_version() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    state.with_library_mut(|db, _| {
        db.execute_batch("DROP TABLE IF EXISTS learning_source_profiles; UPDATE schema_meta SET value='30' WHERE key='schema_version'; CREATE TRIGGER reject_v31 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='31' BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
        assert!(super::migrate(db).is_err());
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |r| r.get::<_,String>(0)).unwrap(), "30");
        assert_eq!(db.query_row("SELECT count(*) FROM sqlite_schema WHERE name='learning_source_profiles'", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
        db.execute_batch("DROP TRIGGER reject_v31;").unwrap();
        super::migrate(db)?;
        Ok(())
    }).unwrap();
}

#[test]
fn learning_source_profiles_scope_root_and_switch_without_authorizing_a_source() {
    let temp = tempfile::tempdir().unwrap();
    let first_path = temp.path().join("first");
    let first = test_state(&first_path);
    let first_scope = scope(&first);
    let profile = source_profiles::create(&first, &first_scope).unwrap();
    assert_ne!(profile.id, profile.access_epoch);
    assert_eq!(
        uuid::Uuid::parse_str(&profile.id).unwrap().to_string(),
        profile.id
    );
    assert_eq!(
        uuid::Uuid::parse_str(&profile.access_epoch)
            .unwrap()
            .to_string(),
        profile.access_epoch
    );
    assert_eq!(profile.application, "yingliu");
    assert!(source_profiles::get(&first, &first_scope, "not-a-uuid").is_err());
    assert!(source_profiles::get(&first, &first_scope, &profile.id.to_uppercase()).is_err());
    assert!(
        source_profiles::get(&first, &first_scope, "00000000-0000-0000-0000-000000000000").is_err()
    );
    assert!(source_profiles::get(&first, &first_scope, &uuid::Uuid::new_v4().to_string()).is_err());

    let copied_path = temp.path().join("copied");
    fs::create_dir_all(&copied_path).unwrap();
    fs::copy(
        first_path.join("library.json"),
        copied_path.join("library.json"),
    )
    .unwrap();
    let mut copy = Connection::open(copied_path.join("library.db")).unwrap();
    first
        .with_library(|db, _| {
            rusqlite::backup::Backup::new(db, &mut copy)
                .unwrap()
                .run_to_completion(64, std::time::Duration::from_millis(1), None)
                .unwrap();
            Ok(())
        })
        .unwrap();
    drop(copy);
    let copied = test_state(&copied_path);
    assert_eq!(
        copied.active_library_id().unwrap(),
        first.active_library_id().unwrap()
    );
    assert!(source_profiles::create(&copied, &first_scope).is_err());
    assert!(source_profiles::get(&copied, &first_scope, &profile.id).is_err());
    assert_eq!(
        source_profiles::get(&copied, &scope(&copied), &profile.id)
            .unwrap()
            .id,
        profile.id
    );

    first
        .change_library(StorageChangeRequest {
            mode: "create".into(),
            path: temp
                .path()
                .join("new-library")
                .to_string_lossy()
                .into_owned(),
            name: Some("新库".into()),
        })
        .unwrap();
    assert!(source_profiles::get(&first, &first_scope, &profile.id).is_err());
}

#[test]
fn learning_source_profiles_reject_corrupt_records_and_missing_table() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library_scope = scope(&state);
    let profile = source_profiles::create(&state, &library_scope).unwrap();
    state
        .with_library_mut(|db, _| {
            db.execute(
                "UPDATE learning_source_profiles SET access_epoch=?1 WHERE id=?2",
                [&"x".repeat(36), &profile.id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(source_profiles::get(&state, &library_scope, &profile.id).is_err());
    state
        .with_library_mut(|db, _| {
            db.execute(
                "UPDATE learning_source_profiles SET access_epoch=?1, created_at='invalid' WHERE id=?2",
                [&profile.access_epoch, &profile.id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(source_profiles::get(&state, &library_scope, &profile.id).is_err());
    state
        .with_library_mut(|db, _| {
            db.execute_batch("DROP TABLE learning_source_profiles;")
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(source_profiles::get(&state, &library_scope, &profile.id).is_err());
    assert!(source_profiles::create(&state, &library_scope).is_err());
}

#[test]
fn learning_source_profiles_backup_restore_keeps_identity_but_rotates_access_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library_scope = scope(&state);
    let profile = source_profiles::create(&state, &library_scope).unwrap();
    let backup = temp.path().join("library.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    let restored = source_profiles::get(&state, &library_scope, &profile.id).unwrap();
    assert_eq!(restored.id, profile.id);
    assert_ne!(restored.access_epoch, profile.access_epoch);
}

#[test]
fn learning_source_profiles_interrupted_restore_rotates_access_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library_scope = scope(&state);
    let profile = source_profiles::create(&state, &library_scope).unwrap();
    state
        .with_active_mut(|active| {
            let _: (i64, i64, i64) = active
                .connection
                .as_ref()
                .unwrap()
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .unwrap();
            drop(active.connection.take());
            let old = active.base_dir.join(".restore-old-profile-test");
            fs::create_dir(&old).unwrap();
            fs::rename(&active.db_path, old.join("library.db")).unwrap();
            fs::write(&active.db_path, b"partial restore").unwrap();
            fs::write(
                active.base_dir.join(crate::backup::RESTORE_MARKER),
                serde_json::to_vec(
                    &json!({"rollback":".restore-old-profile-test","originals":["library.db"]}),
                )
                .unwrap(),
            )
            .unwrap();
            crate::backup::recover_interrupted_restore(active, |_| Ok(()))
        })
        .unwrap();
    let restored = source_profiles::get(&state, &library_scope, &profile.id).unwrap();
    assert_eq!(restored.id, profile.id);
    assert_ne!(restored.access_epoch, profile.access_epoch);
}
