use super::{
    excerpt_access::*,
    excerpt_publish::{self, CommitRequest, Destination},
    excerpt_store,
};
fn fixture() -> (
    tempfile::TempDir,
    crate::state::AppState,
    String,
    CommitRequest,
    String,
) {
    let (root, state, library, prepared) = super::excerpt_store_tests::fixture(false);
    let preview = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let request = CommitRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        excerpt_id: preview.id,
        destination: Destination::New {
            title: "可恢复的摘录".into(),
            notebook_id: None,
        },
        include_frame: false,
    };
    (root, state, library, request, prepared.text.course_id)
}
#[test]
fn learning_excerpt_access_preview_is_scoped_and_cancel_does_not_fall_back() {
    let (root, state, library, request, _) = fixture();
    assert_eq!(
        get_in_library(&state, &library, &request.excerpt_id)
            .unwrap()
            .status,
        "ready"
    );
    assert!(get_in_library(
        &state,
        &uuid::Uuid::new_v4().to_string(),
        &request.excerpt_id
    )
    .is_err());
    assert!(get_in_library(&state, &library, "../library.db").is_err());
    let marker = root.path().join("library.json");
    let bytes = std::fs::read(&marker).unwrap();
    std::fs::write(&marker, b"{}").unwrap();
    assert!(get_in_library(&state, &library, &request.excerpt_id).is_err());
    std::fs::write(marker, bytes).unwrap();
    cancel_preview_in_library(&state, &library, &request.excerpt_id).unwrap();
    assert_eq!(
        get_in_library(&state, &library, &request.excerpt_id)
            .unwrap()
            .status,
        "cancelled"
    );
}
#[test]
fn learning_excerpt_access_pending_operation_is_recovered_and_cancelled_by_identity_only() {
    let (_root, state, library, request, course) = fixture();
    assert!(
        excerpt_publish::commit_guarded(&state, &library, &request, &|| false, |_tx| Err(
            "pending fixture".into()
        ))
        .is_err()
    );
    let record = operation_in_library(&state, &library, &request.operation_id).unwrap();
    assert_eq!(record.status, "pending");
    assert!(record.preview_readable);
    assert_eq!(record.request, request);
    assert!(cancel_preview_in_library(&state, &library, &request.excerpt_id).is_err());
    let page = list_in_library(&state, &library, &course, 0, 20).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].operation_id, request.operation_id);
    assert_eq!(
        list_in_library(&state, &library, &uuid::Uuid::new_v4().to_string(), 0, 20)
            .unwrap()
            .total,
        0
    );
    assert!(list_in_library(&state, &library, &course, -1, 20).is_err());
    assert!(list_in_library(&state, &library, &course, 0, 51).is_err());
    assert!(list_in_library(&state, &uuid::Uuid::new_v4().to_string(), &course, 0, 20).is_err());
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_excerpts SET payload_json='{}' WHERE id=?1",
                [&request.excerpt_id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    let damaged = operation_in_library(&state, &library, &request.operation_id).unwrap();
    assert!(damaged.receipt.is_none());
    assert!(!damaged.preview_readable);
    assert!(damaged.warning.unwrap().contains("未提交"));
    cancel_operation_in_library(&state, &library, &request.operation_id).unwrap();
    assert_eq!(
        operation_in_library(&state, &library, &request.operation_id)
            .unwrap()
            .status,
        "cancelled"
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
}
#[test]
fn learning_excerpt_access_corrupt_source_does_not_hide_committed_receipt_or_enable_cancel() {
    let (_root, state, library, request, _) = fixture();
    let saved = excerpt_publish::commit_in_library(&state, &library, &request, &|| false).unwrap();
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_excerpts SET payload_json='{}' WHERE id=?1",
                [&request.excerpt_id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    let record = operation_in_library(&state, &library, &request.operation_id).unwrap();
    assert_eq!(record.receipt, Some(saved.receipt));
    assert!(!record.preview_readable);
    assert!(record.warning.is_some());
    assert!(cancel_operation_in_library(&state, &library, &request.operation_id).is_err());
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
fn learning_excerpt_access_list_retains_bad_rows_without_breaking_other_receipts() {
    let (_root, state, library, mut request, course) = fixture();
    excerpt_publish::commit_in_library(&state, &library, &request, &|| false).unwrap();
    let old = request.operation_id.clone();
    let preview = state
        .with_library(|db, base| {
            let stored = excerpt_store::get_preview(db, base, &library, &request.excerpt_id)?;
            excerpt_store::save_prepared(db, base, &library, &stored.prepared)
        })
        .unwrap();
    request.operation_id = uuid::Uuid::new_v4().to_string();
    request.excerpt_id = preview.id;
    assert!(
        excerpt_publish::commit_guarded(&state, &library, &request, &|| false, |_tx| Err(
            "pending fixture".into()
        ))
        .is_err()
    );
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_excerpt_operations SET request_json='{' WHERE operation_id=?1",
                [&request.operation_id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    let page = list_in_library(&state, &library, &course, 0, 20).unwrap();
    assert_eq!(page.total, 2);
    assert!(page
        .items
        .iter()
        .any(|r| r.operation_id == old && r.receipt.is_some()));
    assert!(page
        .items
        .iter()
        .any(|r| r.operation_id == request.operation_id
            && r.status == "unreadable"
            && r.error.is_some()));
    assert!(operation_in_library(&state, &library, &request.operation_id).is_err());
    assert!(cancel_operation_in_library(&state, &library, &request.operation_id).is_err());
}
#[test]
fn learning_excerpt_access_targets_are_read_only_and_exclude_archived_projects() {
    let (_root, state, library, _request, _) = fixture();
    state.with_library(|db,_|{let board=crate::reference_boards::create(db,"现有参考板")?;db.execute("INSERT INTO projects(id,name,description,project_type,status,created_at,updated_at) VALUES(?1,'归档项目','','scene','archived','now','now')",[uuid::Uuid::new_v4().to_string()]).unwrap();assert!(!board.id.is_empty());Ok(())}).unwrap();
    let targets = targets_in_library(&state, &library).unwrap();
    assert_eq!(targets.boards.len(), 1);
    assert!(targets.projects.is_empty());
    assert!(targets_in_library(&state, &uuid::Uuid::new_v4().to_string()).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM knowledge_notes", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM reference_board_items", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
}
