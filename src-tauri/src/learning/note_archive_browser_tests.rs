use super::{
    note_archive::{self, ArchiveSource},
    note_preview::{CourseMap, Destination},
    note_preview_runtime::*,
};
use crate::state::AppState;
use sha2::{Digest, Sha256};

fn fixture() -> (tempfile::TempDir, AppState, String) {
    let root = tempfile::tempdir().unwrap();
    let state = crate::state::test_state(&root.path().join("library"));
    let library = state.active_library_id().unwrap();
    (root, state, library)
}
fn archive(state: &AppState, library: &str, key: &str, bytes: &[u8]) -> String {
    state
        .with_library(|db, base| {
            note_archive::archive_bytes(
                db,
                base,
                &ArchiveSource {
                    profile_id: library,
                    source_key: key,
                    version_key: "current",
                },
                bytes,
                &format!("{:x}", Sha256::digest(bytes)),
            )
        })
        .unwrap()
        .id
}
fn convert(state: &AppState, library: &str, id: &str) {
    let preview = prepare_in_library(
        state,
        library,
        PrepareNotePreviewRequest {
            archive_id: id.into(),
            destination: Destination::New {
                title: "lesson".into(),
                notebook_id: None,
            },
            attachments: vec![],
            courses: CourseMap::new(),
        },
    )
    .unwrap();
    commit_in_library(state, library, &preview.id).unwrap();
}

#[test]
fn learning_note_archive_browser_counts_partial_and_unconverted_without_loading_all_bodies() {
    let (root, state, library) = fixture();
    let empty = list_archives_in_library(&state, &library, 0, 50).unwrap();
    assert_eq!(
        (
            empty.versions.total,
            empty.unconverted_count,
            empty.partial_count
        ),
        (0, 0, 0)
    );
    let full = archive(&state, &library, "full.md", b"# full\n");
    let partial = archive(&state, &library, "partial.md", b"<section>raw</section>\n");
    let invalid = archive(&state, &library, "invalid.md", b"\xff\xfe\x00");
    convert(&state, &library, &full);
    convert(&state, &library, &partial);
    let page = list_archives_in_library(&state, &library, 0, 50).unwrap();
    assert_eq!(
        (
            page.versions.total,
            page.unconverted_count,
            page.partial_count
        ),
        (3, 2, 1)
    );
    for (id, want) in [
        (&full, "converted"),
        (&partial, "partial"),
        (&invalid, "unconverted"),
    ] {
        assert_eq!(
            page.versions
                .items
                .iter()
                .find(|row| &row.id == id)
                .unwrap()
                .conversion_state,
            want
        );
    }
    let pages = (0..3)
        .map(|offset| {
            list_archives_in_library(&state, &library, offset, 1)
                .unwrap()
                .versions
                .items
                .remove(0)
                .id
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pages,
        page.versions
            .items
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>()
    );
    let page = list_archives_in_library(&state, &library, -5, 999).unwrap();
    assert_eq!((page.versions.offset, page.versions.limit), (0, 50));
    assert!(read_archive_in_library(&state, &library, &invalid)
        .unwrap()
        .text
        .is_none());
    let binary = root.path().join("invalid.bin");
    export_archive_in_library(&state, &library, &invalid, &binary).unwrap();
    assert_eq!(std::fs::read(binary).unwrap(), b"\xff\xfe\x00");
    state
        .with_library(|db, _| {
            db.execute(
                "UPDATE learning_note_previews SET receipt_json='not-json' WHERE archive_id=?1",
                [&full],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    let uncertain = list_archives_in_library(&state, &library, 0, 50).unwrap();
    assert_eq!(
        (uncertain.unconverted_count, uncertain.partial_count),
        (3, 2)
    );
    assert!(read_archive_in_library(&state, &uuid::Uuid::new_v4().to_string(), &full).is_err());
}

#[test]
fn learning_note_archive_browser_exports_exact_bytes_without_overwriting_or_writing_library() {
    let (root, state, library) = fixture();
    let bytes = b"\xef\xbb\xbf# title\r\n\r\n<section>raw</section>\r\n";
    let id = archive(&state, &library, "raw.md", bytes);
    let view = read_archive_in_library(&state, &library, &id).unwrap();
    assert_eq!(view.text.unwrap().as_bytes(), bytes);
    assert_eq!(view.size, bytes.len() as u64);
    let target = root.path().join("original.md");
    export_archive_in_library(&state, &library, &id, &target).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    assert!(export_archive_in_library(&state, &library, &id, &target).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    let inside = root.path().join("library/should-not-exist.md");
    assert!(export_archive_in_library(&state, &library, &id, &inside).is_err());
    assert!(!inside.exists());
    assert!(
        export_archive_in_library(&state, &library, &id, &root.path().join("unsafe.html")).is_err()
    );
    let wrong = root.path().join("wrong.md");
    assert!(
        export_archive_in_library(&state, &uuid::Uuid::new_v4().to_string(), &id, &wrong).is_err()
    );
    assert!(!wrong.exists());
    state
        .with_library(|db, base| {
            let relative: String = db
                .query_row(
                    "SELECT relative_path FROM learning_note_archives WHERE id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .unwrap();
            std::fs::write(base.join(relative), b"changed").unwrap();
            Ok(())
        })
        .unwrap();
    assert!(read_archive_in_library(&state, &library, &id).is_err());
    let changed = root.path().join("changed.bin");
    assert!(export_archive_in_library(&state, &library, &id, &changed).is_err());
    assert!(!changed.exists());
}

#[test]
fn learning_note_archive_browser_restored_raw_versions_remain_readable_without_old_approval() {
    let (root, state, library) = fixture();
    let bytes = b"\xef\xbb\xbf# restored\r\n";
    let id = archive(&state, &library, "history.md", bytes);
    let preview = prepare_in_library(
        &state,
        &library,
        PrepareNotePreviewRequest {
            archive_id: id.clone(),
            destination: Destination::New {
                title: "restored".into(),
                notebook_id: None,
            },
            attachments: vec![],
            courses: CourseMap::new(),
        },
    )
    .unwrap();
    let backup = root.path().join("backup.dccassetlib");
    state
        .with_library(|db, base| crate::backup::export_library(db, base, &backup))
        .unwrap();
    let target = crate::state::test_state(&root.path().join("restored"));
    let target_library = target.active_library_id().unwrap();
    target
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    assert!(get_in_library(&target, &target_library, &preview.id).is_err());
    assert_eq!(
        read_archive_in_library(&target, &target_library, &id)
            .unwrap()
            .text
            .unwrap()
            .as_bytes(),
        bytes
    );
    assert_eq!(
        list_archives_in_library(&target, &target_library, 0, 50)
            .unwrap()
            .unconverted_count,
        1
    );
}
