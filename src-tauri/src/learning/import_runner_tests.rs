use super::{
    import_ops, import_runner::ImportTaskRequest, import_runner::PrepareMarkdownItem, note_origins,
    source_profiles,
};
use crate::{knowledge, state::test_state, tasks::TaskRequest};
use serde_json::json;
use sha2::{Digest, Sha256};

fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[test]
fn learning_import_runner_course_cancel_is_scoped_and_rejects_notes() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let batch = super::course_batch::create_course_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &sha("course-task"),
    )
    .unwrap();
    let request: ImportTaskRequest =
        serde_json::from_value(json!({"action":"cancelCourse","batchId":batch})).unwrap();
    let (task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(request.clone()),
        )
        .unwrap();
    let value = super::import_runner::run_local(&state, &task, &request).unwrap();
    assert_eq!(value["batchId"], batch);
    assert_eq!(value["status"], "cancelled");
    let note = import_ops::create_batch(
        &state,
        &scope,
        &profile.id,
        &profile.access_epoch,
        &sha("note-task"),
    )
    .unwrap();
    let wrong: ImportTaskRequest =
        serde_json::from_value(json!({"action":"cancelCourse","batchId":note.id})).unwrap();
    let (task, _) = state
        .tasks
        .submit(library, TaskRequest::LearningImport(wrong.clone()))
        .unwrap();
    assert!(super::import_runner::run_local(&state, &task, &wrong).is_err());
    assert_eq!(
        import_ops::list_batches(&state, &scope).unwrap()[0].status,
        "pending"
    );
}

fn doc() -> serde_json::Value {
    json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"任务迁入"}]}]})
}

fn prepare_request(
    profile: &source_profiles::SourceProfile,
    request_tag: &str,
    path: &str,
    markdown: &str,
) -> ImportTaskRequest {
    ImportTaskRequest::Prepare {
        profile_id: profile.id.clone(),
        epoch: profile.access_epoch.clone(),
        request_sha256: sha(request_tag),
        vault: "vault-task".into(),
        relative_path: path.into(),
        source_sha256: sha(markdown),
        title: "任务迁入笔记".into(),
        document: doc(),
        markdown: markdown.into(),
        items: Vec::new(),
    }
}

#[test]
fn learning_import_runner_prepare_commit_via_task_registry_without_overwrite_on_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let markdown = "# task import\n";
    let prepare = prepare_request(&profile, "runner-commit", "notes/task.md", markdown);
    let (task, launch) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(prepare.clone()),
        )
        .unwrap();
    assert!(launch);
    let prepared = super::import_runner::run_local(&state, &task, &prepare).unwrap();
    let batch_id = prepared["batchId"].as_str().unwrap().to_string();
    let item_id = prepared["itemId"].as_str().unwrap().to_string();
    assert_eq!(prepared["status"], "staged");

    let commit = ImportTaskRequest::Commit {
        batch_id: batch_id.clone(),
    };
    let (commit_task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::LearningImport(commit.clone()))
        .unwrap();
    let committed = super::import_runner::run_local(&state, &commit_task, &commit).unwrap();
    assert_eq!(committed["status"], "committed");

    let item = import_ops::get_item(&state, &scope, &item_id).unwrap();
    assert_eq!(item.status, "committed");
    let note_id = item.target_id.clone().unwrap();
    let note = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_id)?))
        .unwrap();
    assert_eq!(note.title, "任务迁入笔记");

    state
        .with_library_mut(|db, _| {
            knowledge::save(
                db,
                knowledge::NoteInput {
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
    let conflict_prepare = prepare_request(&profile, "runner-conflict", "notes/task.md", markdown);
    let (p2, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(conflict_prepare.clone()),
        )
        .unwrap();
    let prepared2 = super::import_runner::run_local(&state, &p2, &conflict_prepare).unwrap();
    let conflict_commit = ImportTaskRequest::Commit {
        batch_id: prepared2["batchId"].as_str().unwrap().into(),
    };
    let (c2, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(conflict_commit.clone()),
        )
        .unwrap();
    let result = super::import_runner::run_local(&state, &c2, &conflict_commit).unwrap();
    assert_eq!(result["status"], "committed");
    let conflict_item =
        import_ops::get_item(&state, &scope, prepared2["itemId"].as_str().unwrap()).unwrap();
    assert_eq!(conflict_item.status, "conflict");
    assert_eq!(
        conflict_item.conflict_code.as_deref(),
        Some("target_changed")
    );
    let after = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_id)?))
        .unwrap();
    assert_eq!(after.title, "用户已改");
}

#[test]
fn learning_import_runner_dedup_cancel_prepare_and_cancel_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let prepare = prepare_request(&profile, "runner-dedup", "a.md", "dedup-md");
    let (first, launch1) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(prepare.clone()),
        )
        .unwrap();
    let (second, launch2) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(prepare.clone()),
        )
        .unwrap();
    assert_eq!(first.id, second.id);
    assert!(launch1);
    assert!(!launch2);
    state.tasks.cancel(&first.id).unwrap();
    assert!(super::import_runner::run_local(&state, &first, &prepare).is_err());

    let prepare2 = prepare_request(&profile, "runner-cancel-batch", "b.md", "cancel-body");
    let (prep_task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(prepare2.clone()),
        )
        .unwrap();
    let prepared = super::import_runner::run_local(&state, &prep_task, &prepare2).unwrap();
    let batch_id = prepared["batchId"].as_str().unwrap().to_string();
    let cancel = ImportTaskRequest::Cancel { batch_id };
    let (cancel_task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::LearningImport(cancel.clone()))
        .unwrap();
    let cancelled = super::import_runner::run_local(&state, &cancel_task, &cancel).unwrap();
    assert_eq!(cancelled["status"], "cancelled");
    let key = note_origins::NoteOriginKey::new(&profile.id, "vault-task", "b.md").unwrap();
    let mapped = state
        .with_library(|db, _| note_origins::inspect(db, &key, &sha("cancel-body")))
        .unwrap();
    assert_eq!(mapped, note_origins::NoteOriginState::Unmapped);
}

#[test]
fn learning_import_runner_rejects_bad_payload_before_queue() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let bad = ImportTaskRequest::Prepare {
        profile_id: "not-a-uuid".into(),
        epoch: "also-bad".into(),
        request_sha256: "00".into(),
        vault: "v".into(),
        relative_path: "x.md".into(),
        source_sha256: "11".into(),
        title: "t".into(),
        document: doc(),
        markdown: "x".into(),
        items: Vec::new(),
    };
    assert!(state
        .tasks
        .submit(library, TaskRequest::LearningImport(bad))
        .is_err());
}

#[test]
fn learning_import_runner_replays_committed_batch_receipt_without_restaging() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let prepare = prepare_request(&profile, "runner-replay", "replay.md", "# replay\n");
    let (task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(prepare.clone()),
        )
        .unwrap();
    let prepared = super::import_runner::run_local(&state, &task, &prepare).unwrap();
    let batch_id = prepared["batchId"].as_str().unwrap().to_string();
    let commit = ImportTaskRequest::Commit {
        batch_id: batch_id.clone(),
    };
    let (commit_task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::LearningImport(commit.clone()))
        .unwrap();
    let committed = super::import_runner::run_local(&state, &commit_task, &commit).unwrap();
    assert_eq!(committed["status"], "committed");
    // Retire the original prepare task so the same request can be submitted again.
    state.tasks.cancel(&task.id).unwrap();

    let (replay_task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(prepare.clone()),
        )
        .unwrap();
    assert_ne!(replay_task.id, task.id);
    let replay = super::import_runner::run_local(&state, &replay_task, &prepare).unwrap();
    assert_eq!(replay["batchId"], batch_id);
    assert_eq!(replay["status"], "committed");
    assert_eq!(replay["receipt"], committed["receipt"]);
    assert!(!replay["receipt"].is_null());
    let items = import_ops::list_items(&state, &scope, &batch_id).unwrap();
    assert_eq!(
        items.len(),
        1,
        "committed replay must not add or stage items"
    );
}

#[test]
fn learning_import_runner_reports_the_actual_terminal_batch_state() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    for (tag, first_action, second_action, expected_state, expected_phase) in [
        (
            "first-cancel",
            "cancel",
            "commit",
            "cancelled",
            "批次已取消，未提交的条目未写入，已提交内容保留",
        ),
        (
            "first-commit",
            "commit",
            "cancel",
            "committed",
            "批次已提交，无法取消；已提交内容保留",
        ),
    ] {
        let prepare = prepare_request(&profile, tag, &format!("{tag}.md"), "# 内容\n");
        let (task, _) = state
            .tasks
            .submit(
                library.clone(),
                TaskRequest::LearningImport(prepare.clone()),
            )
            .unwrap();
        let prepared = super::import_runner::run_local(&state, &task, &prepare).unwrap();
        let batch_id = prepared["batchId"].as_str().unwrap().to_string();
        let action = |name| {
            if name == "commit" {
                ImportTaskRequest::Commit {
                    batch_id: batch_id.clone(),
                }
            } else {
                ImportTaskRequest::Cancel {
                    batch_id: batch_id.clone(),
                }
            }
        };
        let first = action(first_action);
        let (first_task, _) = state
            .tasks
            .submit(library.clone(), TaskRequest::LearningImport(first.clone()))
            .unwrap();
        super::import_runner::run_local(&state, &first_task, &first).unwrap();
        let second = action(second_action);
        let (second_task, _) = state
            .tasks
            .submit(library.clone(), TaskRequest::LearningImport(second.clone()))
            .unwrap();
        let result = super::import_runner::run_local(&state, &second_task, &second).unwrap();
        assert_eq!(result["status"], expected_state);
        assert_eq!(
            state.tasks.get(&second_task.id).unwrap().phase,
            expected_phase
        );
    }
}

#[test]
fn learning_import_runner_queues_revert_and_returns_its_durable_report() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let prepare = prepare_request(&profile, "runner-revert", "revert.md", "# revert\n");
    let (task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(prepare.clone()),
        )
        .unwrap();
    let staged = super::import_runner::run_local(&state, &task, &prepare).unwrap();
    let batch_id = staged["batchId"].as_str().unwrap().to_string();
    let commit = ImportTaskRequest::Commit {
        batch_id: batch_id.clone(),
    };
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::LearningImport(commit.clone()))
        .unwrap();
    super::import_runner::run_local(&state, &task, &commit).unwrap();
    let note_id = import_ops::list_items(&state, &scope, &batch_id).unwrap()[0]
        .target_id
        .clone()
        .unwrap();
    let revert: ImportTaskRequest = serde_json::from_value(
        json!({"action":"revert","batchId":batch_id,"expectedEpoch":profile.access_epoch}),
    )
    .unwrap();
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::LearningImport(revert.clone()))
        .unwrap();
    let result = super::import_runner::run_local(&state, &task, &revert).unwrap();
    assert_eq!(result["status"], "reverted");
    assert_eq!(result["report"]["reverted"], json!([note_id]));
    let note = state
        .with_library(|db, _| knowledge::get(db, &note_id))
        .unwrap();
    assert!(note.deleted_at.is_some());
    #[cfg(windows)]
    {
        let batches = super::import_access::list_batches_in_library(&state, &library).unwrap();
        let batch = batches.iter().find(|batch| batch.id == batch_id).unwrap();
        assert_eq!(
            serde_json::to_value(batch).unwrap()["revertReport"],
            result["report"]
        );
    }
}

#[test]
fn learning_import_runner_rejects_revert_confirmed_before_backup_restore() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let prepare = prepare_request(&profile, "stale-revert", "stale.md", "# stale\n");
    let (task, _) = state
        .tasks
        .submit(
            library.clone(),
            TaskRequest::LearningImport(prepare.clone()),
        )
        .unwrap();
    let staged = super::import_runner::run_local(&state, &task, &prepare).unwrap();
    let batch_id = staged["batchId"].as_str().unwrap().to_string();
    let commit = ImportTaskRequest::Commit {
        batch_id: batch_id.clone(),
    };
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::LearningImport(commit.clone()))
        .unwrap();
    super::import_runner::run_local(&state, &task, &commit).unwrap();
    let note_id = import_ops::list_items(&state, &scope, &batch_id).unwrap()[0]
        .target_id
        .clone()
        .unwrap();
    let backup = temp.path().join("backup.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    state.restore_backup_for_task(&backup).unwrap();
    let stale: ImportTaskRequest = serde_json::from_value(
        json!({"action":"revert","batchId":batch_id,"expectedEpoch":profile.access_epoch}),
    )
    .unwrap();
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::LearningImport(stale.clone()))
        .unwrap();
    assert!(super::import_runner::run_local(&state, &task, &stale).is_err());
    assert!(state
        .with_library(|db, _| knowledge::get(db, &note_id))
        .unwrap()
        .deleted_at
        .is_none());
    let epoch = state
        .with_library(|db, _| Ok(source_profiles::get_in(db, &profile.id)?.access_epoch))
        .unwrap();
    assert_ne!(epoch, profile.access_epoch);
    let fresh: ImportTaskRequest =
        serde_json::from_value(json!({"action":"revert","batchId":batch_id,"expectedEpoch":epoch}))
            .unwrap();
    let (task, _) = state
        .tasks
        .submit(library, TaskRequest::LearningImport(fresh.clone()))
        .unwrap();
    assert_eq!(
        super::import_runner::run_local(&state, &task, &fresh).unwrap()["status"],
        "reverted"
    );
    state.with_library(|db, base| {
        let archive: String = db.query_row("SELECT json_extract(receipt_json,'$.archiveId') FROM learning_import_items WHERE batch_id=?1", [&batch_id], |row| row.get(0)).map_err(|error| error.to_string())?;
        assert_eq!(super::note_archive::read_archive(db, base, &archive)?, b"# stale\n");
        Ok(())
    }).unwrap();
}

fn item_path(item: &import_ops::ImportItem) -> String {
    let parts: Vec<String> = serde_json::from_str(&item.source_key_json).unwrap();
    parts[3].clone()
}

fn prepare_many(
    profile: &source_profiles::SourceProfile,
    request_tag: &str,
    notes: &[(&str, &str, &str)],
) -> ImportTaskRequest {
    let items = notes
        .iter()
        .map(|(path, title, markdown)| PrepareMarkdownItem {
            relative_path: (*path).into(),
            source_sha256: sha(markdown),
            title: (*title).into(),
            document: doc(),
            markdown: (*markdown).into(),
        })
        .collect();
    ImportTaskRequest::Prepare {
        profile_id: profile.id.clone(),
        epoch: profile.access_epoch.clone(),
        request_sha256: sha(request_tag),
        vault: "vault-task".into(),
        relative_path: String::new(),
        source_sha256: String::new(),
        title: String::new(),
        document: serde_json::Value::Null,
        markdown: String::new(),
        items,
    }
}

fn submit_and_run(
    state: &crate::state::AppState,
    library: &str,
    request: &ImportTaskRequest,
) -> serde_json::Value {
    let (task, _) = state
        .tasks
        .submit(library.into(), TaskRequest::LearningImport(request.clone()))
        .unwrap();
    super::import_runner::run_local(state, &task, request).unwrap()
}

#[test]
fn learning_import_runner_single_note_serde_and_prepare_stay_one_item() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let prepare = prepare_request(&profile, "still-single", "notes/only.md", "# only\n");
    let saved = serde_json::to_value(&prepare).unwrap();
    assert!(saved.get("items").is_none());
    let restored: ImportTaskRequest = serde_json::from_value(saved).unwrap();
    assert_eq!(restored, prepare);
    let prepared = submit_and_run(&state, &library, &prepare);
    assert_eq!(prepared["status"], "staged");
    let batch_id = prepared["batchId"].as_str().unwrap();
    let items = import_ops::list_items(&state, &scope, batch_id).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(item_path(&items[0]), "notes/only.md");
    assert_eq!(prepared["itemId"].as_str(), Some(items[0].id.as_str()));
}

#[test]
fn learning_import_runner_rejects_duplicate_notes_in_one_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let prepare = prepare_many(
        &profile,
        "batch-duplicate",
        &[("Notes/A.md", "甲", "# A\n"), ("notes/a.md", "乙", "# A\n")],
    );
    let error = match state
        .tasks
        .submit(library, TaskRequest::LearningImport(prepare))
    {
        Err(error) => error,
        Ok(_) => panic!("duplicate notes must not enter the queue"),
    };
    assert!(
        error.contains("重复笔记"),
        "case-insensitive duplicate must be rejected, got {error}"
    );
    assert!(import_ops::list_batches(&state, &scope).unwrap().is_empty());
}

#[test]
fn learning_import_runner_prepares_several_notes_into_one_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let prepare = prepare_many(
        &profile,
        "batch-two",
        &[
            ("notes/a.md", "笔记A", "# A\n"),
            ("notes/b.md", "笔记B", "# B\n"),
        ],
    );
    let prepared = submit_and_run(&state, &library, &prepare);
    assert_eq!(prepared["status"], "staged");
    let batch_id = prepared["batchId"].as_str().unwrap();
    assert_eq!(prepared["itemIds"].as_array().unwrap().len(), 2);
    let items = import_ops::list_items(&state, &scope, batch_id).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items
        .iter()
        .all(|item| item.batch_id == batch_id && item.status == "staged"));
    let paths: Vec<_> = items.iter().map(item_path).collect();
    assert_eq!(
        paths,
        vec!["notes/a.md".to_string(), "notes/b.md".to_string()]
    );
    let batches = import_ops::list_batches(&state, &scope).unwrap();
    assert_eq!(
        batches
            .iter()
            .filter(|batch| batch.status == "pending")
            .count(),
        1
    );
}

#[test]
fn learning_import_runner_rejects_out_of_root_note_in_a_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let prepare = prepare_many(
        &profile,
        "batch-escape",
        &[
            ("notes/inside.md", "内部", "# in\n"),
            ("../outside.md", "越界", "# out\n"),
        ],
    );
    let error = match state
        .tasks
        .submit(library, TaskRequest::LearningImport(prepare))
    {
        Err(error) => error,
        Ok(_) => panic!("out-of-root prepare must not enter the queue"),
    };
    assert!(
        error.contains("笔记来源身份无效"),
        "out-of-root relative path must be rejected before queue, got {error}"
    );
    assert!(import_ops::list_batches(&state, &scope).unwrap().is_empty());
}

#[test]
fn learning_import_runner_conflict_item_does_not_overwrite_when_batch_commits() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let original = "# 原文\n";
    let first = prepare_request(&profile, "seed-note", "notes/a.md", original);
    let seeded = submit_and_run(&state, &library, &first);
    let commit = ImportTaskRequest::Commit {
        batch_id: seeded["batchId"].as_str().unwrap().into(),
    };
    submit_and_run(&state, &library, &commit);
    let seeded_item =
        import_ops::get_item(&state, &scope, seeded["itemId"].as_str().unwrap()).unwrap();
    let note_id = seeded_item.target_id.clone().unwrap();
    state
        .with_library_mut(|db, _| {
            let note = knowledge::get(db, &note_id)?;
            knowledge::save(
                db,
                knowledge::NoteInput {
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
    let prepare = prepare_many(
        &profile,
        "batch-conflict",
        &[
            ("notes/a.md", "笔记A", original),
            ("notes/b.md", "笔记B", "# 新笔记\n"),
        ],
    );
    let prepared = submit_and_run(&state, &library, &prepare);
    let batch_id = prepared["batchId"].as_str().unwrap().to_string();
    let commit = ImportTaskRequest::Commit {
        batch_id: batch_id.clone(),
    };
    let committed = submit_and_run(&state, &library, &commit);
    assert_eq!(committed["status"], "committed");
    let items = import_ops::list_items(&state, &scope, &batch_id).unwrap();
    let conflict = items
        .iter()
        .find(|item| item_path(item) == "notes/a.md")
        .unwrap();
    let created = items
        .iter()
        .find(|item| item_path(item) == "notes/b.md")
        .unwrap();
    assert_eq!(conflict.status, "conflict");
    assert_eq!(conflict.conflict_code.as_deref(), Some("target_changed"));
    assert_eq!(created.status, "committed");
    let kept = state
        .with_library(|db, _| Ok(knowledge::get(db, &note_id)?))
        .unwrap();
    assert_eq!(kept.title, "用户已改");
    let fresh = state
        .with_library(|db, _| Ok(knowledge::get(db, created.target_id.as_deref().unwrap())?))
        .unwrap();
    assert_eq!(fresh.title, "笔记B");
}

#[test]
fn learning_import_runner_cancel_commit_and_revert_a_multi_note_batch() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state(&temp.path().join("library"));
    let library = state.active_library_id().unwrap();
    let scope = source_profiles::scope(&state, &library).unwrap();
    let profile = source_profiles::create(&state, &scope).unwrap();
    let cancelled = prepare_many(
        &profile,
        "batch-cancel",
        &[
            ("notes/c1.md", "取消一", "# c1\n"),
            ("notes/c2.md", "取消二", "# c2\n"),
        ],
    );
    let staged = submit_and_run(&state, &library, &cancelled);
    let cancel = ImportTaskRequest::Cancel {
        batch_id: staged["batchId"].as_str().unwrap().into(),
    };
    let cancelled_result = submit_and_run(&state, &library, &cancel);
    assert_eq!(cancelled_result["status"], "cancelled");
    for path in ["notes/c1.md", "notes/c2.md"] {
        let key = note_origins::NoteOriginKey::new(&profile.id, "vault-task", path).unwrap();
        let mapped = state
            .with_library(|db, _| note_origins::inspect(db, &key, &sha("# unused\n")))
            .unwrap();
        assert_eq!(mapped, note_origins::NoteOriginState::Unmapped);
    }
    let notes: i64 = state
        .with_library(|db, _| {
            db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |row| row.get(0))
                .map_err(|error| error.to_string())
        })
        .unwrap();
    assert_eq!(notes, 0);

    let prepare = prepare_many(
        &profile,
        "batch-revert",
        &[
            ("notes/r1.md", "撤回一", "# r1\n"),
            ("notes/r2.md", "撤回二", "# r2\n"),
        ],
    );
    let staged = submit_and_run(&state, &library, &prepare);
    let batch_id = staged["batchId"].as_str().unwrap().to_string();
    let commit = ImportTaskRequest::Commit {
        batch_id: batch_id.clone(),
    };
    assert_eq!(
        submit_and_run(&state, &library, &commit)["status"],
        "committed"
    );
    let items = import_ops::list_items(&state, &scope, &batch_id).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.status == "committed"));
    let revert = ImportTaskRequest::Revert {
        batch_id: batch_id.clone(),
        expected_epoch: profile.access_epoch.clone(),
    };
    let reverted = submit_and_run(&state, &library, &revert);
    assert_eq!(reverted["status"], "reverted");
    let trashed = reverted["report"]["reverted"].as_array().unwrap();
    assert_eq!(trashed.len(), 2);
    for item in &items {
        let note_id = item.target_id.as_deref().unwrap();
        assert!(trashed.iter().any(|value| value.as_str() == Some(note_id)));
        let note = state
            .with_library(|db, _| knowledge::get(db, note_id))
            .unwrap();
        assert!(note.deleted_at.is_some());
    }
    assert_eq!(reverted["report"]["archivesKept"], true);
}
