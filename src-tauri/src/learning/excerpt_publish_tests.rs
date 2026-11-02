use super::{excerpt_publish::*, excerpt_runtime, excerpt_store};
use crate::{
    knowledge::{self, NoteInput},
    state::AppState,
};
use rusqlite::params;
use serde_json::json;
fn fixture() -> (tempfile::TempDir, AppState, String, CommitRequest) {
    let (root, state, library, course, selection) = super::excerpt_runtime_tests::fixture(false);
    let prepared =
        excerpt_runtime::prepare_in_library(&state, &library, &course, &selection, None, &|| false)
            .unwrap();
    let preview = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let request = CommitRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        excerpt_id: preview.id,
        destination: Destination::New {
            title: "学习摘录".into(),
            notebook_id: None,
        },
        include_frame: false,
    };
    (root, state, library, request)
}
fn counts(state: &AppState) -> (i64, i64, i64) {
    state
        .with_library(|db, _| {
            Ok((
                db.query_row("SELECT count(*) FROM knowledge_notes", [], |r| r.get(0))
                    .unwrap(),
                db.query_row(
                    "SELECT count(*) FROM learning_excerpt_operations WHERE status='pending'",
                    [],
                    |r| r.get(0),
                )
                .unwrap(),
                db.query_row(
                    "SELECT count(*) FROM learning_excerpt_operations WHERE status='committed'",
                    [],
                    |r| r.get(0),
                )
                .unwrap(),
            ))
        })
        .unwrap()
}
#[test]
fn learning_excerpt_publish_replays_one_receipt_after_restart_source_change_and_target_deletion() {
    let (root, state, library, request) = fixture();
    let result = commit_in_library(&state, &library, &request, &|| false).unwrap();
    assert!(result.target_available);
    assert_eq!(counts(&state), (1, 0, 1));
    let repeat = commit_in_library(&state, &library, &request, &|| false).unwrap();
    assert_eq!(repeat.receipt, result.receipt);
    std::fs::write(
        root.path().join("fixture.mp4"),
        b"changed after successful publication",
    )
    .unwrap();
    state
        .with_library(|db, _| {
            db.execute(
                "DELETE FROM knowledge_notes WHERE id=?1",
                [&result.receipt.note_id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    drop(state);
    let state = crate::state::test_state(root.path());
    let replay = commit_in_library(&state, &library, &request, &|| false).unwrap();
    assert_eq!(replay.receipt, result.receipt);
    assert!(!replay.target_available);
    assert_eq!(counts(&state), (0, 0, 1));
    let mut different = request.clone();
    different.destination = Destination::New {
        title: "不得更换原请求".into(),
        notebook_id: None,
    };
    assert!(commit_in_library(&state, &library, &different, &|| false).is_err());
    different = request.clone();
    different.operation_id = uuid::Uuid::new_v4().to_string();
    assert!(commit_in_library(&state, &library, &different, &|| false).is_err());
}
#[test]
fn learning_excerpt_publish_append_preserves_body_metadata_color_and_rejects_stale_revision() {
    let (_root, state, library, mut request) = fixture();
    let note=state.with_library_mut(|db,_|knowledge::save(db,NoteInput{id:None,expected_revision:None,notebook_id:None,title:"已有学习笔记".into(),document:json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"原文前段"}]},{"type":"paragraph","content":[{"type":"text","text":"原文后段"}]}]}),tags:vec!["UE5".into()],favorite:true,journal_date:None})).unwrap();
    state
        .with_library(|db, _| knowledge::set_color(db, &note.id, Some("#aabbcc")).map(|_| ()))
        .unwrap();
    request.destination = Destination::Append {
        note_id: note.id.clone(),
        expected_revision: note.revision,
        insert_index: 1,
    };
    let receipt = commit_in_library(&state, &library, &request, &|| false)
        .unwrap()
        .receipt;
    assert_eq!(receipt.revision, Some(note.revision + 1));
    let saved = state
        .with_library(|db, _| knowledge::get(db, &note.id))
        .unwrap();
    assert_eq!(saved.title, note.title);
    assert_eq!(saved.tags, note.tags);
    assert!(saved.favorite);
    assert_eq!(saved.color, Some("#AABBCC".into()));
    let body = saved.document["content"].as_array().unwrap();
    assert_eq!(body.first(), Some(&note.document["content"][0]));
    assert_eq!(body.last(), Some(&note.document["content"][1]));
    assert_eq!(body[1]["type"], "learningReference");
    assert!(saved.plain_text.contains("完整第二行") || saved.plain_text.contains("保留第二行"));
    let preview = state
        .with_library(|db, base| {
            let old = excerpt_store::get_preview(db, base, &library, &request.excerpt_id)?;
            excerpt_store::save_prepared(db, base, &library, &old.prepared)
        })
        .unwrap();
    request.excerpt_id = preview.id;
    request.operation_id = uuid::Uuid::new_v4().to_string();
    assert!(commit_in_library(&state, &library, &request, &|| false).is_err());
    assert_eq!(
        state
            .with_library(|db, _| knowledge::get(db, &note.id))
            .unwrap()
            .document,
        saved.document
    );
}
#[test]
fn learning_excerpt_publish_project_links_real_entities_and_survives_later_note_save() {
    let (_root, state, library, mut request) = fixture();
    let project = state
        .with_library_mut(|db, _| {
            crate::projects::upsert_project(
                db,
                crate::models::ProjectInput {
                    id: None,
                    name: "UE学习场景".into(),
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
            )
        })
        .unwrap()
        .summary
        .id;
    request.destination = Destination::Project {
        project_id: project.clone(),
        title: "项目学习资料".into(),
        notebook_id: None,
    };
    let result = commit_in_library(&state, &library, &request, &|| false).unwrap();
    state.with_library_mut(|db,_|{
        let mut note=knowledge::get(db,result.receipt.note_id.as_deref().unwrap())?;assert!(note.plain_text.contains("UE学习场景"));
        note.document["content"].as_array_mut().unwrap().push(json!({"type":"paragraph","content":[{"type":"text","text":"后续手动笔记"}]}));
        knowledge::save(db,NoteInput{id:Some(note.id.clone()),expected_revision:Some(note.revision),notebook_id:note.notebook_id,title:note.title,document:note.document,tags:note.tags,favorite:note.favorite,journal_date:note.journal_date})?;
        assert_eq!(db.query_row("SELECT count(*) FROM knowledge_references WHERE note_id=?1 AND target_kind='project' AND target_id=?2",params![note.id,project],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(db.query_row("SELECT count(*) FROM learning_course_notes WHERE note_id=?1",[&note.id],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(db.query_row("SELECT count(*) FROM project_media_entries WHERE project_id=?1",[&project],|r|r.get::<_,i64>(0)).unwrap(),1);
        crate::projects::archive_project(db,&project,true)?;Ok(())
    }).unwrap();
    let preview = state
        .with_library(|db, base| {
            let old = excerpt_store::get_preview(db, base, &library, &request.excerpt_id)?;
            excerpt_store::save_prepared(db, base, &library, &old.prepared)
        })
        .unwrap();
    request.excerpt_id = preview.id;
    request.operation_id = uuid::Uuid::new_v4().to_string();
    assert!(commit_in_library(&state, &library, &request, &|| false).is_err());
    assert_eq!(counts(&state).0, 1);
}
#[test]
fn learning_excerpt_publish_rollback_retries_pending_but_lost_ack_reads_durable_receipt() {
    let (_root, state, library, request) = fixture();
    assert!(
        commit_guarded(&state, &library, &request, &|| false, |_tx| Err(
            "模拟提交前故障".into()
        ))
        .is_err()
    );
    assert_eq!(counts(&state), (0, 1, 0));
    let result = commit_guarded(&state, &library, &request, &|| false, |tx| {
        tx.commit().map_err(|e| e.to_string())?;
        Err("模拟已提交但响应丢失".into())
    })
    .unwrap();
    assert!(result.target_available);
    assert_eq!(counts(&state), (1, 0, 1));
}
#[test]
fn learning_excerpt_publish_cancel_wrong_library_and_changed_source_never_create_a_note() {
    let (root, state, library, request) = fixture();
    assert!(commit_in_library(&state, &library, &request, &|| true).is_err());
    assert_eq!(counts(&state), (0, 0, 0));
    assert!(
        commit_in_library(&state, &uuid::Uuid::new_v4().to_string(), &request, &|| {
            false
        })
        .is_err()
    );
    std::fs::write(
        root.path().join("fixture.mp4"),
        b"replaced before confirmation",
    )
    .unwrap();
    assert!(commit_in_library(&state, &library, &request, &|| false).is_err());
    assert_eq!(counts(&state).0, 0);
    cancel_in_library(&state, &library, &request).unwrap();
    assert!(commit_in_library(&state, &library, &request, &|| false).is_err());
    assert_eq!(
        state
            .with_library(|db, _| db
                .query_row(
                    "SELECT status FROM learning_excerpts WHERE id=?1",
                    [&request.excerpt_id],
                    |r| r.get::<_, String>(0)
                )
                .map_err(|e| e.to_string()))
            .unwrap(),
        "cancelled"
    );
}
#[test]
fn learning_excerpt_publish_selected_subtitles_are_verified_before_writing_full_text() {
    let (root, state, library, course, mut selection) =
        super::excerpt_runtime_tests::fixture(false);
    let source = root.path().join("source.srt");
    std::fs::write(
        &source,
        "1\n00:00:00,100 --> 00:00:00,700\n完整字幕第一行\n第二行也保留\n",
    )
    .unwrap();
    let sub = state
        .with_library(|db, base| super::subtitles::import(db, base, &course, &source))
        .unwrap();
    selection.subtitle_id = Some(sub.id.clone());
    selection.cue_ids = vec![format!("{}:0", sub.id)];
    selection.ranges.clear();
    let prepared =
        excerpt_runtime::prepare_in_library(&state, &library, &course, &selection, None, &|| false)
            .unwrap();
    let preview = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let request = CommitRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        excerpt_id: preview.id,
        destination: Destination::New {
            title: "字幕摘录".into(),
            notebook_id: None,
        },
        include_frame: false,
    };
    let subtitle_path = state
        .with_library(|db, base| super::subtitles::path(db, base, &course, &sub.id))
        .unwrap();
    let result = commit_guarded(&state, &library, &request, &|| false, |tx| {
        #[cfg(windows)]
        {
            assert!(std::fs::write(
                root.path().join("fixture.mp4"),
                b"must not replace leased video"
            )
            .is_err());
            assert!(std::fs::write(&subtitle_path, b"must not replace leased subtitle").is_err());
        }
        tx.commit().map_err(|e| e.to_string())
    })
    .unwrap();
    let note = state
        .with_library(|db, _| knowledge::get(db, result.receipt.note_id.as_deref().unwrap()))
        .unwrap();
    assert!(note.plain_text.contains("完整字幕第一行"));
    assert!(note.plain_text.contains("第二行也保留"));
    // An explicit request for a frame may never silently publish text alone.
    let mut image = request.clone();
    image.operation_id = uuid::Uuid::new_v4().to_string();
    image.include_frame = true;
    assert!(commit_in_library(&state, &library, &image, &|| false).is_err());
}
#[test]
fn learning_excerpt_publish_concurrent_clicks_have_one_receipt_and_busy_database_returns_without_waiting(
) {
    let (_root, state, library, request) = fixture();
    let outcomes = std::thread::scope(|scope| {
        let first = scope.spawn(|| commit_in_library(&state, &library, &request, &|| false));
        let second = scope.spawn(|| commit_in_library(&state, &library, &request, &|| false));
        [first.join().unwrap(), second.join().unwrap()]
    });
    // Active/SQLite contention is a retryable result, never a second operation.
    assert!(outcomes.iter().any(Result::is_ok));
    let saved = commit_in_library(&state, &library, &request, &|| false).unwrap();
    for result in outcomes.into_iter().filter_map(Result::ok) {
        assert_eq!(result.receipt, saved.receipt);
    }
    assert_eq!(counts(&state), (1, 0, 1));

    let preview = state
        .with_library(|db, base| {
            let old = excerpt_store::get_preview(db, base, &library, &request.excerpt_id)?;
            excerpt_store::save_prepared(db, base, &library, &old.prepared)
        })
        .unwrap();
    let mut next = request.clone();
    next.excerpt_id = preview.id;
    next.operation_id = uuid::Uuid::new_v4().to_string();
    assert!(
        commit_guarded(&state, &library, &next, &|| false, |_tx| Err(
            "保留pending用于锁检查".into()
        ))
        .is_err()
    );
    let base = state.with_library(|_, base| Ok(base.to_owned())).unwrap();
    let blocker = crate::tasks::open_media_worker_connection(&base.join("library.db")).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let before = std::time::Instant::now();
    assert!(commit_in_library(&state, &library, &next, &|| false).is_err());
    assert!(before.elapsed() < std::time::Duration::from_secs(2));
    blocker.execute_batch("ROLLBACK").unwrap();
    assert!(commit_in_library(&state, &library, &next, &|| false).is_ok());
    assert_eq!(counts(&state), (2, 0, 2));
}
#[test]
fn learning_excerpt_publish_rejects_a_stale_worker_after_same_library_connection_is_replaced() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    let (root, state, library, request) = fixture();
    let restored_path = root.path().join("restored-connection.db");
    state
        .with_library(|db, _| {
            db.execute("VACUUM INTO ?1", [restored_path.to_string_lossy().as_ref()])
                .map_err(|e| e.to_string())?;
            Ok(())
        })
        .unwrap();
    let restored = crate::db::open_database(&restored_path).unwrap();
    super::excerpt_store::invalidate_previews(&restored).unwrap();
    let replacement = Mutex::new(Some(restored));
    let calls = AtomicUsize::new(0);
    // Model the active connection generation changing after intent admission;
    // the old worker remains open. This is separate from real backup tests.
    let result = commit_in_library(&state, &library, &request, &|| {
        if calls.fetch_add(1, Ordering::SeqCst) == 1 {
            state.active.lock().unwrap().connection = replacement.lock().unwrap().take();
        }
        false
    });
    assert!(
        result.is_err(),
        "a stale worker must not report a write to the active restored library"
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
