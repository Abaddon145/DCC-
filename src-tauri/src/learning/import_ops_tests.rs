use super::{
    import_ops::{self, NewImportItem},
    note_origins, source_profiles,
};
use crate::{
    knowledge::{self, NoteInput},
    state::test_state,
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn scope(state: &crate::state::AppState) -> source_profiles::LibraryScope {
    source_profiles::scope(state, &state.active_library_id().unwrap()).unwrap()
}

fn profile(state: &crate::state::AppState) -> source_profiles::SourceProfile {
    source_profiles::create(state, &scope(state)).unwrap()
}

fn doc() -> serde_json::Value {
    json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"迁入正文"}]}]})
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn learning_import_ops_upgrade_rolls_back_and_reaches_v34() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    state
        .with_library_mut(|db, _| {
            db.execute_batch(
                "DROP TABLE learning_import_items; DROP TABLE learning_import_batches; UPDATE schema_meta SET value='33' WHERE key='schema_version'; CREATE TRIGGER reject_v34 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='34' BEGIN SELECT RAISE(ABORT,'test rollback'); END;",
            )
            .unwrap();
            assert!(super::migrate(db).is_err());
            assert_eq!(
                db.query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "33"
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='learning_import_batches'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            db.execute_batch("DROP TRIGGER reject_v34;").unwrap();
            super::migrate(db)?;
            super::migrate(db)?;
            assert_eq!(
                db.query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "34"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_import_ops_batch_is_idempotent_and_epoch_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let source = profile(&state);
    let digest = sha(b"request-one");
    let a =
        import_ops::create_batch(&state, &sc, &source.id, &source.access_epoch, &digest).unwrap();
    let b =
        import_ops::create_batch(&state, &sc, &source.id, &source.access_epoch, &digest).unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(a.status, "pending");
    let stale = "00000000-0000-4000-8000-000000000099";
    assert!(import_ops::create_batch(&state, &sc, &source.id, stale, &sha(b"other")).is_err());
}

#[test]
fn learning_import_ops_stage_cancel_cleans_runtime_files() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let state = test_state(&library);
    let sc = scope(&state);
    let source = profile(&state);
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"stage-cancel"),
    )
    .unwrap();
    let bytes = b"# hello import\n";
    let item = import_ops::add_item(
        &state,
        &sc,
        &batch.id,
        NewImportItem {
            vault: "vault-a".into(),
            relative_path: "notes/a.md".into(),
            source_sha256: sha(bytes),
            title: "暂存笔记".into(),
            document: doc(),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    assert!(import_ops::stage_item_bytes(&state, &sc, &batch.id, &item.id, b"wrong").is_err());
    let staged = import_ops::stage_item_bytes(&state, &sc, &batch.id, &item.id, bytes).unwrap();
    assert_eq!(staged.status, "staged");
    let relative = staged.stage_relative.clone().unwrap();
    let path = library.join(&relative);
    assert!(path.is_file());
    let cancelled = import_ops::cancel_batch(&state, &sc, &batch.id).unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(!path.exists());
    let listed = import_ops::list_batches(&state, &sc).unwrap();
    assert!(listed
        .iter()
        .any(|b| b.id == batch.id && b.status == "cancelled"));
}

#[test]
fn learning_import_ops_commit_creates_note_and_origin_without_overwrite_on_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let state = test_state(&library);
    let sc = scope(&state);
    let source = profile(&state);
    let bytes = b"body-a";
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"commit-new"),
    )
    .unwrap();
    let item = import_ops::add_item(
        &state,
        &sc,
        &batch.id,
        NewImportItem {
            vault: "vault-a".into(),
            relative_path: "notes/new.md".into(),
            source_sha256: sha(bytes),
            title: "新建迁入".into(),
            document: doc(),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    import_ops::stage_item_bytes(&state, &sc, &batch.id, &item.id, bytes).unwrap();
    let done = import_ops::commit_batch(&state, &sc, &batch.id).unwrap();
    assert_eq!(done.status, "committed");
    let committed = import_ops::get_item(&state, &sc, &item.id).unwrap();
    assert_eq!(committed.status, "committed");
    let note_id = committed.target_id.clone().unwrap();
    let note = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_id)?))
        .unwrap();
    assert_eq!(note.title, "新建迁入");
    let stage = library.join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, item.id));
    assert!(!stage.exists());

    // Second import same key with same source hash is Unchanged on a new batch? Origin already mapped.
    // Target revision conflict path: edit note then try import with old expected mapping via separate batch
    // that sees TargetChanged because source hash same but note body changed.
    state
        .with_library_mut(|db, _| {
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
    let conflict_batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"commit-conflict"),
    )
    .unwrap();
    let conflict_item = import_ops::add_item(
        &state,
        &sc,
        &conflict_batch.id,
        NewImportItem {
            vault: "vault-a".into(),
            relative_path: "notes/new.md".into(),
            source_sha256: sha(bytes),
            title: "应冲突".into(),
            document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"覆盖尝试"}]}]}),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    import_ops::stage_item_bytes(&state, &sc, &conflict_batch.id, &conflict_item.id, bytes)
        .unwrap();
    let conflicted = import_ops::commit_batch(&state, &sc, &conflict_batch.id).unwrap();
    assert_eq!(conflicted.status, "committed");
    let after = import_ops::get_item(&state, &sc, &conflict_item.id).unwrap();
    assert_eq!(after.status, "conflict");
    assert_eq!(after.conflict_code.as_deref(), Some("target_changed"));
    let note_after = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_id)?))
        .unwrap();
    assert_eq!(note_after.title, "用户已改");
    assert_ne!(note_after.title, "应冲突");
}

#[test]
fn learning_import_ops_pending_survives_for_interrupt_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let state = test_state(&library);
    let sc = scope(&state);
    let source = profile(&state);
    let bytes = b"interrupt";
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"interrupt"),
    )
    .unwrap();
    let item = import_ops::add_item(
        &state,
        &sc,
        &batch.id,
        NewImportItem {
            vault: "vault-b".into(),
            relative_path: "x.md".into(),
            source_sha256: sha(bytes),
            title: "中断".into(),
            document: doc(),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    import_ops::stage_item_bytes(&state, &sc, &batch.id, &item.id, bytes).unwrap();
    let pending = import_ops::list_batches(&state, &sc)
        .unwrap()
        .into_iter()
        .find(|b| b.id == batch.id)
        .unwrap();
    assert_eq!(pending.status, "pending");
    assert!(library
        .join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, item.id))
        .is_file());
    import_ops::cancel_batch(&state, &sc, &batch.id).unwrap();
    assert!(!library
        .join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, item.id))
        .exists());
    // No note created for cancelled path.
    let key = note_origins::NoteOriginKey::new(&source.id, "vault-b", "x.md").unwrap();
    let mapped = state
        .with_library(|db, _| note_origins::inspect(db, &key, &sha(bytes)))
        .unwrap();
    assert_eq!(mapped, note_origins::NoteOriginState::Unmapped);
}
