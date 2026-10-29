use super::{
    import_access,
    import_runner::ImportTaskRequest,
    note_conversion::{self, AttachmentMap, MediaMap},
};
use crate::{
    knowledge::{self, NoteInput},
    state::test_state,
    tasks::TaskRequest,
};
use sha2::{Digest, Sha256};
use std::fs;

fn write_note(root: &std::path::Path, relative: &str, body: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

#[test]
fn learning_import_access_bind_preview_prepare_without_overwrite_on_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let vault_root = temp.path().join("vault");
    fs::create_dir(&vault_root).unwrap();
    write_note(&vault_root, "notes/hello.md", "# 迁入预检\n\n正文一行。\n");
    let state = test_state(&library);
    let library_id = state.active_library_id().unwrap();

    let binding = import_access::bind_in_library(
        &state,
        &library_id,
        "vault-ui",
        vault_root.to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(binding.library_id, library_id);
    assert!(binding.boundary.contains("不会覆盖"));

    let preview = import_access::preview_in_library(
        &state,
        &library_id,
        &binding.profile_id,
        &binding.epoch,
        &binding.vault,
        &binding.root,
        "notes/hello.md",
    )
    .unwrap();
    assert_eq!(preview.conflict, "unmapped");
    assert!(preview.can_prepare);
    assert!(!preview.will_overwrite);

    let markdown = fs::read_to_string(vault_root.join("notes/hello.md")).unwrap();
    let conversion = note_conversion::convert_source_note(
        markdown.as_bytes(),
        &AttachmentMap::new(),
        &MediaMap::new(),
    )
    .unwrap();
    assert!(!conversion.pending);
    let document = conversion.document.expect("converted document");
    let request_seed = format!(
        "{}|{}|{}|{}|{}",
        preview.profile_id,
        preview.vault,
        preview.relative_path,
        preview.source_sha256,
        preview.epoch
    );
    let prepare = ImportTaskRequest::Prepare {
        profile_id: preview.profile_id.clone(),
        epoch: preview.epoch.clone(),
        request_sha256: format!("{:x}", Sha256::digest(request_seed.as_bytes())),
        vault: preview.vault.clone(),
        relative_path: preview.relative_path.clone(),
        source_sha256: preview.source_sha256.clone(),
        title: preview.title.clone(),
        document,
        markdown,
    };
    let (task, _) = state
        .tasks
        .submit(
            library_id.clone(),
            TaskRequest::LearningImport(prepare.clone()),
        )
        .unwrap();
    let prepared = super::import_runner::run_local(&state, &task, &prepare).unwrap();
    assert_eq!(prepared["status"], "staged");
    let batch_id = prepared["batchId"].as_str().unwrap().to_string();

    let commit = ImportTaskRequest::Commit {
        batch_id: batch_id.clone(),
    };
    let (commit_task, _) = state
        .tasks
        .submit(
            library_id.clone(),
            TaskRequest::LearningImport(commit.clone()),
        )
        .unwrap();
    let committed = super::import_runner::run_local(&state, &commit_task, &commit).unwrap();
    assert_eq!(committed["status"], "committed");

    let batches = import_access::list_batches_in_library(&state, &library_id).unwrap();
    assert!(batches
        .iter()
        .any(|b| b.id == batch_id && b.status == "committed"));
    let items = import_access::list_items_in_library(&state, &library_id, &batch_id).unwrap();
    assert_eq!(items[0].status, "committed");
    let note_id = items[0].target_id.clone().unwrap();

    state
        .with_library_mut(|db, _| {
            let note = knowledge::get(db, &note_id)?;
            knowledge::save(
                db,
                NoteInput {
                    id: Some(note.id.clone()),
                    expected_revision: Some(note.revision),
                    notebook_id: note.notebook_id.clone(),
                    title: "用户已改".into(),
                    document: note.document.clone(),
                    tags: note.tags.clone(),
                    favorite: note.favorite,
                    journal_date: note.journal_date.clone(),
                },
            )?;
            Ok(())
        })
        .unwrap();
    let conflict = import_access::preview_in_library(
        &state,
        &library_id,
        &binding.profile_id,
        &binding.epoch,
        &binding.vault,
        &binding.root,
        "notes/hello.md",
    )
    .unwrap();
    assert_eq!(conflict.conflict, "target_changed");
    assert!(!conflict.will_overwrite);
    assert!(conflict.message.contains("不会静默覆盖"));
    assert!(conflict.can_prepare);
}

#[test]
fn learning_import_access_rejects_pending_conversion_and_wrong_library() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let vault_root = temp.path().join("vault");
    fs::create_dir(&vault_root).unwrap();
    fs::write(vault_root.join("bad.md"), [0xff, 0xfe, b'#', b' ', b'x']).unwrap();
    let state = test_state(&library);
    let library_id = state.active_library_id().unwrap();
    let binding = import_access::bind_in_library(
        &state,
        &library_id,
        "vault-ui",
        vault_root.to_str().unwrap(),
    )
    .unwrap();
    let preview = import_access::preview_in_library(
        &state,
        &library_id,
        &binding.profile_id,
        &binding.epoch,
        &binding.vault,
        &binding.root,
        "bad.md",
    )
    .unwrap();
    assert!(preview.pending_conversion);
    assert!(!preview.can_prepare);
    assert!(import_access::bind_in_library(
        &state,
        "00000000-0000-4000-8000-000000000099",
        "vault-ui",
        vault_root.to_str().unwrap(),
    )
    .is_err());
}

#[test]
fn learning_import_access_requires_current_session_native_selection() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    write_note(&vault, "note.md", "# Test\n");
    let state = test_state(&temp.path().join("library"));
    let library_id = state.active_library_id().unwrap();
    let root = vault.to_str().unwrap();
    let fake = import_access::bind_in_library(&state, &library_id, "vault-ui", root).unwrap();
    assert!(import_access::preview_authorized(
        &state,
        &library_id,
        &fake.profile_id,
        &fake.epoch,
        &fake.vault,
        &fake.root,
        "note.md"
    )
    .is_err());
    let bound =
        import_access::bind_with_picker(&state, &library_id, "vault-ui", || Some(vault.clone()))
            .unwrap()
            .unwrap();
    assert!(import_access::preview_authorized(
        &state,
        &library_id,
        &bound.profile_id,
        &bound.epoch,
        &bound.vault,
        &bound.root,
        "note.md"
    )
    .is_ok());
    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    assert!(import_access::preview_authorized(
        &state,
        &library_id,
        &bound.profile_id,
        &bound.epoch,
        &bound.vault,
        &bound.root,
        "note.md"
    )
    .is_err());
    assert!(
        import_access::bind_with_picker(&state, &library_id, "vault-ui", || None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn learning_import_access_selection_cancel_and_library_change_do_not_grant_access() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    write_note(&vault, "note.md", "# Test\n");
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let binding =
        import_access::bind_with_picker(&state, &library, "vault-ui", || Some(vault.clone()))
            .unwrap()
            .unwrap();
    assert!(
        import_access::bind_with_picker(&state, &library, "vault-ui", || None)
            .unwrap()
            .is_none()
    );
    assert!(import_access::preview_authorized(
        &state,
        &library,
        &binding.profile_id,
        &binding.epoch,
        &binding.vault,
        &binding.root,
        "note.md"
    )
    .is_err());
    assert!(
        import_access::bind_with_picker(&state, &library, "vault-ui", || {
            *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
            Some(vault.clone())
        })
        .is_err()
    );
}

#[test]
fn learning_import_enqueue_rejects_stale_library_session() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = super::source_profiles::scope(&state, &library).unwrap();
    let payload = TaskRequest::LearningImport(ImportTaskRequest::Cancel {
        batch_id: uuid::Uuid::new_v4().to_string(),
    });
    assert!(crate::tasks::submit_scoped_learning_import(&state, payload.clone(), &scope).is_ok());
    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    assert!(crate::tasks::submit_scoped_learning_import(&state, payload, &scope).is_err());
}

#[test]
fn learning_import_prepared_payload_keeps_its_original_session() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    write_note(&vault, "note.md", "# Test\n");
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let binding = import_access::bind_with_picker(&state, &library, "vault-ui", || Some(vault))
        .unwrap()
        .unwrap();
    let (payload, scope) = import_access::prepare_payload(
        &state,
        &library,
        &binding.profile_id,
        &binding.epoch,
        &binding.vault,
        &binding.root,
        "note.md",
    )
    .unwrap();
    assert!(matches!(
        payload,
        TaskRequest::LearningImport(ImportTaskRequest::Prepare { .. })
    ));
    *state.source_scope_nonce.lock().unwrap() = uuid::Uuid::new_v4();
    assert!(crate::tasks::submit_scoped_learning_import(&state, payload, &scope).is_err());
}
