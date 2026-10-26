use super::{excerpt_publish::*, excerpt_store};
use rusqlite::params;
use serde_json::json;
use std::fs;
fn fixture() -> (
    tempfile::TempDir,
    crate::state::AppState,
    String,
    CommitRequest,
) {
    let (root, state, library, prepared) = super::excerpt_store_tests::fixture(true);
    let preview = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let request = CommitRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        excerpt_id: preview.id,
        destination: Destination::New {
            title: "完整截图摘录".into(),
            notebook_id: None,
        },
        include_frame: true,
    };
    (root, state, library, request)
}
fn note_id(result: &CommitResult) -> String {
    serde_json::to_value(&result.receipt).unwrap()["noteId"]
        .as_str()
        .unwrap()
        .to_owned()
}
#[test]
fn learning_excerpt_images_new_note_registers_owned_image_and_exact_frame_reference() {
    let (root, state, library, request) = fixture();
    let result = commit_guarded(&state, &library, &request, &|| false, |tx| {
        let path: String = tx
            .query_row(
                "SELECT relative_path FROM knowledge_attachments LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        #[cfg(windows)]
        assert!(fs::write(
            root.path().join(path),
            b"must not mutate final image before commit"
        )
        .is_err());
        tx.commit().map_err(|e| e.to_string())
    })
    .unwrap();
    let note = state
        .with_library(|db, base| {
            let note = crate::knowledge::get(db, &note_id(&result))?;
            let (attachment, sha): (String, String) = db
                .query_row(
                    "SELECT id,sha256 FROM knowledge_attachments WHERE note_id=?1",
                    [&note.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| e.to_string())?;
            let (path, _) = crate::knowledge_files::path(db, base, &attachment)?;
            assert_eq!(crate::file_digest::digest_file(&path)?.1, sha);
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM knowledge_revisions WHERE note_id=?1",
                    [&note.id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            Ok(note)
        })
        .unwrap();
    let nodes = note.document["content"].as_array().unwrap();
    assert!(nodes.iter().any(|n| n["type"] == "image"));
    assert!(nodes
        .iter()
        .any(|n| n["type"] == "learningReference" && n["attrs"]["ranges"][0]["startMs"] == 400));
    assert!(note.plain_text.contains("保留第二行"));
    let again = commit_in_library(&state, &library, &request, &|| false).unwrap();
    assert_eq!(again.receipt, result.receipt);
}
#[test]
fn learning_excerpt_images_sql_failure_rolls_back_files_and_reuses_reserved_identity() {
    let (root, state, library, request) = fixture();
    state.with_library(|db,_|db.execute_batch("CREATE TRIGGER fail_excerpt_image BEFORE INSERT ON knowledge_attachments BEGIN SELECT RAISE(ABORT,'test attachment publication failure'); END;").map_err(|e|e.to_string())).unwrap();
    assert!(commit_in_library(&state, &library, &request, &|| false).is_err());
    // Read directly to avoid re-entering AppState inside a library closure.
    let stored=state.with_library(|db,base|{
        assert_eq!(db.query_row("SELECT count(*) FROM knowledge_notes",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        let location:String=db.query_row("SELECT manifest_relative FROM learning_excerpt_operations WHERE operation_id=?1",[&request.operation_id],|r|r.get(0)).unwrap();
        let raw=fs::read(base.join(location)).unwrap();let manifest:super::excerpt_files::PublicationManifest=serde_json::from_slice(&raw).unwrap();
        for file in &manifest.files {assert!(!base.join(&file.final_relative).exists());}
        db.execute_batch("DROP TRIGGER fail_excerpt_image").unwrap();Ok(manifest)
    }).unwrap();
    let result = commit_in_library(&state, &library, &request, &|| false).unwrap();
    let super::excerpt_files::FileDestination::Note { note_id: reserved } = &stored.target else {
        panic!("note target")
    };
    assert_eq!(&note_id(&result), reserved);
    for file in &stored.files {
        assert!(root.path().join(&file.final_relative).is_file());
    }
}
#[test]
fn learning_excerpt_images_board_requires_frame_and_keeps_source_and_replay_after_item_deletion() {
    let (_root, state, library, mut request) = fixture();
    let board = state
        .with_library(|db, _| crate::reference_boards::create(db, "学习截图"))
        .unwrap();
    request.destination = Destination::Board {
        board_id: board.id.clone(),
        x: 20.0,
        y: 40.0,
    };
    let result = commit_in_library(&state, &library, &request, &|| false).unwrap();
    let receipt = serde_json::to_value(&result.receipt).unwrap();
    assert_eq!(receipt["boardId"], board.id);
    let item = receipt["boardItemId"].as_str().unwrap();
    state.with_library(|db,base|{
        assert_eq!(db.query_row("SELECT count(*) FROM knowledge_notes",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(db.query_row("SELECT excerpt_id FROM learning_excerpt_board_items WHERE item_id=?1",[item],|r|r.get::<_,String>(0)).unwrap(),request.excerpt_id);
        let paths:(String,String)=db.query_row("SELECT original_rel_path,thumbnail_rel_path FROM reference_board_items WHERE id=?1",[item],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();assert!(base.join(paths.0).is_file());assert!(base.join(paths.1).is_file());
        db.execute("DELETE FROM reference_board_items WHERE id=?1",[item]).unwrap();Ok(())
    }).unwrap();
    let repeat = commit_in_library(&state, &library, &request, &|| false).unwrap();
    assert_eq!(repeat.receipt, result.receipt);
    assert!(!repeat.target_available);
    let mut invalid = request.clone();
    invalid.operation_id = uuid::Uuid::new_v4().to_string();
    invalid.include_frame = false;
    assert!(commit_in_library(&state, &library, &invalid, &|| false).is_err());
    let mut input = serde_json::to_value(request).unwrap();
    input["destination"]["x"] = json!(1000001);
    let invalid: CommitRequest = serde_json::from_value(input).unwrap();
    assert!(commit_in_library(&state, &library, &invalid, &|| false).is_err());
}
#[test]
fn learning_excerpt_images_lost_ack_and_real_backup_preserve_registered_images() {
    let (root, state, library, request) = fixture();
    let result = commit_guarded(&state, &library, &request, &|| false, |tx| {
        tx.commit().map_err(|e| e.to_string())?;
        Err("simulated lost response after image commit".into())
    })
    .unwrap();
    let id = note_id(&result);
    let backup = root.path().join("image-excerpt.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    let repeat = commit_in_library(&state, &library, &request, &|| false).unwrap();
    assert_eq!(repeat.receipt, result.receipt);
    state
        .with_library(|db, base| {
            let attachment: String = db
                .query_row(
                    "SELECT id FROM knowledge_attachments WHERE note_id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .unwrap();
            let (path, _) = crate::knowledge_files::path(db, base, &attachment)?;
            assert!(path.is_file());
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM knowledge_attachments WHERE note_id=?1",
                    params![id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn learning_excerpt_images_append_and_project_keep_existing_content_and_associations() {
    let (_root, state, library, mut request) = fixture();
    let existing=state.with_library_mut(|db,_|crate::knowledge::save(db,crate::knowledge::NoteInput{id:None,expected_revision:None,notebook_id:None,title:"保留原笔记".into(),document:json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"不能覆盖的已有正文"}]}]}),tags:vec!["UE5".into()],favorite:true,journal_date:None})).unwrap();
    request.destination = Destination::Append {
        note_id: existing.id.clone(),
        expected_revision: existing.revision,
        insert_index: 1,
    };
    let appended = commit_in_library(&state, &library, &request, &|| false).unwrap();
    assert_eq!(note_id(&appended), existing.id);
    state
        .with_library(|db, _| {
            let note = crate::knowledge::get(db, &existing.id)?;
            assert!(note.plain_text.contains("不能覆盖的已有正文"));
            assert!(note.plain_text.contains("保留第二行"));
            assert!(note.favorite);
            assert_eq!(note.tags, vec!["UE5"]);
            assert!(note.document["content"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["type"] == "image"));
            Ok(())
        })
        .unwrap();
    let preview = state
        .with_library(|db, base| {
            let old = excerpt_store::get_preview(db, base, &library, &request.excerpt_id)?;
            excerpt_store::save_prepared(db, base, &library, &old.prepared)
        })
        .unwrap();
    request.operation_id = uuid::Uuid::new_v4().to_string();
    request.excerpt_id = preview.id;
    assert!(commit_in_library(&state, &library, &request, &|| false).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM knowledge_attachments", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
    cancel_in_library(&state, &library, &request).unwrap();
    let preview = state
        .with_library(|db, base| {
            let old = excerpt_store::get_preview(db, base, &library, &request.excerpt_id)?;
            excerpt_store::save_prepared(db, base, &library, &old.prepared)
        })
        .unwrap();
    let project = state
        .with_library_mut(|db, _| {
            crate::projects::upsert_project(
                db,
                crate::models::ProjectInput {
                    id: None,
                    name: "截图项目".into(),
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
    request.operation_id = uuid::Uuid::new_v4().to_string();
    request.excerpt_id = preview.id;
    request.destination = Destination::Project {
        project_id: project.clone(),
        title: "项目截图笔记".into(),
        notebook_id: None,
    };
    let saved = commit_in_library(&state, &library, &request, &|| false).unwrap();
    state.with_library(|db,_|{
        assert_eq!(db.query_row("SELECT count(*) FROM knowledge_attachments",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(db.query_row("SELECT count(*) FROM knowledge_references WHERE note_id=?1 AND target_kind='project' AND target_id=?2",params![note_id(&saved),project],|r|r.get::<_,i64>(0)).unwrap(),1);Ok(())
    }).unwrap();
}
#[test]
fn learning_excerpt_images_no_frame_creates_no_board_item_and_old_text_receipts_remain_readable() {
    let (_root, state, library, prepared) = super::excerpt_store_tests::fixture(false);
    let preview = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let board = state
        .with_library(|db, _| crate::reference_boards::create(db, "无图不可添加"))
        .unwrap();
    let mut request = CommitRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        excerpt_id: preview.id,
        destination: Destination::Board {
            board_id: board.id,
            x: 0.0,
            y: 0.0,
        },
        include_frame: true,
    };
    assert!(commit_in_library(&state, &library, &request, &|| false).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM reference_board_items", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
    request.include_frame = false;
    request.destination = Destination::New {
        title: "旧格式回执兼容".into(),
        notebook_id: None,
    };
    let result = commit_in_library(&state, &library, &request, &|| false).unwrap();
    let legacy = json!({"operationId":request.operation_id,"excerptId":request.excerpt_id,"noteId":note_id(&result),"revision":result.receipt.revision,"warnings":[]});
    let parsed: ExcerptReceipt = serde_json::from_value(legacy).unwrap();
    assert_eq!(parsed, result.receipt);
}
