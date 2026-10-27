use super::{
    excerpt_board_source::get_in_library,
    excerpt_publish::{self, CommitRequest, Destination},
    excerpt_store, note_nodes,
};
use rusqlite::params;
fn fixture() -> (
    tempfile::TempDir,
    crate::state::AppState,
    String,
    String,
    String,
    String,
) {
    let (root, state, library, prepared) = super::excerpt_store_tests::fixture(true);
    let stored = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let board = state
        .with_library(|db, _| crate::reference_boards::create(db, "学习参考图"))
        .unwrap();
    let request = CommitRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        excerpt_id: stored.id,
        destination: Destination::Board {
            board_id: board.id.clone(),
            x: 0.0,
            y: 0.0,
        },
        include_frame: true,
    };
    let result = excerpt_publish::commit_in_library(&state, &library, &request, &|| false).unwrap();
    (
        root,
        state,
        library,
        board.id,
        result.receipt.board_item_id.unwrap(),
        request.operation_id,
    )
}
#[test]
fn learning_board_excerpt_returns_verified_exact_frame_and_rejects_wrong_scope() {
    let (root, state, library, board, item, _) = fixture();
    let reference = get_in_library(&state, &library, &board, &item)
        .unwrap()
        .unwrap();
    assert_eq!(reference.ranges.len(), 1);
    assert_eq!(reference.ranges[0].start_ms, 400);
    assert_eq!(reference.ranges[0].end_ms, 401);
    let jump = note_nodes::prepare_jump(&state, &library, &reference, 0).unwrap();
    assert_eq!(jump.position_ms, 400);
    assert!(get_in_library(&state, &uuid::Uuid::new_v4().to_string(), &board, &item).is_err());
    assert!(get_in_library(&state, &library, &uuid::Uuid::new_v4().to_string(), &item).is_err());
    assert!(get_in_library(&state, &library, &board, "../item").is_err());
    std::fs::write(
        root.path().join(format!(
            "media-library/video/{}/video.mp4",
            reference.media_id
        )),
        b"changed original",
    )
    .unwrap();
    assert!(note_nodes::prepare_jump(&state, &library, &reference, 0).is_err());
}
#[test]
fn learning_board_excerpt_preserves_image_when_history_is_damaged_or_unlinked() {
    let (root, state, library, board, item, operation) = fixture();
    let path = state
        .with_library(|db, base| {
            let relative: String = db
                .query_row(
                    "SELECT original_rel_path FROM reference_board_items WHERE id=?1",
                    [&item],
                    |r| r.get(0),
                )
                .unwrap();
            Ok(base.join(relative))
        })
        .unwrap();
    assert!(path.is_file());
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_excerpt_operations SET receipt_json='{}' WHERE operation_id=?1",
                [&operation],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(get_in_library(&state, &library, &board, &item).is_err());
    assert!(path.is_file());
    state
        .with_library(|db, _| {
            db.execute(
                "DELETE FROM learning_excerpt_board_items WHERE item_id=?1",
                [&item],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(get_in_library(&state, &library, &board, &item)
        .unwrap()
        .is_none());
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE reference_board_items SET deleted_at='trash' WHERE id=?1",
                params![item],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(get_in_library(&state, &library, &board, &item).is_err());
    assert!(root.path().exists());
}
#[test]
fn learning_board_excerpt_jump_requires_registered_main_file() {
    let (_root, state, library, board, item, _) = fixture();
    let reference = get_in_library(&state, &library, &board, &item)
        .unwrap()
        .unwrap();
    assert!(note_nodes::prepare_jump(&state, &library, &reference, 0).is_ok());
    state.with_library(|db,_|{db.execute("UPDATE media_files SET role='proxy' WHERE id=(SELECT primary_file_id FROM media_entries WHERE id=?1)",[&reference.media_id]).unwrap();Ok(())}).unwrap();
    assert!(note_nodes::prepare_jump(&state, &library, &reference, 0).is_err());
}
#[test]
fn learning_board_excerpt_existing_note_jump_rejects_non_main_file() {
    let (_root, state, library, course, selection) = super::excerpt_runtime_tests::fixture(false);
    let prepared = super::excerpt_runtime::prepare_in_library(
        &state,
        &library,
        &course,
        &selection,
        None,
        &|| false,
    )
    .unwrap();
    let reference = note_nodes::LearningReference {
        course_id: course,
        media_id: prepared.text.media_id,
        fingerprint: prepared.source_sha256,
        ranges: vec![note_nodes::TimeRange {
            start_ms: 0,
            end_ms: 1,
        }],
        label: "已保存引用".into(),
    };
    assert!(note_nodes::prepare_jump(&state, &library, &reference, 0).is_ok());
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE media_files SET role='proxy' WHERE id=?1",
                [&prepared.source_file_id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(note_nodes::prepare_jump(&state, &library, &reference, 0).is_err());
}
#[test]
fn learning_board_excerpt_rechecks_item_after_source_validation() {
    let (_root, state, library, board, item, _) = fixture();
    let first =
        super::excerpt_board_source::prepare_guarded(&state, &library, &board, &item, || Ok(()))
            .unwrap()
            .unwrap();
    assert_eq!(first.position_ms, 400);
    assert_eq!(first.end_ms, 401);
    assert_eq!(first.source_fingerprint.len(), 64);
    let changed =
        super::excerpt_board_source::prepare_guarded(&state, &library, &board, &item, || {
            state.with_library(|db, _| {
                db.execute(
                    "DELETE FROM learning_excerpt_board_items WHERE item_id=?1",
                    [&item],
                )
                .unwrap();
                Ok(())
            })
        });
    assert!(
        changed.is_err(),
        "source recheck must reject a removed item binding"
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM reference_board_items WHERE id=?1",
                    [&item],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}
