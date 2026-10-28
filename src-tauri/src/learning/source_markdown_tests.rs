use super::{note_origins, source_markdown, source_profiles};
use crate::{
    knowledge::{self, NoteInput},
    state::test_state,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn learning_source_vault_create_concurrent_choices_reuse_one_profile() {
    use std::sync::{Arc, Barrier};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    let state = Arc::new(test_state(&temp.path().join("library")));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|threads| {
        let calls = (0..2)
            .map(|_| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                let scope = scope.clone();
                let root = root.clone();
                threads.spawn(move || {
                    barrier.wait();
                    source_markdown::create_vault_source(&state, &scope, "vault", &root).is_ok()
                })
            })
            .collect::<Vec<_>>();
        for call in calls {
            assert!(call.join().unwrap());
        }
    });
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_source_profiles", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
                1
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_source_vault_bindings",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_source_vault_create_rejects_ambiguous_existing_bindings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    for _ in 0..2 {
        let profile = source_profiles::create(&state, &scope).unwrap();
        let selected = source_markdown::select_vault(
            &state,
            &scope,
            &profile.id,
            &profile.access_epoch,
            "vault",
            &root,
        )
        .unwrap();
        selected.register_identity(&state).unwrap();
    }
    assert!(source_markdown::create_vault_source(&state, &scope, "vault", &root).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_source_profiles", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
                2
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_source_vault_create_is_atomic_and_reuses_same_source() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("note.md"), b"# UE").unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    state.with_library(|db, _| { db.execute_batch("CREATE TRIGGER fail_binding BEFORE INSERT ON learning_source_vault_bindings BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap(); Ok(()) }).unwrap();
    assert!(source_markdown::create_vault_source(&state, &scope, "vault", &root).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_source_profiles", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
                0
            );
            db.execute_batch("DROP TRIGGER fail_binding;").unwrap();
            Ok(())
        })
        .unwrap();
    let first = source_markdown::create_vault_source(&state, &scope, "vault", &root).unwrap();
    assert_eq!(
        first.inspect(&state, "note.md").unwrap().state,
        note_origins::NoteOriginState::Unmapped
    );
    let again = source_markdown::create_vault_source(&state, &scope, "vault", &root).unwrap();
    assert_eq!(
        again.inspect(&state, "note.md").unwrap().state,
        note_origins::NoteOriginState::Unmapped
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_source_profiles", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
                1
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_source_vault_bindings",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
    let backup = temp.path().join("source.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    assert!(first.inspect(&state, "note.md").is_err());
    let after_restore =
        source_markdown::create_vault_source(&state, &scope, "vault", &root).unwrap();
    assert_eq!(
        after_restore.inspect(&state, "note.md").unwrap().state,
        note_origins::NoteOriginState::Unmapped
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_source_profiles", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_source_vault_inspect_requires_registered_matching_identity() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let other = temp.path().join("other");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&other).unwrap();
    fs::write(first.join("note.md"), b"first").unwrap();
    fs::write(other.join("note.md"), b"other").unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let selected = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &first,
    )
    .unwrap();
    assert!(selected.inspect(&state, "note.md").is_err());
    selected.register_identity(&state).unwrap();
    assert_eq!(
        selected.inspect(&state, "note.md").unwrap().state,
        note_origins::NoteOriginState::Unmapped
    );
    let wrong = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &other,
    )
    .unwrap();
    assert!(wrong.inspect(&state, "note.md").is_err());
    state.with_library(|db, _| { db.execute("UPDATE learning_source_vault_bindings SET root_identity='corrupt' WHERE source_profile_id=?1", [&profile.id]).unwrap(); Ok(()) }).unwrap();
    assert!(selected.inspect(&state, "note.md").is_err());
    state
        .with_library(|db, _| {
            db.execute(
                "DELETE FROM learning_source_vault_bindings WHERE source_profile_id=?1",
                [&profile.id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(selected.inspect(&state, "note.md").is_err());
}

#[test]
fn learning_source_vault_identity_cannot_be_silently_rebound() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let other = temp.path().join("other");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&other).unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let selected = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &first,
    )
    .unwrap();
    selected.register_identity(&state).unwrap();
    selected.register_identity(&state).unwrap();
    let replacement = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &other,
    )
    .unwrap();
    assert!(replacement.register_identity(&state).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_source_vault_bindings",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_source_vault_identity_rejects_binding_old_note_mappings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    state
        .with_library_mut(|db, _| {
            let note = knowledge::save(
                db,
                NoteInput {
                    id: None,
                    expected_revision: None,
                    notebook_id: None,
                    title: "保留".into(),
                    document: json!({"type":"doc","content":[]}),
                    tags: vec![],
                    favorite: false,
                    journal_date: None,
                },
            )?;
            let key = note_origins::NoteOriginKey::new(&profile.id, "vault", "note.md")?;
            let tx = db.transaction().unwrap();
            note_origins::record_note_in(
                &tx,
                &key,
                &note.id,
                &"a".repeat(64),
                &profile.access_epoch,
            )?;
            tx.commit().unwrap();
            Ok(())
        })
        .unwrap();
    let selected = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &root,
    )
    .unwrap();
    assert!(selected.register_identity(&state).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_source_vault_bindings",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_origin_map", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_source_vault_identity_survives_backup_and_revokes_old_selection() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let selected = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &root,
    )
    .unwrap();
    selected.register_identity(&state).unwrap();
    let backup = temp.path().join("backup.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    assert!(selected.register_identity(&state).is_err());
    let restored = source_profiles::get(&state, &scope, &profile.id).unwrap();
    assert_ne!(restored.access_epoch, profile.access_epoch);
    let fresh = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &restored.access_epoch,
        "vault",
        &root,
    )
    .unwrap();
    fresh.register_identity(&state).unwrap();
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_source_vault_bindings",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_source_markdown_preview_converts_verified_bytes_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    let source = root.join("note.md");
    let raw = b"# UE5\n\nLearning **assets**\n";
    fs::write(&source, raw).unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let selection = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &root,
    )
    .unwrap();
    selection.register_identity(&state).unwrap();
    let result = selection.inspect(&state, "note.md").unwrap();
    assert_eq!(result.state, note_origins::NoteOriginState::Unmapped);
    assert_eq!(result.sha256, result.conversion.archive_hash);
    assert!(!result.conversion.pending);
    assert!(result
        .conversion
        .document
        .unwrap()
        .to_string()
        .contains("UE5"));
    assert_eq!(fs::read(&source).unwrap(), raw);
    fs::write(&source, b"![image](missing.png)").unwrap();
    assert!(
        selection
            .inspect(&state, "note.md")
            .unwrap()
            .conversion
            .pending
    );
    fs::write(&source, [0xff, 0xfe]).unwrap();
    let binary = selection.inspect(&state, "note.md").unwrap();
    assert!(binary.conversion.pending);
    assert!(binary.conversion.document.is_none());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM knowledge_notes", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_source_vault_selection_binds_root_scope_and_epoch() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("first.md"), b"first").unwrap();
    fs::write(root.join("second.md"), b"second").unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let selection = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &root,
    )
    .unwrap();
    selection.register_identity(&state).unwrap();
    assert!(fs::rename(&root, temp.path().join("replaced")).is_err());
    assert_eq!(
        selection.inspect(&state, "first.md").unwrap().state,
        note_origins::NoteOriginState::Unmapped
    );
    assert_eq!(
        selection.inspect(&state, "second.md").unwrap().state,
        note_origins::NoteOriginState::Unmapped
    );
    assert!(selection.inspect(&state, "FIRST.md").is_err());
    state
        .with_library(|db, _| source_profiles::invalidate_epochs(db))
        .unwrap();
    assert!(selection.inspect(&state, "first.md").is_err());
    drop(selection);
    fs::rename(&root, temp.path().join("replaced")).unwrap();
}

#[test]
fn learning_source_markdown_checks_real_bytes_and_target_without_writing_either() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    let source = root.join("note.md");
    fs::write(&source, b"original").unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let selection = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault-id",
        &root,
    )
    .unwrap();
    selection.register_identity(&state).unwrap();
    let inspect = || selection.inspect(&state, "note.md");
    assert_eq!(
        inspect().unwrap().state,
        note_origins::NoteOriginState::Unmapped
    );
    let note_id = state
        .with_library_mut(|db, _| {
            let note = knowledge::save(
                db,
                NoteInput {
                    id: None,
                    expected_revision: None,
                    notebook_id: None,
                    title: "笔记".into(),
                    document: json!({"type":"doc","content":[]}),
                    tags: vec![],
                    favorite: false,
                    journal_date: None,
                },
            )?;
            let key = note_origins::NoteOriginKey::new(&profile.id, "vault-id", "note.md")?;
            let tx = db.transaction().unwrap();
            note_origins::record_note_in(
                &tx,
                &key,
                &note.id,
                &format!("{:x}", Sha256::digest(b"original")),
                &profile.access_epoch,
            )?;
            tx.commit().unwrap();
            Ok(note.id)
        })
        .unwrap();
    assert_eq!(
        inspect().unwrap().state,
        note_origins::NoteOriginState::Unchanged(note_id.clone())
    );
    fs::write(&source, b"changed").unwrap();
    let result = inspect().unwrap();
    assert_eq!(
        result.state,
        note_origins::NoteOriginState::SourceChanged(note_id.clone())
    );
    assert_eq!(result.sha256, format!("{:x}", Sha256::digest(b"changed")));
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE knowledge_notes SET title='用户修改' WHERE id=?1",
                [&note_id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        inspect().unwrap().state,
        note_origins::NoteOriginState::BothChanged(note_id.clone())
    );
    fs::write(&source, b"original").unwrap();
    assert_eq!(
        inspect().unwrap().state,
        note_origins::NoteOriginState::TargetChanged(note_id)
    );
    assert_eq!(fs::read(source).unwrap(), b"original");
    assert!(selection.inspect(&state, "../other.md").is_err());
    state
        .with_library(|db, _| source_profiles::invalidate_epochs(db))
        .unwrap();
    assert!(inspect().is_err());
}

#[test]
fn learning_source_markdown_rejects_hardlinks_oversize_and_write_during_read() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    let file = root.join("note.md");
    fs::write(&file, b"test").unwrap();
    let canonical_root = root.canonicalize().unwrap();
    let lease = source_markdown::open_markdown(&canonical_root, "note.md").unwrap();
    assert!(fs::write(&file, b"other").is_err());
    assert!(fs::rename(&root, temp.path().join("moved")).is_err());
    drop(lease);
    let link = root.join("link.md");
    fs::hard_link(&file, &link).unwrap();
    assert!(source_markdown::open_markdown(&canonical_root, "note.md").is_err());
    fs::remove_file(link).unwrap();
    fs::write(&file, vec![0u8; 5 * 1024 * 1024 + 1]).unwrap();
    assert!(source_markdown::open_markdown(&canonical_root, "note.md").is_err());
}

#[test]
fn learning_source_markdown_scope_does_not_revive_after_switch() {
    use crate::models::StorageChangeRequest;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("note.md"), b"source").unwrap();
    let state = test_state(&temp.path().join("library"));
    let scope = source_profiles::scope(&state, &state.active_library_id().unwrap()).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let selection = source_markdown::select_vault(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        "vault",
        &root,
    )
    .unwrap();
    selection.register_identity(&state).unwrap();
    state
        .change_library(StorageChangeRequest {
            mode: "create".into(),
            path: temp.path().join("other").to_string_lossy().into_owned(),
            name: Some("其他".into()),
        })
        .unwrap();
    assert!(selection.inspect(&state, "note.md").is_err());
    assert!(source_markdown::create_vault_source(&state, &scope, "vault", &root).is_err());
    state
        .change_library(StorageChangeRequest {
            mode: "open".into(),
            path: temp.path().join("library").to_string_lossy().into_owned(),
            name: None,
        })
        .unwrap();
    assert!(selection.inspect(&state, "note.md").is_err());
}
