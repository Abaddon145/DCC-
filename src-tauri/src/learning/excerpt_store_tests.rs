use super::{
    excerpt_runtime::{self, PreparedExcerpt},
    excerpt_store::*,
};
use crate::knowledge::{self, NoteInput};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::fs;
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
pub(super) fn fixture(
    frame: bool,
) -> (
    tempfile::TempDir,
    crate::state::AppState,
    String,
    PreparedExcerpt,
) {
    let (root, state, library, course, mut selection) =
        super::excerpt_runtime_tests::fixture(frame);
    state
        .with_library(|db, base| {
            let media = super::store::course(db, &course)?.media_id;
            let relative = format!("media-library/video/{media}/video.mp4");
            let target = base.join(&relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(base.join("fixture.mp4"), target).unwrap();
            db.execute(
                "UPDATE media_files SET rel_path=?1 WHERE entry_id=?2",
                params![relative, media],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    if frame {
        selection.frame_time_ms = Some(400);
    }
    let prepared =
        excerpt_runtime::prepare_in_library(&state, &library, &course, &selection, None, &|| false)
            .unwrap();
    (root, state, library, prepared)
}
#[test]
fn learning_excerpt_store_persists_deduplicates_and_reopens_with_a_scoped_immutable_payload() {
    let (root, state, library, prepared) = fixture(true);
    let first = state
        .with_library(|db, base| save_prepared(db, base, &library, &prepared))
        .unwrap();
    let second = state
        .with_library(|db, base| save_prepared(db, base, &library, &prepared))
        .unwrap();
    assert_eq!(first.id, second.id);
    let db = crate::db::open_database(&root.path().join("library.db")).unwrap();
    let loaded = get_preview(&db, root.path(), &library, &first.id).unwrap();
    assert_eq!(loaded.status, "ready");
    assert_eq!(
        serde_json::to_value(&loaded.prepared).unwrap(),
        serde_json::to_value(&prepared).unwrap()
    );
    assert!(get_preview(
        &db,
        root.path(),
        &uuid::Uuid::new_v4().to_string(),
        &first.id
    )
    .is_err());
    assert!(get_preview(&db, root.path(), &library, "../library.db").is_err());
    let cancelled = cancel_preview(&db, root.path(), &library, &first.id).unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(
        cancel_preview(&db, root.path(), &library, &first.id)
            .unwrap()
            .status,
        "cancelled"
    );
    let again = save_prepared(&db, root.path(), &library, &prepared).unwrap();
    assert_ne!(again.id, first.id);
}
#[test]
fn learning_excerpt_store_rejects_forged_bindings_documents_images_and_corrupt_payloads() {
    let (root, state, library, prepared) = fixture(true);
    let db = crate::db::open_database(&root.path().join("library.db")).unwrap();
    let mut bad = prepared.clone();
    bad.root_hash = "f".repeat(64);
    assert!(save_prepared(&db, root.path(), &library, &bad).is_err());
    bad = prepared.clone();
    bad.library_id = uuid::Uuid::new_v4().to_string();
    assert!(save_prepared(&db, root.path(), &library, &bad).is_err());
    bad = prepared.clone();
    bad.document = serde_json::json!({"type":"doc","content":[]});
    assert!(save_prepared(&db, root.path(), &library, &bad).is_err());
    bad = prepared.clone();
    bad.frame.as_mut().unwrap().sha256 = "f".repeat(64);
    assert!(save_prepared(&db, root.path(), &library, &bad).is_err());
    bad = prepared.clone();
    bad.frame = None;
    bad.warnings.clear();
    assert!(save_prepared(&db, root.path(), &library, &bad).is_err());
    let good = save_prepared(&db, root.path(), &library, &prepared).unwrap();
    db.execute(
        "UPDATE learning_excerpts SET payload_json='{}' WHERE id=?1",
        [&good.id],
    )
    .unwrap();
    assert!(get_preview(&db, root.path(), &library, &good.id).is_err());
    db.execute(
        "UPDATE learning_excerpts SET payload_sha256=?1 WHERE id=?2",
        params![hash(b"{}"), good.id],
    )
    .unwrap();
    assert!(get_preview(&db, root.path(), &library, &good.id).is_err());
    drop(state);
}
#[test]
fn learning_excerpt_store_parallel_preparations_share_one_ready_identity() {
    let (root, _state, library, prepared) = fixture(false);
    let ids = std::thread::scope(|scope| {
        let mut workers = vec![];
        for _ in 0..3 {
            let path = root.path();
            let library = &library;
            let prepared = &prepared;
            workers.push(scope.spawn(move || {
                let db =
                    crate::tasks::open_media_worker_connection(&path.join("library.db")).unwrap();
                save_prepared(&db, path, library, prepared).unwrap().id
            }));
        }
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(ids.iter().all(|id| id == &ids[0]));
}
#[test]
fn learning_excerpt_store_scope_rejection_rolls_back_and_changed_source_registration_is_rejected() {
    let (root, _state, library, prepared) = fixture(false);
    let db = crate::db::open_database(&root.path().join("library.db")).unwrap();
    assert!(
        save_prepared_guarded(&db, root.path(), &library, &prepared, |_tx| {
            Err("最终作用域已切换".into())
        })
        .is_err()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_excerpts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // Check the final registry as well as the initial preparation, using the shared owner query.
    db.execute(
        "UPDATE media_files SET role='proxy' WHERE id=?1",
        [&prepared.source_file_id],
    )
    .unwrap();
    assert!(save_prepared(&db, root.path(), &library, &prepared).is_err());
    db.execute(
        "UPDATE media_files SET role='main',checksum=?1 WHERE id=?2",
        params!["f".repeat(64), prepared.source_file_id],
    )
    .unwrap();
    assert!(save_prepared(&db, root.path(), &library, &prepared).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_excerpts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn learning_excerpt_store_v28_upgrade_preserves_notes_and_enforces_operation_receipts() {
    let (_root, state, library, prepared) = fixture(false);
    state.with_library_mut(|db,base|{
        let note=knowledge::save(db,NoteInput{id:None,expected_revision:None,notebook_id:None,title:"已有笔记".into(),document:prepared.document.clone(),tags:vec![],favorite:false,journal_date:None})?;
        db.execute_batch("DROP TABLE IF EXISTS learning_origin_map; DROP TABLE learning_source_profiles;DROP TABLE IF EXISTS learning_course_notes;DROP TABLE IF EXISTS learning_excerpt_operations;DROP TABLE IF EXISTS learning_excerpts;UPDATE schema_meta SET value='28' WHERE key='schema_version';").unwrap();super::migrate(db)?;
        assert_eq!(db.query_row("SELECT value FROM schema_meta WHERE key='schema_version'",[],|r|r.get::<_,String>(0)).unwrap(),"32");assert_eq!(knowledge::get(db,&note.id)?.document,note.document);
        let preview=save_prepared(db,base,&library,&prepared)?;let operation=uuid::Uuid::new_v4().to_string();
        assert!(db.execute("INSERT INTO learning_excerpt_operations(operation_id,excerpt_id,request_json,request_sha256,status,created_at) VALUES(?1,?2,'{}',?3,'committed','now')",params![operation,preview.id,hash(b"{}")]).is_err());
        db.execute("INSERT INTO learning_excerpt_operations(operation_id,excerpt_id,request_json,request_sha256,status,created_at) VALUES(?1,?2,'{}',?3,'pending','now')",params![operation,preview.id,hash(b"{}")]).unwrap();
        assert!(cancel_preview(db,base,&library,&preview.id).is_err());
        assert!(db.execute("INSERT INTO learning_excerpt_operations(operation_id,excerpt_id,request_json,request_sha256,status,created_at) VALUES(?1,?2,'{}',?3,'pending','now')",params![uuid::Uuid::new_v4().to_string(),preview.id,hash(b"{}")]).is_err());
        db.execute("INSERT INTO learning_course_notes(course_id,note_id) VALUES(?1,?2)",params![prepared.text.course_id,note.id]).unwrap();
        db.execute("DELETE FROM knowledge_notes WHERE id=?1",[&note.id]).unwrap();assert_eq!(db.query_row("SELECT count(*) FROM learning_course_notes",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(db.query_row("SELECT count(*) FROM learning_excerpt_operations WHERE operation_id=?1",[&operation],|r|r.get::<_,i64>(0)).unwrap(),1);Ok(())
    }).unwrap();
}
#[test]
fn learning_excerpt_store_backup_revokes_pending_approval_without_deleting_saved_notes_or_receipts()
{
    let (root, state, library, prepared) = fixture(false);
    let (pending,committed,note_id,operation)=state.with_library_mut(|db,base|{
        let first=save_prepared(db,base,&library,&prepared)?;
        let mut second=prepared.clone();second.text.comment.push_str("另一份");second.document=super::excerpts::document(&second.text,&second.source_sha256)?;
        let second=save_prepared(db,base,&library,&second)?;
        let note=knowledge::save(db,NoteInput{id:None,expected_revision:None,notebook_id:None,title:"保留已保存摘录".into(),document:prepared.document.clone(),tags:vec![],favorite:false,journal_date:None})?;
        let operation=uuid::Uuid::new_v4().to_string();
        db.execute("INSERT INTO learning_excerpt_operations(operation_id,excerpt_id,request_json,request_sha256,status,created_at) VALUES(?1,?2,'{}',?3,'pending','now')",params![uuid::Uuid::new_v4().to_string(),first.id,hash(b"{}")]).unwrap();
        db.execute("INSERT INTO learning_excerpt_operations(operation_id,excerpt_id,request_json,request_sha256,status,receipt_json,created_at) VALUES(?1,?2,'{}',?3,'committed',?4,'now')",params![operation,second.id,hash(b"{}"),serde_json::json!({"noteId":note.id,"revision":note.revision}).to_string()]).unwrap();
        db.execute("UPDATE learning_excerpts SET status='committed' WHERE id=?1",[&second.id]).unwrap();Ok((first.id,second.id,note.id,operation))
    }).unwrap();
    let backup = root.path().join("excerpts.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    state
        .with_library(|db, base| {
            assert_eq!(
                get_preview(db, base, &library, &pending)?.status,
                "cancelled"
            );
            assert_eq!(
                get_preview(db, base, &library, &committed)?.status,
                "committed"
            );
            assert_eq!(
                db.query_row(
                    "SELECT status FROM learning_excerpt_operations WHERE excerpt_id=?1",
                    [&pending],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "cancelled"
            );
            assert!(db
                .query_row(
                    "SELECT receipt_json FROM learning_excerpt_operations WHERE operation_id=?1",
                    [&operation],
                    |r| r.get::<_, String>(0)
                )
                .unwrap()
                .contains(&note_id));
            assert_eq!(knowledge::get(db, &note_id)?.title, "保留已保存摘录");
            Ok(())
        })
        .unwrap();
}
