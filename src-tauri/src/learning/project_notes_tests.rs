use super::{
    excerpt_publish::{self, CommitRequest, Destination},
    excerpt_runtime, excerpt_store,
    project_notes::*,
};
use crate::{
    knowledge::{self, NoteInput},
    models::ProjectInput,
    state::AppState,
};
use rusqlite::params;
use serde_json::json;
fn project(state: &AppState, name: &str) -> String {
    state
        .with_library_mut(|db, _| {
            crate::projects::upsert_project(
                db,
                ProjectInput {
                    id: None,
                    name: name.into(),
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
        .id
}
fn note(state: &AppState, project: Option<&str>) -> knowledge::Note {
    let content=project.map(|id|json!({"type":"reference","attrs":{"kind":"project","targetId":id,"label":"原项目引用"}}));
    state.with_library_mut(|db,_|knowledge::save(db,NoteInput{id:None,expected_revision:None,notebook_id:None,title:"学习笔记".into(),document:json!({"type":"doc","content":[{"type":"paragraph","content":content.into_iter().collect::<Vec<_>>()},{"type":"paragraph","content":[{"type":"text","text":"保留的正文"}]}]}),tags:vec![],favorite:false,journal_date:None})).unwrap()
}
#[test]
fn learning_project_notes_migration_backfills_real_project_references_once() {
    let (_root, state, library, course, _) = super::excerpt_runtime_tests::fixture(false);
    let first = project(&state, "项目一");
    let other = project(&state, "项目二");
    let original = note(&state, Some(&first));
    let unrelated = note(&state, Some(&other));
    state.with_library_mut(|db,_|{db.execute("INSERT INTO learning_course_notes(course_id,note_id) VALUES(?1,?2)",params![course,original.id]).unwrap();db.execute_batch("DROP TABLE IF EXISTS project_learning_notes;UPDATE schema_meta SET value='29' WHERE key='schema_version';").unwrap();super::migrate(db)?;Ok(())}).unwrap();
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        list_in_library(&state, &library, &other, 0, 20)
            .unwrap()
            .total,
        0
    );
    unlink_in_library(&state, &library, &first, &original.id).unwrap();
    state
        .with_library(|db, _| {
            assert_eq!(
                knowledge::get(db, &unrelated.id)?.document,
                unrelated.document
            );
            Ok(())
        })
        .unwrap();
    state.with_library_mut(|db, _| super::migrate(db)).unwrap();
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        0
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                knowledge::get(db, &original.id)?.document,
                original.document
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn learning_project_notes_link_is_scoped_and_unlink_preserves_user_content() {
    let (_root, state, library, _course, _) = super::excerpt_runtime_tests::fixture(false);
    let first = project(&state, "场景一");
    let other = project(&state, "场景二");
    let original = note(&state, None);
    link_in_library(&state, &library, &first, &original.id).unwrap();
    link_in_library(&state, &library, &first, &original.id).unwrap();
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        list_in_library(&state, &library, &other, 0, 20)
            .unwrap()
            .total,
        0
    );
    assert!(list_in_library(&state, &uuid::Uuid::new_v4().to_string(), &first, 0, 20).is_err());
    assert!(link_in_library(&state, &library, "../bad", &original.id).is_err());
    assert!(list_in_library(&state, &library, &first, 0, 51).is_err());
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE knowledge_notes SET deleted_at='trash' WHERE id=?1",
                [&original.id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        0
    );
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE knowledge_notes SET deleted_at=NULL WHERE id=?1",
                [&original.id],
            )
            .unwrap();
            crate::projects::archive_project(db, &first, true)
        })
        .unwrap();
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        1
    );
    assert!(link_in_library(&state, &library, &first, &original.id).is_err());
    unlink_in_library(&state, &library, &first, &original.id).unwrap();
    state
        .with_library(|db, _| {
            assert_eq!(
                knowledge::get(db, &original.id)?.document,
                original.document
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn learning_project_notes_excerpt_save_links_atomically_without_affecting_other_project() {
    let (_root, state, library, course, selection) = super::excerpt_runtime_tests::fixture(false);
    let first = project(&state, "项目一");
    let other = project(&state, "项目二");
    let prepared =
        excerpt_runtime::prepare_in_library(&state, &library, &course, &selection, None, &|| false)
            .unwrap();
    let stored = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let request = CommitRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        excerpt_id: stored.id,
        destination: Destination::Project {
            project_id: first.clone(),
            title: "项目摘录".into(),
            notebook_id: None,
        },
        include_frame: false,
    };
    let receipt =
        excerpt_publish::commit_in_library(&state, &library, &request, &|| false).unwrap();
    let page = list_in_library(&state, &library, &first, 0, 20).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, receipt.receipt.note_id.unwrap());
    assert_eq!(page.items[0].course_count, 1);
    assert_eq!(
        list_in_library(&state, &library, &other, 0, 20)
            .unwrap()
            .total,
        0
    );
}
#[test]
fn learning_project_notes_failed_excerpt_transaction_leaves_no_link_and_retries() {
    let (_root, state, library, course, selection) = super::excerpt_runtime_tests::fixture(false);
    let first = project(&state, "失败回滚");
    let prepared =
        excerpt_runtime::prepare_in_library(&state, &library, &course, &selection, None, &|| false)
            .unwrap();
    let stored = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let request = CommitRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        excerpt_id: stored.id,
        destination: Destination::Project {
            project_id: first.clone(),
            title: "项目摘录".into(),
            notebook_id: None,
        },
        include_frame: false,
    };
    assert!(
        excerpt_publish::commit_guarded(&state, &library, &request, &|| false, |_tx| Err(
            "模拟提交失败".into()
        ))
        .is_err()
    );
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        0
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
    excerpt_publish::commit_in_library(&state, &library, &request, &|| false).unwrap();
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        1
    );
}
#[test]
fn learning_project_notes_backup_restore_preserves_link_and_note_body() {
    let (root, state, library, _course, _) = super::excerpt_runtime_tests::fixture(false);
    let first = project(&state, "备份项目");
    let original = note(&state, Some(&first));
    link_in_library(&state, &library, &first, &original.id).unwrap();
    let backup = root.path().join("project-notes.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    unlink_in_library(&state, &library, &first, &original.id).unwrap();
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        0
    );
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    assert_eq!(
        list_in_library(&state, &library, &first, 0, 20)
            .unwrap()
            .total,
        1
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                knowledge::get(db, &original.id)?.document,
                original.document
            );
            Ok(())
        })
        .unwrap();
}
