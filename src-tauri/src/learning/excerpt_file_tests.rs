use super::{excerpt_files::*, excerpt_runtime, excerpt_store};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::fs;
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn fixture(
    board: bool,
) -> (
    tempfile::TempDir,
    crate::state::AppState,
    PublicationManifest,
    String,
) {
    let (root, state, library, course, mut selection) = super::excerpt_runtime_tests::fixture(true);
    selection.frame_time_ms = Some(400);
    let prepared =
        excerpt_runtime::prepare_in_library(&state, &library, &course, &selection, None, &|| false)
            .unwrap();
    let preview = state
        .with_library(|db, base| excerpt_store::save_prepared(db, base, &library, &prepared))
        .unwrap();
    let operation = uuid::Uuid::new_v4().to_string();
    let frame = prepare_files(
        root.path(),
        &operation,
        &library,
        &prepared.root_hash,
        prepared.frame.as_ref().unwrap(),
    )
    .unwrap();
    let target = if board {
        FileDestination::Board {
            board_id: state
                .with_library(|db, _| crate::reference_boards::create(db, "截图参考板"))
                .unwrap()
                .id,
        }
    } else {
        FileDestination::Note {
            note_id: uuid::Uuid::new_v4().to_string(),
        }
    };
    let (manifest, locator) = create_manifest(
        root.path(),
        &operation,
        &preview.id,
        &library,
        &prepared.root_hash,
        &hash(b"{}"),
        target,
        &frame,
    )
    .unwrap();
    state.with_library(|db,_|{
        db.execute("INSERT INTO learning_excerpt_operations(operation_id,excerpt_id,request_json,request_sha256,status,manifest_relative,created_at) VALUES(?1,?2,'{}',?3,'pending',?4,'now')",params![operation,preview.id,hash(b"{}"),locator]).unwrap();Ok(())
    }).unwrap();
    (root, state, manifest, locator)
}
#[test]
fn learning_excerpt_files_real_frame_and_manifest_preserve_source_and_recover_partial_publication()
{
    let (root, _state, manifest, locator) = fixture(true);
    let original = fs::read(root.path().join("fixture.mp4")).unwrap();
    let loaded = read_manifest(
        root.path(),
        &manifest.operation_id,
        &manifest.excerpt_id,
        &manifest.library_id,
        &manifest.request_sha256,
        &locator,
    )
    .unwrap();
    assert_eq!(loaded.image_id, manifest.image_id);
    assert_eq!(loaded.files.len(), 2);
    // Simulate a crash after the first complete file but before the final transaction.
    let first = &manifest.files[0];
    let target = root.path().join(&first.final_relative);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(root.path().join(&first.staged_relative), &target).unwrap();
    publish_files(root.path(), &manifest).unwrap();
    publish_files(root.path(), &manifest).unwrap();
    for file in &manifest.files {
        let bytes = fs::read(root.path().join(&file.final_relative)).unwrap();
        assert_eq!(bytes.len() as u64, file.size);
        assert_eq!(hash(&bytes), file.sha256);
    }
    assert_eq!(fs::read(root.path().join("fixture.mp4")).unwrap(), original);
}
#[test]
fn learning_excerpt_files_tampered_locator_root_and_staged_image_are_rejected_without_overwrite() {
    let (root, _state, mut manifest, locator) = fixture(false);
    assert!(read_manifest(
        root.path(),
        &manifest.operation_id,
        &manifest.excerpt_id,
        &manifest.library_id,
        &manifest.request_sha256,
        "../library.db"
    )
    .is_err());
    let valid_manifest = fs::read(root.path().join(&locator)).unwrap();
    fs::write(root.path().join(&locator), b"{}").unwrap();
    assert!(read_manifest(
        root.path(),
        &manifest.operation_id,
        &manifest.excerpt_id,
        &manifest.library_id,
        &manifest.request_sha256,
        &locator
    )
    .is_err());
    fs::write(root.path().join(&locator), &valid_manifest).unwrap();
    let staged = root.path().join(&manifest.files[0].staged_relative);
    fs::write(staged, b"tampered").unwrap();
    assert!(publish_files(root.path(), &manifest).is_err());
    assert!(!root.path().join(&manifest.files[0].final_relative).exists());
    manifest.root_hash = "f".repeat(64);
    assert!(publish_files(root.path(), &manifest).is_err());
    manifest.files[0].final_relative = "../outside.jpg".into();
    assert!(publish_files(root.path(), &manifest).is_err());
}
#[test]
fn learning_excerpt_files_repeated_preparation_does_not_overwrite_and_copied_roots_are_not_authorized(
) {
    use base64::Engine;
    let (root, _state, manifest, locator) = fixture(false);
    let bytes = fs::read(root.path().join(&manifest.files[0].staged_relative)).unwrap();
    let frame = super::excerpt_runtime::ExcerptFrame {
        time_ms: 400,
        sha256: hash(&bytes),
        data_url: format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ),
    };
    let again = prepare_files(
        root.path(),
        &manifest.operation_id,
        &manifest.library_id,
        &manifest.root_hash,
        &frame,
    )
    .unwrap();
    assert_eq!(again.original.sha256, frame.sha256);
    let mut forged = frame.clone();
    forged.sha256 = "f".repeat(64);
    assert!(prepare_files(
        root.path(),
        &manifest.operation_id,
        &manifest.library_id,
        &manifest.root_hash,
        &forged
    )
    .is_err());
    let mut changed = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64, 48, image::Rgb([255, 0, 0])))
        .write_to(&mut changed, image::ImageFormat::Jpeg)
        .unwrap();
    let changed = changed.into_inner();
    forged.sha256 = hash(&changed);
    forged.data_url = format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&changed)
    );
    assert!(prepare_files(
        root.path(),
        &manifest.operation_id,
        &manifest.library_id,
        &manifest.root_hash,
        &forged
    )
    .is_err());
    assert_eq!(
        fs::read(root.path().join(&again.original.relative)).unwrap(),
        bytes
    );
    let copied = tempfile::tempdir().unwrap();
    fs::copy(
        root.path().join("library.json"),
        copied.path().join("library.json"),
    )
    .unwrap();
    let target = copied.path().join(&locator);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(root.path().join(&locator), &target).unwrap();
    assert!(read_manifest(
        copied.path(),
        &manifest.operation_id,
        &manifest.excerpt_id,
        &manifest.library_id,
        &manifest.request_sha256,
        &locator
    )
    .is_err());
}
#[test]
fn learning_excerpt_files_cleanup_never_deletes_registered_or_changed_files() {
    let (root, state, manifest, _) = fixture(false);
    publish_files(root.path(), &manifest).unwrap();
    let file = &manifest.files[0];
    let path = root.path().join(&file.final_relative);
    let before = fs::read(&path).unwrap();
    let FileDestination::Note { note_id } = &manifest.target else {
        panic!("note target")
    };
    state.with_library(|db,_|{
        // A separately registered owner must stop recovery cleanup.
        db.execute("INSERT INTO knowledge_notes(id,title,document_json,plain_text,tags_json,favorite,revision,created_at,updated_at) VALUES(?1,'kept','{}','','[]',0,1,'now','now')",[note_id]).unwrap();
        db.execute("INSERT INTO knowledge_attachments(id,note_id,filename,relative_path,mime,sha256,size,created_at) VALUES(?1,?2,'frame.jpg',?3,'image/jpeg',?4,?5,'now')",params![manifest.image_id,note_id,file.final_relative,file.sha256,file.size]).unwrap();
        assert!(cleanup_uncommitted(db,root.path(),&manifest).is_err());assert_eq!(fs::read(&path).unwrap(),before);
        db.execute("DELETE FROM knowledge_attachments WHERE id=?1",[&manifest.image_id]).unwrap();
        fs::write(&path,b"user-edited bytes").unwrap();assert!(cleanup_uncommitted(db,root.path(),&manifest).is_err());assert_eq!(fs::read(&path).unwrap(),b"user-edited bytes");
        fs::write(&path,&before).unwrap();cleanup_uncommitted(db,root.path(),&manifest)?;assert!(!path.exists());assert!(root.path().join(&file.staged_relative).exists());
        Ok(())
    }).unwrap();
}
#[test]
fn learning_excerpt_files_retry_keeps_preexisting_mismatched_targets_and_committed_receipts() {
    let (root, state, manifest, _) = fixture(true);
    let file = &manifest.files[0];
    let path = root.path().join(&file.final_relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"already belongs to someone else").unwrap();
    assert!(publish_files(root.path(), &manifest).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"already belongs to someone else");
    fs::remove_file(&path).unwrap();
    publish_files(root.path(), &manifest).unwrap();
    state.with_library(|db,_|{
        db.execute("UPDATE learning_excerpt_operations SET status='committed',receipt_json='{}' WHERE operation_id=?1",[&manifest.operation_id]).unwrap();
        assert!(cleanup_uncommitted(db,root.path(),&manifest).is_err());assert!(path.exists());Ok(())
    }).unwrap();
}
