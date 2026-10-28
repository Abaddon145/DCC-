use super::{note_origins, source_profiles};
use crate::{
    knowledge::{self, NoteInput},
    state::{test_state, AppState},
};
use serde_json::json;

fn profile(state: &AppState) -> source_profiles::SourceProfile {
    let scope = source_profiles::scope(state, &state.active_library_id().unwrap()).unwrap();
    source_profiles::create(state, &scope).unwrap()
}

fn input(title: &str) -> NoteInput {
    NoteInput {
        id: None,
        expected_revision: None,
        notebook_id: None,
        title: title.into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"正文"}]}]}),
        tags: vec![],
        favorite: false,
        journal_date: None,
    }
}

#[test]
fn learning_note_origin_keys_preserve_segments_and_validate_input() {
    let id = uuid::Uuid::new_v4().to_string();
    let a = note_origins::NoteOriginKey::new(&id, "素材/库", "笔记.md").unwrap();
    let b = note_origins::NoteOriginKey::new(&id, "素材", "库/笔记.md").unwrap();
    assert_ne!(a.encoded(), b.encoded());
    let long = "字".repeat(1364);
    let key = note_origins::NoteOriginKey::new(&id, &long, &format!("{long}.md")).unwrap();
    assert!(key.encoded().len() > 4096);
    assert_eq!(
        serde_json::from_str::<Vec<String>>(key.encoded()).unwrap()[2],
        long
    );
    for (bad_id, vault, path) in [
        ("bad", "vault", "note.md"),
        ("00000000-0000-0000-0000-000000000000", "vault", "note.md"),
        (id.as_str(), "", "note.md"),
        (id.as_str(), "vault\n", "note.md"),
        (id.as_str(), "vault", "../note.md"),
        (id.as_str(), "vault", "C:/note.md"),
    ] {
        assert!(note_origins::NoteOriginKey::new(bad_id, vault, path).is_err());
    }
    assert!(note_origins::NoteOriginKey::new(&id, &"a".repeat(4097), "note.md").is_err());
}

#[test]
fn learning_note_origins_check_library_scope_and_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let source = source_profiles::create(&state, &scope).unwrap();
    let key = note_origins::NoteOriginKey::new(&source.id, "vault", "note.md").unwrap();
    let sha = "a".repeat(64);
    assert_eq!(
        note_origins::inspect_scoped(&state, &scope, &key, &source.access_epoch, &sha).unwrap(),
        note_origins::NoteOriginState::Unmapped
    );
    assert!(note_origins::inspect_scoped(
        &state,
        &scope,
        &key,
        &uuid::Uuid::new_v4().to_string(),
        &sha
    )
    .is_err());
    state
        .change_library(crate::models::StorageChangeRequest {
            mode: "create".into(),
            path: temp.path().join("other").to_string_lossy().into_owned(),
            name: Some("另一库".into()),
        })
        .unwrap();
    assert!(
        note_origins::inspect_scoped(&state, &scope, &key, &source.access_epoch, &sha).is_err()
    );
    state
        .change_library(crate::models::StorageChangeRequest {
            mode: "open".into(),
            path: temp.path().join("library").to_string_lossy().into_owned(),
            name: None,
        })
        .unwrap();
    assert!(
        note_origins::inspect_scoped(&state, &scope, &key, &source.access_epoch, &sha).is_err()
    );
    let fresh = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    assert_eq!(
        note_origins::inspect_scoped(&state, &fresh, &key, &source.access_epoch, &sha).unwrap(),
        note_origins::NoteOriginState::Unmapped
    );
}

#[test]
fn learning_note_origins_store_long_keys_without_truncation() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let source = profile(&state);
    let long = "字".repeat(1364);
    let key = note_origins::NoteOriginKey::new(&source.id, &long, &format!("{long}.md")).unwrap();
    let sha = "a".repeat(64);
    state
        .with_library_mut(|db, _| {
            let note = knowledge::save(db, input("长来源"))?;
            let tx = db.transaction().unwrap();
            note_origins::record_note_in(&tx, &key, &note.id, &sha, &source.access_epoch)?;
            tx.commit().unwrap();
            let saved: String = db
                .query_row(
                    "SELECT source_key_json FROM learning_origin_map WHERE target_id=?1",
                    [&note.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(saved.len() > 4096);
            assert_eq!(saved, key.encoded());
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_note_origins_record_atomically_and_reject_silent_merge() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let a = profile(&state);
    let b = profile(&state);
    let key = note_origins::NoteOriginKey::new(&a.id, "vault", "note.md").unwrap();
    let other = note_origins::NoteOriginKey::new(&b.id, "vault", "note.md").unwrap();
    let unknown =
        note_origins::NoteOriginKey::new(&uuid::Uuid::new_v4().to_string(), "vault", "note.md")
            .unwrap();
    let sha = "a".repeat(64);
    state
        .with_library_mut(|db, _| {
            assert_eq!(
                note_origins::inspect(db, &key, &sha)?,
                note_origins::NoteOriginState::Unmapped
            );
            assert!(note_origins::inspect(db, &unknown, &sha).is_err());
            assert!(note_origins::record_note_in(
                db,
                &key,
                &uuid::Uuid::new_v4().to_string(),
                &sha,
                &a.access_epoch
            )
            .is_err());
            let rolled_back_id = {
                let tx = db.transaction().unwrap();
                let note = knowledge::save_in(&tx, input("临时"), false)?;
                note_origins::record_note_in(&tx, &key, &note.id, &sha, &a.access_epoch)?;
                note.id
            };
            assert!(knowledge::get(db, &rolled_back_id).is_err());
            assert_eq!(
                note_origins::inspect(db, &key, &sha)?,
                note_origins::NoteOriginState::Unmapped
            );
            let tx = db.transaction().unwrap();
            let note = knowledge::save_in(&tx, input("保留"), false)?;
            assert!(note_origins::record_note_in(
                &tx,
                &key,
                &uuid::Uuid::new_v4().to_string(),
                &sha,
                &a.access_epoch
            )
            .is_err());
            note_origins::record_note_in(&tx, &key, &note.id, &sha, &a.access_epoch)?;
            note_origins::record_note_in(&tx, &key, &note.id, &sha, &a.access_epoch)?;
            assert!(
                note_origins::record_note_in(&tx, &other, &note.id, &sha, &b.access_epoch).is_err()
            );
            let other_note = knowledge::save_in(&tx, input("保留"), false)?;
            note_origins::record_note_in(&tx, &other, &other_note.id, &sha, &b.access_epoch)?;
            tx.commit().unwrap();
            assert_eq!(
                note_origins::inspect(db, &key, &sha)?,
                note_origins::NoteOriginState::Unchanged(note.id)
            );
            assert_eq!(
                note_origins::inspect(db, &other, &sha)?,
                note_origins::NoteOriginState::Unchanged(other_note.id)
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_note_origins_detect_source_target_trash_and_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let source = profile(&state);
    let key = note_origins::NoteOriginKey::new(&source.id, "vault", "note.md").unwrap();
    let original = "a".repeat(64);
    let changed = "b".repeat(64);
    state
        .with_library_mut(|db, _| {
            let note = knowledge::save(db, input("原始"))?;
            let tx = db.transaction().unwrap();
            note_origins::record_note_in(&tx, &key, &note.id, &original, &source.access_epoch)?;
            tx.commit().unwrap();
            assert_eq!(
                note_origins::inspect(db, &key, &changed)?,
                note_origins::NoteOriginState::SourceChanged(note.id.clone())
            );
            knowledge::set_color(db, &note.id, Some("#123456"))?;
            assert_eq!(knowledge::get(db, &note.id)?.revision, note.revision);
            assert_eq!(
                note_origins::inspect(db, &key, &original)?,
                note_origins::NoteOriginState::TargetChanged(note.id.clone())
            );
            assert_eq!(
                note_origins::inspect(db, &key, &changed)?,
                note_origins::NoteOriginState::BothChanged(note.id.clone())
            );
            let mut update = input("原始");
            update.id = Some(note.id.clone());
            update.expected_revision = Some(note.revision);
            update.document = json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"用户新正文"}]}]});
            let edited = knowledge::save(db, update)?;
            assert_eq!(note_origins::inspect(db, &key, &original)?, note_origins::NoteOriginState::TargetChanged(note.id.clone()));
            let tx = db.transaction().unwrap();
            assert!(note_origins::record_note_in(&tx, &key, &note.id, &original, &source.access_epoch).is_err());
            tx.rollback().unwrap();
            knowledge::set_deleted(db, &note.id, edited.revision, true)?;
            assert_eq!(
                note_origins::inspect(db, &key, &original)?,
                note_origins::NoteOriginState::TargetUnavailable(note.id.clone())
            );
            db.execute("DELETE FROM knowledge_notes WHERE id=?1", [&note.id])
                .unwrap();
            assert_eq!(
                note_origins::inspect(db, &key, &original)?,
                note_origins::NoteOriginState::TargetUnavailable(note.id)
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_note_origins_upgrade_is_transactional() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let source = profile(&state);
    state.with_library_mut(|db, _| {
        let note = knowledge::save(db, input("旧笔记"))?;
        db.execute_batch("DROP TABLE learning_origin_map; UPDATE schema_meta SET value='31' WHERE key='schema_version'; CREATE TRIGGER reject_v32 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='32' BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
        assert!(super::migrate(db).is_err());
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |r| r.get::<_,String>(0)).unwrap(), "31");
        assert_eq!(db.query_row("SELECT count(*) FROM sqlite_schema WHERE name='learning_origin_map'", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
        assert_eq!(knowledge::get(db, &note.id)?.title, "旧笔记");
        db.execute_batch("DROP TRIGGER reject_v32;").unwrap();
        super::migrate(db)?;
        super::migrate(db)?;
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |r| r.get::<_,String>(0)).unwrap(), "32");
        assert_eq!(db.query_row("SELECT count(*) FROM learning_source_profiles WHERE id=?1", [&source.id], |r| r.get::<_,i64>(0)).unwrap(), 1);
        Ok(())
    }).unwrap();
}

#[test]
fn learning_note_origins_backup_restore_keeps_mapping_and_rotates_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let source = profile(&state);
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let key = note_origins::NoteOriginKey::new(&source.id, "vault", "note.md").unwrap();
    let sha = "a".repeat(64);
    let note_id = state
        .with_library_mut(|db, _| {
            let note = knowledge::save(db, input("学习资料"))?;
            let tx = db.transaction().unwrap();
            note_origins::record_note_in(&tx, &key, &note.id, &sha, &source.access_epoch)?;
            tx.commit().unwrap();
            Ok(note.id)
        })
        .unwrap();
    let backup = temp.path().join("library.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    let restored = source_profiles::get(&state, &scope, &source.id).unwrap();
    assert_ne!(restored.access_epoch, source.access_epoch);
    assert!(
        note_origins::inspect_scoped(&state, &scope, &key, &source.access_epoch, &sha).is_err()
    );
    assert_eq!(
        note_origins::inspect_scoped(&state, &scope, &key, &restored.access_epoch, &sha).unwrap(),
        note_origins::NoteOriginState::Unchanged(note_id.clone())
    );
    state
        .with_library_mut(|db, _| {
            let tx = db.transaction().unwrap();
            assert!(
                note_origins::record_note_in(&tx, &key, &note_id, &sha, &source.access_epoch)
                    .is_err()
            );
            tx.rollback().unwrap();
            Ok(())
        })
        .unwrap();
}
