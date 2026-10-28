use super::{note_origins, source_markdown, source_profiles};
use crate::{
    knowledge::{self, NoteInput},
    state::test_state,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

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
    let inspect = || {
        source_markdown::inspect_markdown(
            &state,
            &scope,
            &profile.id,
            &profile.access_epoch,
            &root,
            "vault-id",
            "note.md",
        )
    };
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
    assert!(source_markdown::inspect_markdown(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &root,
        "vault-id",
        "../other.md"
    )
    .is_err());
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
    let lease = source_markdown::open_markdown(&root, "note.md").unwrap();
    assert!(fs::write(&file, b"other").is_err());
    assert!(fs::rename(&root, temp.path().join("moved")).is_err());
    drop(lease);
    let link = root.join("link.md");
    fs::hard_link(&file, &link).unwrap();
    assert!(source_markdown::open_markdown(&root, "note.md").is_err());
    fs::remove_file(link).unwrap();
    fs::write(&file, vec![0u8; 5 * 1024 * 1024 + 1]).unwrap();
    assert!(source_markdown::open_markdown(&root, "note.md").is_err());
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
    state
        .change_library(StorageChangeRequest {
            mode: "create".into(),
            path: temp.path().join("other").to_string_lossy().into_owned(),
            name: Some("其他".into()),
        })
        .unwrap();
    assert!(source_markdown::inspect_markdown(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &root,
        "vault",
        "note.md"
    )
    .is_err());
}
