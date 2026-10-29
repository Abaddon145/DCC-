use super::{import_ops, import_runner::ImportTaskRequest, note_origins, source_profiles};
use crate::{knowledge, state::test_state, tasks::TaskRequest};
use serde_json::json;
use sha2::{Digest, Sha256};

fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
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
