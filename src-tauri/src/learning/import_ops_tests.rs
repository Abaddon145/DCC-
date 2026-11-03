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
                "DROP TABLE learning_import_item_reverts; DROP TABLE learning_import_reverts; DROP TABLE learning_import_items; DROP TABLE learning_import_batches; UPDATE schema_meta SET value='33' WHERE key='schema_version'; CREATE TRIGGER reject_v34 BEFORE UPDATE OF value ON schema_meta WHEN NEW.key='schema_version' AND NEW.value='34' BEGIN SELECT RAISE(ABORT,'test rollback'); END;",
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
                "41"
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

fn add_staged(
    state: &crate::state::AppState,
    sc: &source_profiles::LibraryScope,
    batch_id: &str,
    vault: &str,
    path: &str,
    bytes: &[u8],
    title: &str,
) -> import_ops::ImportItem {
    let item = import_ops::add_item(
        state,
        sc,
        batch_id,
        NewImportItem {
            vault: vault.into(),
            relative_path: path.into(),
            source_sha256: sha(bytes),
            title: title.into(),
            document: doc(),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    import_ops::stage_item_bytes(state, sc, batch_id, &item.id, bytes).unwrap()
}

#[test]
fn learning_import_ops_mid_commit_failure_resumes_without_orphan_stages() {
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
        &sha(b"mid-resume"),
    )
    .unwrap();
    let a = add_staged(&state, &sc, &batch.id, "vault", "a.md", b"body-a", "条目甲");
    let b = add_staged(&state, &sc, &batch.id, "vault", "b.md", b"body-b", "条目乙");
    let stage_a = library.join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, a.id));
    let stage_b = library.join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, b.id));
    assert!(stage_a.is_file() && stage_b.is_file());

    import_ops::set_commit_fail_after_applies(Some(1));
    let err = import_ops::commit_batch(&state, &sc, &batch.id).unwrap_err();
    assert!(err.contains("中断"));
    let mid = import_ops::list_batches(&state, &sc)
        .unwrap()
        .into_iter()
        .find(|x| x.id == batch.id)
        .unwrap();
    assert_eq!(mid.status, "pending");
    let items = import_ops::list_items(&state, &sc, &batch.id).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].status, "committed");
    assert!(items[0].stage_relative.is_none());
    assert!(!stage_a.exists());
    assert_eq!(items[1].status, "staged");
    assert!(stage_b.is_file());
    let note_a = items[0].target_id.clone().unwrap();
    let before = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_a)?))
        .unwrap();
    assert_eq!(before.title, "条目甲");

    let done = import_ops::commit_batch(&state, &sc, &batch.id).unwrap();
    assert_eq!(done.status, "committed");
    let receipt: serde_json::Value =
        serde_json::from_str(done.receipt_json.as_deref().unwrap()).unwrap();
    assert_eq!(receipt["committed"], 2);
    assert_eq!(receipt["conflicts"], 0);
    let items = import_ops::list_items(&state, &sc, &batch.id).unwrap();
    assert!(items
        .iter()
        .all(|i| i.status == "committed" && i.stage_relative.is_none()));
    assert!(!stage_a.exists());
    assert!(!stage_b.exists());
    assert!(!library
        .join(format!(".dcc/runtime/imports/{}", batch.id))
        .exists());
    let note_b = items[1].target_id.clone().unwrap();
    let after_a = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_a)?))
        .unwrap();
    let after_b = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_b)?))
        .unwrap();
    assert_eq!(after_a.title, "条目甲");
    assert_eq!(after_b.title, "条目乙");
    // Idempotent re-commit leaves durable receipt.
    let again = import_ops::commit_batch(&state, &sc, &batch.id).unwrap();
    assert_eq!(again.status, "committed");
    assert_eq!(again.receipt_json, done.receipt_json);
}

#[test]
fn learning_import_ops_mid_commit_failure_cancel_keeps_committed_and_cleans_runtime() {
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
        &sha(b"mid-cancel"),
    )
    .unwrap();
    let a = add_staged(
        &state,
        &sc,
        &batch.id,
        "vault",
        "keep.md",
        b"keep-body",
        "保留",
    );
    let b = add_staged(
        &state,
        &sc,
        &batch.id,
        "vault",
        "drop.md",
        b"drop-body",
        "放弃",
    );
    // Simulate legacy orphan stage left on an already-committed row after crash.
    import_ops::set_commit_fail_after_applies(Some(1));
    assert!(import_ops::commit_batch(&state, &sc, &batch.id).is_err());
    let committed = import_ops::get_item(&state, &sc, &a.id).unwrap();
    assert_eq!(committed.status, "committed");
    let note_id = committed.target_id.clone().unwrap();
    let orphan = library.join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, a.id));
    std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    std::fs::write(&orphan, b"orphan-stage").unwrap();
    let orphan_rel = format!(".dcc/runtime/imports/{}/{}.md", batch.id, a.id);
    state
        .with_library_mut(|db, _| {
            db.execute(
                "UPDATE learning_import_items SET stage_relative=?1 WHERE id=?2",
                [orphan_rel.as_str(), a.id.as_str()],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();

    let cancelled = import_ops::cancel_batch(&state, &sc, &batch.id).unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let items = import_ops::list_items(&state, &sc, &batch.id).unwrap();
    let keep = items.iter().find(|i| i.id == a.id).unwrap();
    let drop = items.iter().find(|i| i.id == b.id).unwrap();
    assert_eq!(keep.status, "committed");
    assert!(keep.stage_relative.is_none());
    assert_eq!(drop.status, "cancelled");
    assert!(drop.stage_relative.is_none());
    assert!(!orphan.exists());
    assert!(!library
        .join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, b.id))
        .exists());
    assert!(!library
        .join(format!(".dcc/runtime/imports/{}", batch.id))
        .exists());
    let note = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_id)?))
        .unwrap();
    assert_eq!(note.title, "保留");
    let key_drop = note_origins::NoteOriginKey::new(&source.id, "vault", "drop.md").unwrap();
    let mapped = state
        .with_library(|db, _| note_origins::inspect(db, &key_drop, &sha(b"drop-body")))
        .unwrap();
    assert_eq!(mapped, note_origins::NoteOriginState::Unmapped);
}

#[test]
fn learning_import_ops_preserves_exact_raw_bytes_in_archive_and_backup() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let state = test_state(&library);
    let sc = scope(&state);
    let source = profile(&state);
    let raw = b"\xef\xbb\xbf# Lesson\r\n\r\nOriginal text\r\n";
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"raw-archive"),
    )
    .unwrap();
    let item = add_staged(&state, &sc, &batch.id, "vault", "notes/a.md", raw, "Lesson");
    import_ops::commit_batch(&state, &sc, &batch.id).unwrap();
    let archive_id = state
        .with_library(|db, base| {
            let receipt: String = db
                .query_row(
                    "SELECT receipt_json FROM learning_import_items WHERE id=?1",
                    [&item.id],
                    |r| r.get(0),
                )
                .unwrap();
            let receipt: serde_json::Value = serde_json::from_str(&receipt).unwrap();
            let id = receipt["archiveId"]
                .as_str()
                .expect("committed item links its raw archive")
                .to_string();
            assert_eq!(super::note_archive::read_archive(db, base, &id)?, raw);
            let page = super::note_archive_browser::list_archives(db, 0, 50)?;
            let version = page.versions.items.iter().find(|row| row.id == id).unwrap();
            assert_eq!(version.conversion_state, "converted");
            assert!(version.source_key.contains("notes/a.md"));
            Ok(id)
        })
        .unwrap();
    let backup = temp.path().join("backup.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    let restored = test_state(&temp.path().join("restored"));
    restored
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    restored
        .with_library(|db, base| {
            assert_eq!(
                super::note_archive::read_archive(db, base, &archive_id)?,
                raw
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_import_ops_rejects_missing_or_changed_stage_and_recovers_after_restaging() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let state = test_state(&library);
    let sc = scope(&state);
    let source = profile(&state);
    let raw = b"# source\r\n";
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"stage-integrity"),
    )
    .unwrap();
    let item = import_ops::add_item(
        &state,
        &sc,
        &batch.id,
        NewImportItem {
            vault: "vault".into(),
            relative_path: "a.md".into(),
            source_sha256: sha(raw),
            title: "source".into(),
            document: doc(),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    assert!(
        import_ops::commit_batch(&state, &sc, &batch.id).is_err(),
        "unstaged item cannot be committed"
    );
    import_ops::stage_item_bytes(&state, &sc, &batch.id, &item.id, raw).unwrap();
    let path = library.join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, item.id));
    std::fs::write(&path, b"# changed\n").unwrap();
    assert!(
        import_ops::commit_batch(&state, &sc, &batch.id).is_err(),
        "changed staged bytes cannot be committed"
    );
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
    import_ops::stage_item_bytes(&state, &sc, &batch.id, &item.id, raw).unwrap();
    assert_eq!(
        import_ops::commit_batch(&state, &sc, &batch.id)
            .unwrap()
            .status,
        "committed"
    );
}

#[test]
fn learning_import_ops_reuses_archive_when_note_transaction_fails() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let source = profile(&state);
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"archive-retry"),
    )
    .unwrap();
    let item = add_staged(
        &state,
        &sc,
        &batch.id,
        "vault",
        "a.md",
        b"raw body",
        "source",
    );
    state.with_library(|db, _| {
        db.execute_batch("CREATE TRIGGER fail_import_note BEFORE INSERT ON knowledge_notes BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
        Ok(())
    }).unwrap();
    assert!(import_ops::commit_batch(&state, &sc, &batch.id).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_note_archives", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                db.query_row(
                    "SELECT status FROM learning_import_items WHERE id=?1",
                    [&item.id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "staged"
            );
            db.execute_batch("DROP TRIGGER fail_import_note").unwrap();
            Ok(())
        })
        .unwrap();
    import_ops::commit_batch(&state, &sc, &batch.id).unwrap();
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_note_archives", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_import_ops_staging_does_not_truncate_preexisting_partial_hardlink() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let state = test_state(&library);
    let sc = scope(&state);
    let source = profile(&state);
    let raw = b"# safe\n";
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"partial-hardlink"),
    )
    .unwrap();
    let item = import_ops::add_item(
        &state,
        &sc,
        &batch.id,
        NewImportItem {
            vault: "vault".into(),
            relative_path: "a.md".into(),
            source_sha256: sha(raw),
            title: "source".into(),
            document: doc(),
            expected_target_id: None,
            expected_target_revision: None,
            expected_target_sha256: None,
        },
    )
    .unwrap();
    let dir = library.join(format!(".dcc/runtime/imports/{}", batch.id));
    std::fs::create_dir_all(&dir).unwrap();
    let outside = temp.path().join("untouched.bin");
    std::fs::write(&outside, b"user data").unwrap();
    std::fs::hard_link(&outside, dir.join(format!("{}.md.partial", item.id))).unwrap();
    import_ops::stage_item_bytes(&state, &sc, &batch.id, &item.id, raw).unwrap();
    assert_eq!(std::fs::read(outside).unwrap(), b"user data");
}

#[test]
fn learning_import_ops_duplicate_reuses_note_and_raw_archive() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let source = profile(&state);
    let raw = b"# Same source\r\n";
    for key in [b"first-request".as_slice(), b"second-request".as_slice()] {
        let batch =
            import_ops::create_batch(&state, &sc, &source.id, &source.access_epoch, &sha(key))
                .unwrap();
        add_staged(&state, &sc, &batch.id, "vault", "same.md", raw, "same");
        import_ops::commit_batch(&state, &sc, &batch.id).unwrap();
    }
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM knowledge_notes", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_note_archives", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}

#[cfg(windows)]
#[test]
fn learning_import_ops_rejects_hardlinked_staged_file() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("library");
    let state = test_state(&library);
    let sc = scope(&state);
    let source = profile(&state);
    let raw = b"# source\n";
    let batch = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"hardlinked-stage"),
    )
    .unwrap();
    let item = add_staged(&state, &sc, &batch.id, "vault", "a.md", raw, "source");
    let staged = library.join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, item.id));
    let outside = temp.path().join("source.bin");
    std::fs::write(&outside, raw).unwrap();
    std::fs::remove_file(&staged).unwrap();
    std::fs::hard_link(&outside, &staged).unwrap();
    assert!(import_ops::commit_batch(&state, &sc, &batch.id).is_err());
    assert_eq!(std::fs::read(outside).unwrap(), raw);
}

#[cfg(windows)]
#[test]
fn learning_import_ops_cancel_keeps_recoverable_state_when_stage_is_locked() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
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
        &sha(b"locked-stage"),
    )
    .unwrap();
    let item = add_staged(
        &state, &sc, &batch.id, "vault", "a.md", b"private", "source",
    );
    let staged = library.join(format!(".dcc/runtime/imports/{}/{}.md", batch.id, item.id));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&staged)
        .unwrap();
    assert!(import_ops::cancel_batch(&state, &sc, &batch.id).is_err());
    let pending = import_ops::get_item(&state, &sc, &item.id).unwrap();
    assert_eq!(pending.status, "staged");
    assert!(pending.stage_relative.is_some());
    assert!(staged.exists());
    drop(lock);
    assert_eq!(
        import_ops::cancel_batch(&state, &sc, &batch.id)
            .unwrap()
            .status,
        "cancelled"
    );
    assert!(!staged.exists());
}

#[test]
fn learning_import_ops_conflict_preserves_new_raw_without_overwriting_note() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let sc = scope(&state);
    let source = profile(&state);
    let first = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"first-conflict"),
    )
    .unwrap();
    let created = add_staged(
        &state, &sc, &first.id, "vault", "same.md", b"# old\n", "old",
    );
    import_ops::commit_batch(&state, &sc, &first.id).unwrap();
    let note_id = import_ops::get_item(&state, &sc, &created.id)
        .unwrap()
        .target_id
        .unwrap();
    state
        .with_library_mut(|db, _| {
            let note = knowledge::get(db, &note_id)?;
            knowledge::save(
                db,
                NoteInput {
                    id: Some(note.id),
                    expected_revision: Some(note.revision),
                    notebook_id: note.notebook_id,
                    title: "user edit".into(),
                    document: note.document,
                    tags: note.tags,
                    favorite: note.favorite,
                    journal_date: note.journal_date,
                },
            )?;
            Ok(())
        })
        .unwrap();
    let second = import_ops::create_batch(
        &state,
        &sc,
        &source.id,
        &source.access_epoch,
        &sha(b"second-conflict"),
    )
    .unwrap();
    let raw = b"# new source\r\n";
    let changed = add_staged(&state, &sc, &second.id, "vault", "same.md", raw, "new");
    import_ops::commit_batch(&state, &sc, &second.id).unwrap();
    assert_eq!(
        import_ops::get_item(&state, &sc, &changed.id)
            .unwrap()
            .status,
        "conflict"
    );
    state
        .with_library(|db, base| {
            assert_eq!(knowledge::get(db, &note_id)?.title, "user edit");
            let receipt: String = db
                .query_row(
                    "SELECT receipt_json FROM learning_import_items WHERE id=?1",
                    [&changed.id],
                    |r| r.get(0),
                )
                .unwrap();
            let receipt: serde_json::Value = serde_json::from_str(&receipt).unwrap();
            let archive = receipt["archiveId"].as_str().unwrap();
            assert_eq!(super::note_archive::read_archive(db, base, archive)?, raw);
            let page = super::note_archive_browser::list_archives(db, 0, 50)?;
            assert_eq!(
                page.versions
                    .items
                    .iter()
                    .find(|row| row.id == archive)
                    .unwrap()
                    .conversion_state,
                "unconverted"
            );
            Ok(())
        })
        .unwrap();
}
