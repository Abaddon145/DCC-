//! Synthetic-only tests for per-batch import revert (Task 13 / M6B).
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

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn scope(state: &crate::state::AppState) -> source_profiles::LibraryScope {
    source_profiles::scope(state, &state.active_library_id().unwrap()).unwrap()
}

fn commit_one(
    state: &crate::state::AppState,
    sc: &source_profiles::LibraryScope,
    profile: &source_profiles::SourceProfile,
    request: &[u8],
    path: &str,
    bytes: &[u8],
) -> (String, import_ops::ImportItem) {
    let batch =
        import_ops::create_batch(state, sc, &profile.id, &profile.access_epoch, &sha(request))
            .unwrap();
    let item = import_ops::add_item(
        state,
        sc,
        &batch.id,
        NewImportItem {
            vault: "vault-revert".into(),
            relative_path: path.into(),
            source_sha256: sha(bytes),
            title: "撤回测试".into(),
            document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"合成正文"}]}]}),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    import_ops::stage_item_bytes(state, sc, &batch.id, &item.id, bytes).unwrap();
    import_ops::commit_batch(state, sc, &batch.id).unwrap();
    let item = import_ops::get_item(state, sc, &item.id).unwrap();
    (batch.id, item)
}

fn deleted_at(state: &crate::state::AppState, id: &str) -> Option<String> {
    state
        .with_library(|db, _| Ok(knowledge::get(db, id)?.deleted_at))
        .unwrap()
}

fn origin(
    state: &crate::state::AppState,
    profile: &str,
    path: &str,
    bytes: &[u8],
) -> note_origins::NoteOriginState {
    let key = note_origins::NoteOriginKey::new(profile, "vault-revert", path).unwrap();
    state
        .with_library(|db, _| note_origins::inspect(db, &key, &sha(bytes)))
        .unwrap()
}

#[test]
fn learning_import_recovery_revert_trashes_unchanged_created_note_and_keeps_archive() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let profile = source_profiles::create(&state, &sc).unwrap();
    let raw = b"# revert me\n";
    let (batch_id, item) = commit_one(&state, &sc, &profile, b"revert-a", "a.md", raw);
    let note_id = item.target_id.clone().unwrap();

    let report = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch_id).unwrap();
    assert_eq!(report["batchId"], batch_id);
    assert_eq!(report["reverted"], json!([note_id]));
    assert_eq!(report["kept"], json!([]));
    assert!(
        deleted_at(&state, &note_id).is_some(),
        "created note goes to trash"
    );
    assert_eq!(
        origin(&state, &profile.id, "a.md", raw),
        note_origins::NoteOriginState::Unmapped
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_note_archives", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1,
                "raw archive is never removed by revert"
            );
            Ok(())
        })
        .unwrap();
    // Same request is free again: a new pending batch, not the reverted receipt.
    let again = import_ops::create_batch(
        &state,
        &sc,
        &profile.id,
        &profile.access_epoch,
        &sha(b"revert-a"),
    )
    .unwrap();
    assert_ne!(again.id, batch_id);
    assert_eq!(again.status, "pending");
}

#[test]
fn learning_import_recovery_revert_keeps_note_edited_after_import() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let profile = source_profiles::create(&state, &sc).unwrap();
    let raw = b"# edited later\n";
    let (batch_id, item) = commit_one(&state, &sc, &profile, b"revert-edit", "e.md", raw);
    let note_id = item.target_id.clone().unwrap();
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
    let report = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch_id).unwrap();
    assert_eq!(report["reverted"], json!([]));
    assert_eq!(report["kept"][0]["outcome"], "kept_modified");
    assert_eq!(report["kept"][0]["noteId"], note_id);
    assert!(deleted_at(&state, &note_id).is_none());
    let title = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_id)?.title))
        .unwrap();
    assert_eq!(title, "用户已改");
    assert!(matches!(
        origin(&state, &profile.id, "e.md", raw),
        note_origins::NoteOriginState::TargetChanged(_)
    ));
}

#[test]
fn learning_import_recovery_revert_keeps_reused_note_from_earlier_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let profile = source_profiles::create(&state, &sc).unwrap();
    let raw = b"# shared\n";
    let (_first, first_item) = commit_one(&state, &sc, &profile, b"reuse-1", "s.md", raw);
    let (second, second_item) = commit_one(&state, &sc, &profile, b"reuse-2", "s.md", raw);
    let note_id = first_item.target_id.clone().unwrap();
    assert_eq!(second_item.target_id.as_deref(), Some(note_id.as_str()));
    let report = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &second).unwrap();
    assert_eq!(report["reverted"], json!([]));
    assert_eq!(report["kept"][0]["outcome"], "kept_reused");
    assert!(deleted_at(&state, &note_id).is_none());
    assert!(matches!(
        origin(&state, &profile.id, "s.md", raw),
        note_origins::NoteOriginState::Unchanged(_)
    ));
}

#[test]
fn learning_import_recovery_revert_keeps_created_note_reused_by_a_later_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let profile = source_profiles::create(&state, &sc).unwrap();
    let raw = b"# shared later\n";
    let (first, original) = commit_one(&state, &sc, &profile, b"first", "shared.md", raw);
    let (_second, reused) = commit_one(&state, &sc, &profile, b"second", "shared.md", raw);
    let note_id = original.target_id.unwrap();
    assert_eq!(reused.target_id.as_deref(), Some(note_id.as_str()));

    let report = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &first).unwrap();
    assert_eq!(report["reverted"], json!([]));
    assert_eq!(report["kept"][0]["outcome"], "kept_reused");
    assert!(deleted_at(&state, &note_id).is_none());
    assert_eq!(
        origin(&state, &profile.id, "shared.md", raw),
        note_origins::NoteOriginState::Unchanged(note_id)
    );
}

#[test]
fn learning_import_recovery_revert_keeps_created_note_linked_to_a_project_later() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let profile = source_profiles::create(&state, &sc).unwrap();
    let (batch, item) = commit_one(&state, &sc, &profile, b"project", "p.md", b"# project\n");
    let note_id = item.target_id.unwrap();
    let project_id = state
        .with_library_mut(|db, _| {
            Ok(crate::projects::upsert_project(
                db,
                crate::models::ProjectInput {
                    id: None,
                    name: "我的项目".into(),
                    description: String::new(),
                    project_type: "scene".into(),
                    status: "active".into(),
                    target_tools: vec![],
                    versions: vec![],
                    resolution_width: None,
                    resolution_height: None,
                    frame_rate: None,
                    cover_asset_id: None,
                    cover_media_entry_id: None,
                },
            )?
            .summary
            .id)
        })
        .unwrap();
    super::project_notes::link_in_library(&state, sc.library_id(), &project_id, &note_id).unwrap();

    let report = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch).unwrap();
    assert_eq!(report["reverted"], json!([]));
    assert_eq!(report["kept"][0]["outcome"], "kept_modified");
    assert!(deleted_at(&state, &note_id).is_none());
    assert_eq!(
        super::project_notes::list_in_library(&state, sc.library_id(), &project_id, 0, 20)
            .unwrap()
            .total,
        1
    );
}

#[test]
fn learning_import_recovery_revert_keeps_created_note_referenced_by_another_note() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let profile = source_profiles::create(&state, &sc).unwrap();
    let (batch, item) = commit_one(
        &state,
        &sc,
        &profile,
        b"reference",
        "r.md",
        b"# referenced\n",
    );
    let note_id = item.target_id.unwrap();
    state.with_library_mut(|db, _| {
        knowledge::save(db, NoteInput {
            id: None, expected_revision: None, notebook_id: None, title: "后来的笔记".into(),
            document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"reference","attrs":{"kind":"note","targetId":note_id,"label":"引用的笔记"}}]}]}),
            tags: vec![], favorite: false, journal_date: None,
        })?;
        Ok(())
    }).unwrap();

    let report = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch).unwrap();
    assert_eq!(report["reverted"], json!([]));
    assert_eq!(report["kept"][0]["outcome"], "kept_modified");
    assert!(deleted_at(&state, &note_id).is_none());
    state
        .with_library(|db, _| {
            assert_eq!(knowledge::backlinks(db, &note_id)?.len(), 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_import_recovery_revert_keeps_created_note_used_by_course_review_or_order() {
    for usage in ["course", "review", "order"] {
        let (_root, state, _library, course_id, _) = super::excerpt_runtime_tests::fixture(false);
        let sc = scope(&state);
        let profile = source_profiles::create(&state, &sc).unwrap();
        let (batch, item) = commit_one(
            &state,
            &sc,
            &profile,
            usage.as_bytes(),
            "note.md",
            b"# note\n",
        );
        let note_id = item.target_id.unwrap();
        state.with_library_mut(|db, _| {
            match usage {
                "course" => db.execute("INSERT INTO learning_course_notes(course_id,note_id) VALUES(?1,?2)", rusqlite::params![course_id,note_id]),
                "review" => db.execute("INSERT INTO knowledge_reviews(note_id,due_date,status) VALUES(?1,'2026-10-30','waiting')", [&note_id]),
                "order" => db.execute("INSERT INTO knowledge_note_order(scope,note_id,position) VALUES('__all__',?1,0)", [&note_id]),
                _ => unreachable!(),
            }.map_err(|error| error.to_string())?;
            Ok(())
        }).unwrap();

        let report = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch).unwrap();
        assert_eq!(report["reverted"], json!([]), "{usage}");
        assert_eq!(report["kept"][0]["outcome"], "kept_modified", "{usage}");
        assert!(deleted_at(&state, &note_id).is_none(), "{usage}");
    }
}

#[test]
fn learning_import_recovery_revert_is_idempotent_and_rejects_unfinished_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let profile = source_profiles::create(&state, &sc).unwrap();
    let (batch_id, _) = commit_one(&state, &sc, &profile, b"idem", "i.md", b"# idem\n");
    let first = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch_id).unwrap();
    let second = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch_id).unwrap();
    assert_eq!(first, second);
    let pending = import_ops::create_batch(
        &state,
        &sc,
        &profile.id,
        &profile.access_epoch,
        &sha(b"open"),
    )
    .unwrap();
    let err =
        import_ops::revert_batch(&state, &sc, &profile.access_epoch, &pending.id).unwrap_err();
    assert!(err.contains("已提交"), "{err}");
}

#[test]
fn learning_import_recovery_revert_handles_committed_items_in_a_cancelled_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let profile = source_profiles::create(&state, &sc).unwrap();
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &profile.id,
        &profile.access_epoch,
        &sha(b"partial"),
    )
    .unwrap();
    for (path, raw) in [
        ("first.md", b"# first\n".as_slice()),
        ("second.md", b"# second\n".as_slice()),
    ] {
        let item = import_ops::add_item(
            &state,
            &sc,
            &batch.id,
            NewImportItem {
                vault: "vault-revert".into(),
                relative_path: path.into(),
                source_sha256: sha(raw),
                title: path.into(),
                document: json!({"type":"doc","content":[{"type":"paragraph"}]}),
                expected_target_id: None,
                expected_target_revision: None,
                expected_target_sha256: None,
            },
        )
        .unwrap();
        import_ops::stage_item_bytes(&state, &sc, &batch.id, &item.id, raw).unwrap();
    }
    import_ops::set_commit_fail_after_applies(Some(1));
    assert!(import_ops::commit_batch(&state, &sc, &batch.id).is_err());
    assert_eq!(
        import_ops::cancel_batch(&state, &sc, &batch.id)
            .unwrap()
            .status,
        "cancelled"
    );
    let items = import_ops::list_items(&state, &sc, &batch.id).unwrap();
    let note_id = items
        .iter()
        .find(|item| item.status == "committed")
        .unwrap()
        .target_id
        .clone()
        .unwrap();
    assert!(items.iter().any(|item| item.status == "cancelled"));

    let report = import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch.id).unwrap();
    assert_eq!(report["reverted"], json!([note_id]));
    assert!(deleted_at(&state, &note_id).is_some());
    assert_eq!(
        report,
        import_ops::revert_batch(&state, &sc, &profile.access_epoch, &batch.id).unwrap()
    );
}

#[test]
fn learning_import_recovery_backup_restores_batch_history_into_a_different_library() {
    let temp = tempfile::tempdir().unwrap();
    let source = test_state(&temp.path().join("source"));
    let sc = scope(&source);
    let profile = source_profiles::create(&source, &sc).unwrap();
    let (batch_id, item) = commit_one(&source, &sc, &profile, b"restore", "r.md", b"# raw\r\n");
    let backup = temp.path().join("backup.dccassetlib");
    source
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    let target = test_state(&temp.path().join("target"));
    assert_ne!(
        source.active_library_id().unwrap(),
        target.active_library_id().unwrap()
    );
    target
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    let target_scope = scope(&target);
    let batches = import_ops::list_batches(&target, &target_scope).unwrap();
    let restored_batch = batches
        .iter()
        .find(|batch| batch.id == batch_id)
        .expect("restored history must remain visible");
    assert_ne!(restored_batch.current_source_epoch, profile.access_epoch);
    let restored = import_ops::list_items(&target, &target_scope, &batch_id).unwrap();
    assert_eq!(restored[0].target_id, item.target_id);
    let note_id = item.target_id.unwrap();
    let report = import_ops::revert_batch(
        &target,
        &target_scope,
        &restored_batch.current_source_epoch,
        &batch_id,
    )
    .unwrap();
    assert_eq!(report["reverted"], json!([note_id]));
    assert!(deleted_at(&target, &note_id).is_some());
    assert!(
        deleted_at(&source, &note_id).is_none(),
        "restoring and reverting another library must not alter the source"
    );
    target.with_library(|db, base| {
        let archive: String = db.query_row("SELECT json_extract(receipt_json,'$.archiveId') FROM learning_import_items WHERE id=?1", [&item.id], |row| row.get(0)).map_err(|error| error.to_string())?;
        assert_eq!(super::note_archive::read_archive(db, base, &archive)?, b"# raw\r\n");
        Ok(())
    }).unwrap();

    let backup_after = temp.path().join("after.dccassetlib");
    target
        .with_library(|db, base| crate::backup::export_library(db, base, &backup_after))
        .unwrap();
    let restored_again = test_state(&temp.path().join("target-again"));
    restored_again
        .with_active_mut(|active| crate::backup::restore_library(active, &backup_after))
        .unwrap();
    let final_scope = scope(&restored_again);
    let final_batches = import_ops::list_batches(&restored_again, &final_scope).unwrap();
    let saved = final_batches
        .iter()
        .find(|batch| batch.id == batch_id)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(saved.revert_report_json.as_ref().unwrap())
            .unwrap(),
        report
    );
    restored_again.with_library(|db, base| {
        let archive: String = db.query_row("SELECT json_extract(receipt_json,'$.archiveId') FROM learning_import_items WHERE id=?1", [&item.id], |row| row.get(0)).map_err(|error| error.to_string())?;
        assert_eq!(super::note_archive::read_archive(db, base, &archive)?, b"# raw\r\n");
        Ok(())
    }).unwrap();
}
