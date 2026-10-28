use super::{note_origins, source_markdown, source_profiles};
use crate::{
    knowledge::{self, NoteInput},
    state::test_state,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

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
    state
        .change_library(StorageChangeRequest {
            mode: "create".into(),
            path: temp.path().join("other").to_string_lossy().into_owned(),
            name: Some("其他".into()),
        })
        .unwrap();
    assert!(selection.inspect(&state, "note.md").is_err());
    state
        .change_library(StorageChangeRequest {
            mode: "open".into(),
            path: temp.path().join("library").to_string_lossy().into_owned(),
            name: None,
        })
        .unwrap();
    assert!(selection.inspect(&state, "note.md").is_err());
}
