use super::{
    note_archive::{self, ArchiveSource},
    note_preview::{CourseMap, Destination},
    note_preview_runtime::*,
};
use crate::{knowledge::NoteInput, state::AppState};
use serde_json::json;
use sha2::{Digest, Sha256};

fn preview_image_id(preview: &super::note_preview::StoredNotePreview) -> String {
    preview.conversion.document.as_ref().unwrap()["content"][1]["attrs"]["src"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned()
}

#[test]
fn learning_note_preview_content_images_are_scoped_checked_and_read_only() {
    let (root, state, library, archive, attachment) = fixture();
    let preview = prepare_in_library(&state, &library, request(&archive, &attachment)).unwrap();
    let placeholder = preview_image_id(&preview);
    let route = format!("note-preview/{library}/{}/{placeholder}", preview.id);
    let response = image_route(&state, &route).unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["Content-Type"], "image/png");
    assert_eq!(response.headers()["Cache-Control"], "no-store");
    assert_eq!(response.headers()["X-Content-Type-Options"], "nosniff");
    assert_eq!(
        *response.body(),
        std::fs::read(root.path().join("学习示例.png")).unwrap()
    );
    assert!(image_in_library(&state, &library, &preview.id, &attachment).is_err());
    assert!(image_in_library(&state, &library, &preview.id, "../secret.png").is_err());
    assert!(image_route(&state, &format!("{route}/extra")).is_err());
    assert!(image_route(&state, &route.replace(&placeholder, "%2e%2e")).is_err());
    assert!(image_in_library(
        &state,
        "596d257d-3e5c-46c4-a464-187cf180c6bd",
        &preview.id,
        &placeholder
    )
    .is_err());
    let mut other = request(&archive, &attachment);
    other.destination = Destination::New {
        title: "different".into(),
        notebook_id: None,
    };
    let other = prepare_in_library(&state, &library, other).unwrap();
    assert!(image_in_library(&state, &library, &other.id, &placeholder).is_err());
    state.with_library(|db, _| {
        let counts:(i64,i64,i64) = db.query_row("SELECT (SELECT count(*) FROM knowledge_notes),(SELECT count(*) FROM knowledge_attachments),(SELECT count(*) FROM learning_note_archive_items)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(counts,(1,1,0));
        Ok(())
    }).unwrap();
    let connection = state
        .with_active_mut(|active| Ok(active.connection.take()))
        .unwrap();
    assert!(image_route(&state, &route).is_err());
    state
        .with_active_mut(|active| {
            active.connection = connection;
            Ok(())
        })
        .unwrap();
    cancel_in_library(&state, &library, &preview.id).unwrap();
    assert!(image_in_library(&state, &library, &preview.id, &placeholder).is_err());
    let other_placeholder = preview_image_id(&other);
    commit_in_library(&state, &library, &other.id).unwrap();
    assert!(image_in_library(&state, &library, &other.id, &other_placeholder).is_err());
}

#[test]
fn learning_note_preview_content_never_serves_text_attachments_as_images() {
    let (root, state, library, archive, _attachment) = fixture();
    let path = root.path().join("data.txt");
    std::fs::write(&path, b"<script>alert(1)</script>").unwrap();
    state
        .with_library(|db, base| {
            let note: String = db
                .query_row("SELECT id FROM knowledge_notes LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let attachment = crate::knowledge_files::import(db, base, &note, &path)?;
            let grants = crate::media_access::LocalMediaGrants::new(&state.config_directory());
            let preview = super::note_preview::prepare_note_preview(
                db,
                base,
                &library,
                &archive,
                Destination::New {
                    title: "text attachment".into(),
                    notebook_id: None,
                },
                &[super::note_preview::AttachmentSource {
                    source_key: "source.txt".into(),
                    path: crate::knowledge_files::path(db, base, &attachment.id)?.0,
                    sha256: format!("{:x}", Sha256::digest(b"<script>alert(1)</script>")),
                    filename: Some("data.txt".into()),
                }],
                &CourseMap::new(),
                &grants,
                &|| false,
            )?;
            let raw: String = db
                .query_row(
                    "SELECT payload_json FROM learning_note_previews WHERE id=?1",
                    [&preview.id],
                    |r| r.get(0),
                )
                .unwrap();
            let snapshot: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let placeholder = snapshot["files"][0]["placeholder"].as_str().unwrap();
            assert!(super::note_preview::preview_image_response(
                db,
                base,
                &library,
                &preview.id,
                placeholder,
                &|| false
            )
            .is_err());
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_note_preview_content_rejects_changed_image_and_switched_library() {
    let (_root, state, library, archive, attachment) = fixture();
    let preview = prepare_in_library(&state, &library, request(&archive, &attachment)).unwrap();
    let placeholder = preview_image_id(&preview);
    assert!(image_in_library(&state, &library, &preview.id, &placeholder).is_ok());
    state
        .with_active_mut(|active| {
            active.marker.library_id = uuid::Uuid::new_v4().to_string();
            Ok(())
        })
        .unwrap();
    assert!(image_in_library(&state, &library, &preview.id, &placeholder).is_err());
    state
        .with_active_mut(|active| {
            active.marker.library_id = library.clone();
            Ok(())
        })
        .unwrap();
    state
        .with_library(|db, base| {
            let raw: String = db
                .query_row(
                    "SELECT payload_json FROM learning_note_previews WHERE id=?1",
                    [&preview.id],
                    |r| r.get(0),
                )
                .unwrap();
            let snapshot: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let relative = snapshot["files"][0]["relative"].as_str().unwrap();
            std::fs::write(base.join(relative), b"changed").unwrap();
            Ok(())
        })
        .unwrap();
    assert!(image_in_library(&state, &library, &preview.id, &placeholder).is_err());
}

#[test]
fn learning_note_preview_content_reads_exact_raw_block_without_publishing_it() {
    let (_root, state, library, _archive, _attachment) = fixture();
    let bytes = b"# title\r\n\r\n<section>\r\nraw\r\n</section>\r\n";
    let archive = state
        .with_library(|db, base| {
            note_archive::archive_bytes(
                db,
                base,
                &ArchiveSource {
                    profile_id: &library,
                    source_key: "raw.md",
                    version_key: "current",
                },
                bytes,
                &format!("{:x}", Sha256::digest(bytes)),
            )
        })
        .unwrap()
        .id;
    let preview = prepare_in_library(
        &state,
        &library,
        PrepareNotePreviewRequest {
            archive_id: archive.clone(),
            destination: Destination::New {
                title: "raw".into(),
                notebook_id: None,
            },
            attachments: vec![],
            courses: CourseMap::new(),
        },
    )
    .unwrap();
    let placeholder = &preview.conversion.unsupported[0].placeholder_id;
    assert_eq!(
        block_in_library(&state, &library, &preview.id, placeholder).unwrap(),
        "<section>\r\nraw\r\n</section>\r\n"
    );
    assert!(block_in_library(&state, &library, &preview.id, &archive).is_err());
    cancel_in_library(&state, &library, &preview.id).unwrap();
    assert_eq!(
        block_in_library(&state, &library, &preview.id, placeholder).unwrap(),
        "<section>\r\nraw\r\n</section>\r\n"
    );
    state
        .with_library(|db, base| {
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM learning_note_archive_items",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            let relative: String = db
                .query_row(
                    "SELECT relative_path FROM learning_note_archives WHERE id=?1",
                    [&archive],
                    |r| r.get(0),
                )
                .unwrap();
            std::fs::write(base.join(relative), b"tampered").unwrap();
            Ok(())
        })
        .unwrap();
    assert!(block_in_library(&state, &library, &preview.id, placeholder).is_err());
}

#[test]
fn learning_note_runtime_closed_maintenance_connection_blocks_submission() {
    let (_root, state, library, archive, attachment) = fixture();
    let preview = prepare_in_library(&state, &library, request(&archive, &attachment)).unwrap();
    let connection = state
        .with_active_mut(|active| Ok(active.connection.take()))
        .unwrap();
    assert!(commit_in_library(&state, &library, &preview.id).is_err());
    assert!(get_in_library(&state, &library, &preview.id).is_err());
    state
        .with_active_mut(|active| {
            active.connection = connection;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        get_in_library(&state, &library, &preview.id)
            .unwrap()
            .status,
        "ready"
    );
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |row| row
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_note_runtime_parallel_prepare_and_commit_share_one_receipt() {
    let (_root, state, library, _archive, attachment) = fixture();
    let bytes = format!("{}\n\n![preview](source.png)\n", "learning ".repeat(80_000)).into_bytes();
    let archive = state
        .with_library(|db, base| {
            note_archive::archive_bytes(
                db,
                base,
                &ArchiveSource {
                    profile_id: &library,
                    source_key: "parallel.md",
                    version_key: "current",
                },
                &bytes,
                &format!("{:x}", Sha256::digest(&bytes)),
            )
        })
        .unwrap()
        .id;
    let state = std::sync::Arc::new(state);
    for trial in 0..3 {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let jobs = (0..2)
            .map(|_| {
                let state = state.clone();
                let library = library.clone();
                let archive = archive.clone();
                let attachment = attachment.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut input = request(&archive, &attachment);
                    input.destination = Destination::New {
                        title: format!("parallel {trial}"),
                        notebook_id: None,
                    };
                    barrier.wait();
                    prepare_in_library(&state, &library, input)
                })
            })
            .collect::<Vec<_>>();
        let mut previews = jobs
            .into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
            .into_iter();
        let a = previews.next().unwrap().unwrap();
        let b = previews.next().unwrap().unwrap();
        assert_eq!(a.id, b.id);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let jobs = (0..2)
            .map(|_| {
                let state = state.clone();
                let library = library.clone();
                let id = a.id.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    commit_in_library(&state, &library, &id)
                })
            })
            .collect::<Vec<_>>();
        let mut receipts = jobs
            .into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
            .into_iter();
        let a = receipts.next().unwrap().unwrap();
        let b = receipts.next().unwrap().unwrap();
        assert_eq!(a.note_id, b.note_id);
        assert_eq!(a.revision, b.revision);
        state
            .with_library(|db, _| {
                assert_eq!(
                    db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    trial + 2
                );
                Ok(())
            })
            .unwrap();
    }
}
fn fixture() -> (tempfile::TempDir, AppState, String, String, String) {
    let root = tempfile::tempdir().unwrap();
    let state = crate::state::test_state(&root.path().join("library"));
    let library = state.active_library_id().unwrap();
    let image = root.path().join("学习示例.png");
    image::RgbaImage::new(2, 2).save(&image).unwrap();
    let (archive, attachment) = state
        .with_library_mut(|db, base| {
            let note = crate::knowledge::save(
                db,
                NoteInput {
                    id: None,
                    expected_revision: None,
                    notebook_id: None,
                    title: "source".into(),
                    document: json!({"type":"doc","content":[]}),
                    tags: vec![],
                    favorite: false,
                    journal_date: None,
                },
            )?;
            let attachment = crate::knowledge_files::import(db, base, &note.id, &image)?;
            let bytes = b"# lesson\n\n![preview](source.png)\n";
            let archive = note_archive::archive_bytes(
                db,
                base,
                &ArchiveSource {
                    profile_id: &library,
                    source_key: "lesson.md",
                    version_key: "current",
                },
                bytes,
                &format!("{:x}", Sha256::digest(bytes)),
            )?;
            Ok((archive.id, attachment.id))
        })
        .unwrap();
    (root, state, library, archive, attachment)
}
fn request(archive: &str, attachment: &str) -> PrepareNotePreviewRequest {
    PrepareNotePreviewRequest {
        archive_id: archive.into(),
        destination: Destination::New {
            title: "new learning".into(),
            notebook_id: None,
        },
        attachments: vec![RegisteredAttachment {
            source_key: "source.png".into(),
            attachment_id: attachment.into(),
        }],
        courses: CourseMap::new(),
    }
}

#[test]
fn learning_note_runtime_uses_registered_uuid_keeps_filename_and_commits_once() {
    let (_root, state, library, archive, attachment) = fixture();
    let preview = prepare_in_library(&state, &library, request(&archive, &attachment)).unwrap();
    assert!(!preview.conversion.pending);
    assert_eq!(
        get_in_library(&state, &library, &preview.id)
            .unwrap()
            .status,
        "ready"
    );
    let receipt = commit_in_library(&state, &library, &preview.id).unwrap();
    assert_eq!(
        commit_in_library(&state, &library, &preview.id)
            .unwrap()
            .note_id,
        receipt.note_id
    );
    state
        .with_library(|db, base| {
            let (name, relative): (String, String) = db
                .query_row(
                    "SELECT filename,relative_path FROM knowledge_attachments WHERE note_id=?1",
                    [&receipt.note_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(name, "学习示例.png");
            assert!(base.join(relative).is_file());
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                2
            );
            Ok(())
        })
        .unwrap();
    assert!(cancel_in_library(&state, &library, &preview.id).is_err());
}

#[test]
fn learning_note_runtime_rejects_paths_other_libraries_and_stale_state() {
    let (_root, state, library, archive, attachment) = fixture();
    let wrong = "596d257d-3e5c-46c4-a464-187cf180c6bd";
    assert!(serde_json::from_value::<PrepareNotePreviewRequest>(json!({"archiveId":archive,"destination":{"kind":"new","title":"x","notebookId":null},"attachments":[],"courses":{},"path":"C:/secret.md"})).is_err());
    assert!(prepare_in_library(&state, wrong, request(&archive, &attachment)).is_err());
    assert!(prepare_in_library(&state, &library, request(&archive, "C:/secret.png")).is_err());
    let preview = prepare_in_library(&state, &library, request(&archive, &attachment)).unwrap();
    state
        .with_active_mut(|active| {
            active.marker.library_id = wrong.into();
            Ok(())
        })
        .unwrap();
    assert!(commit_in_library(&state, &library, &preview.id).is_err());
    assert!(get_in_library(&state, &library, &preview.id).is_err());
    state
        .with_active_mut(|active| {
            active.marker.library_id = library.clone();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        get_in_library(&state, &library, &preview.id)
            .unwrap()
            .status,
        "ready"
    );
    cancel_in_library(&state, &library, &preview.id).unwrap();
    assert!(commit_in_library(&state, &library, &preview.id).is_err());
    state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            Ok(())
        })
        .unwrap();
}
