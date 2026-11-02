use super::{
    excerpt_publish::{CommitRequest, Destination},
    excerpt_runner::*,
    excerpt_store,
};
use crate::tasks::TaskRequest;
#[test]
fn learning_excerpt_runner_prepare_uses_shared_task_and_returns_only_preview_identity() {
    let (_root, state, library, course, selection) = super::excerpt_runtime_tests::fixture(false);
    let request = ExcerptTaskRequest::Prepare {
        course_id: course,
        selection,
    };
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::CourseExcerpt(request.clone()))
        .unwrap();
    let value = run_local(&state, &task, &request, None).unwrap();
    let id = value["previewId"].as_str().unwrap();
    assert!(value.get("document").is_none());
    assert!(value.get("frame").is_none());
    state
        .with_library(|db, base| {
            assert_eq!(
                excerpt_store::get_preview(db, base, &library, id)?.status,
                "ready"
            );
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
fn learning_excerpt_runner_commit_reuses_the_reliable_receipt_without_a_second_queue() {
    let (_root, state, library, prepared) = super::excerpt_store_tests::fixture(false);
    let preview = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let request = ExcerptTaskRequest::Commit {
        request: CommitRequest {
            operation_id: uuid::Uuid::new_v4().to_string(),
            excerpt_id: preview.id,
            destination: Destination::New {
                title: "后台摘录".into(),
                notebook_id: None,
            },
            include_frame: false,
        },
    };
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::CourseExcerpt(request.clone()))
        .unwrap();
    let value = run_local(&state, &task, &request, None).unwrap();
    assert!(value["receipt"]["noteId"].is_string());
    let repeat = run_local(&state, &task, &request, None).unwrap();
    assert_eq!(value, repeat);
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM knowledge_notes", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn learning_excerpt_runner_cancel_wrong_library_and_input_budget_do_not_publish() {
    let (_root, state, library, course, selection) = super::excerpt_runtime_tests::fixture(false);
    let mut request = ExcerptTaskRequest::Prepare {
        course_id: course,
        selection,
    };
    let (task, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::CourseExcerpt(request.clone()))
        .unwrap();
    state.tasks.cancel(&task.id).unwrap();
    assert!(run_local(&state, &task, &request, None).is_err());
    let (foreign, _) = state
        .tasks
        .submit(
            uuid::Uuid::new_v4().to_string(),
            TaskRequest::CourseExcerpt(request.clone()),
        )
        .unwrap();
    assert!(run_local(&state, &foreign, &request, None).is_err());
    if let ExcerptTaskRequest::Prepare { selection, .. } = &mut request {
        selection.comment = "x".repeat(20001);
    }
    assert!(state
        .tasks
        .submit(library.clone(), TaskRequest::CourseExcerpt(request))
        .is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_excerpts", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn learning_excerpt_runner_identical_active_requests_share_task_and_restart_never_auto_runs() {
    let (root, state, library, course, selection) = super::excerpt_runtime_tests::fixture(false);
    let request = ExcerptTaskRequest::Prepare {
        course_id: course,
        selection,
    };
    let (first, _) = state
        .tasks
        .submit(library.clone(), TaskRequest::CourseExcerpt(request.clone()))
        .unwrap();
    let (second, launch) = state
        .tasks
        .submit(library.clone(), TaskRequest::CourseExcerpt(request))
        .unwrap();
    assert_eq!(first.id, second.id);
    assert!(!launch);
    drop(state);
    let state = crate::state::test_state(root.path());
    let tasks = state.tasks.list(&library).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, "interrupted");
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM learning_excerpts", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
}
