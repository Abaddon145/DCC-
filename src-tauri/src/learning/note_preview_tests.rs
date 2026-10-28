use super::{
    note_archive::{self, ArchiveSource},
    note_preview::*,
};
use crate::knowledge::{self, NoteInput};
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

// Most fixtures exercise a stable library; cancellation has a separate real-core test.
#[test]
fn learning_note_runtime_cancelled_commit_keeps_preview_and_never_saves_body() {
    let (root, mut db, grants) = fixture();
    let raw = archive(&db, root.path(), b"do not save\n");
    let preview = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &raw,
        dest(),
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    assert!(super::note_preview::commit_note_preview(
        &mut db,
        root.path(),
        LIB,
        &preview.id,
        &grants,
        |tx| tx.commit().map_err(|e| e.to_string()),
        &|| true
    )
    .is_err());
    assert_eq!(
        super::note_preview::read_note_preview(&db, root.path(), LIB, &preview.id)
            .unwrap()
            .status,
        "ready"
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
fn prepare_note_preview(
    db: &Connection,
    base: &std::path::Path,
    library: &str,
    archive: &str,
    destination: Destination,
    sources: &[AttachmentSource],
    courses: &CourseMap,
    grants: &crate::media_access::LocalMediaGrants,
) -> Result<StoredNotePreview, String> {
    super::note_preview::prepare_note_preview(
        db,
        base,
        library,
        archive,
        destination,
        sources,
        courses,
        grants,
        &|| false,
    )
}
fn commit_note_preview(
    db: &mut Connection,
    base: &std::path::Path,
    library: &str,
    id: &str,
    grants: &crate::media_access::LocalMediaGrants,
    finalize: impl FnOnce(rusqlite::Transaction<'_>) -> Result<(), String>,
) -> Result<NoteCommitReceipt, String> {
    super::note_preview::commit_note_preview(db, base, library, id, grants, finalize, &|| false)
}

#[test]
fn learning_note_runtime_cancelled_copy_never_publishes_preview_or_body() {
    let (root, db, grants) = fixture();
    let raw = archive(&db, root.path(), b"[data](data.txt)\n");
    let bytes = vec![b'x'; 300 * 1024];
    let path = root.path().join("data.txt");
    fs::write(&path, &bytes).unwrap();
    let sources = vec![AttachmentSource {
        source_key: "data.txt".into(),
        path,
        sha256: hash(&bytes),
        filename: None,
    }];
    let calls = std::cell::Cell::new(0);
    let stop = || {
        let value = calls.get() + 1;
        calls.set(value);
        value >= 6
    };
    assert!(super::note_preview::prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &raw,
        dest(),
        &sources,
        &CourseMap::new(),
        &grants,
        &stop
    )
    .is_err());
    assert!(calls.get() >= 6);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM learning_note_previews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        walkdir::WalkDir::new(root.path().join("learning-data/note-previews"))
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .count(),
        0
    );
}

#[test]
fn learning_note_preview_backup_restores_bytes_but_revokes_old_approvals() {
    let source_root = tempfile::tempdir().unwrap();
    let target_root = tempfile::tempdir().unwrap();
    let source_state = crate::state::test_state(&source_root.path().join("source"));
    let target_state = crate::state::test_state(&target_root.path().join("target"));
    let library = source_state.active_library_id().unwrap();
    let backup = source_root.path().join("notes.dccassetlib");
    let (id, archive_id) = source_state
        .with_library(|db, base| {
            let raw = archive(db, base, b"# original\r\n<!-- preserved -->\r\n");
            let grants =
                crate::media_access::LocalMediaGrants::new(&source_root.path().join("config"));
            let preview = prepare_note_preview(
                db,
                base,
                &library,
                &raw,
                dest(),
                &[],
                &CourseMap::new(),
                &grants,
            )?;
            crate::backup::export_library(db, base, &backup)?;
            Ok((preview.id, raw))
        })
        .unwrap();
    target_state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    target_state
        .with_library(|db, base| {
            assert_eq!(
                db.query_row(
                    "SELECT status FROM learning_note_previews WHERE id=?1",
                    [&id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "cancelled"
            );
            assert_eq!(
                note_archive::read_archive(db, base, &archive_id).unwrap(),
                b"# original\r\n<!-- preserved -->\r\n"
            );
            assert_eq!(
                db.query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "32"
            );
            Ok(())
        })
        .unwrap();
    source_state
        .with_library(|db, _| {
            assert_eq!(
                db.query_row(
                    "SELECT status FROM learning_note_previews WHERE id=?1",
                    [&id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "ready"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn learning_note_preview_v27_upgrade_keeps_note_json_and_raw_registration() {
    let (root, mut db, grants) = fixture();
    let raw = archive(&db, root.path(), b"legacy source\r\n");
    let p = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &raw,
        dest(),
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    let receipt = apply(&mut db, root.path(), &p.id, &grants).unwrap();
    let before = crate::knowledge::get(&db, &receipt.note_id).unwrap();
    db.execute_batch("DROP TABLE IF EXISTS learning_origin_map; DROP TABLE learning_source_profiles; DROP TABLE learning_note_previews; UPDATE schema_meta SET value='27' WHERE key='schema_version'").unwrap();
    super::migrate(&db).unwrap();
    super::migrate(&db).unwrap();
    let after = crate::knowledge::get(&db, &before.id).unwrap();
    assert_eq!(before.document, after.document);
    assert_eq!(before.revision, after.revision);
    assert_eq!(before.updated_at, after.updated_at);
    assert_eq!(
        note_archive::read_archive(&db, root.path(), &raw).unwrap(),
        b"legacy source\r\n"
    );
    assert_eq!(
        db.query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "32"
    );
}

#[test]
fn learning_note_preview_rejects_tampered_snapshots_sources_and_staging_without_overwriting() {
    let (root, mut db, grants) = fixture();
    let raw = archive(&db, root.path(), b"![snapshot](a.png)\n");
    let source = root.path().join("a.png");
    image::RgbaImage::new(2, 2).save(&source).unwrap();
    let original = fs::read(&source).unwrap();
    let mut sources = vec![AttachmentSource {
        filename: None,
        source_key: "a.png".into(),
        path: source.clone(),
        sha256: hash(b"wrong"),
    }];
    assert!(prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &raw,
        dest(),
        &sources,
        &CourseMap::new(),
        &grants
    )
    .is_err());
    sources[0].sha256 = hash(&original);
    db.execute_batch("CREATE TRIGGER fail_preview_insert BEFORE INSERT ON learning_note_previews BEGIN SELECT RAISE(ABORT,'fail'); END;").unwrap();
    assert!(prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &raw,
        dest(),
        &sources,
        &CourseMap::new(),
        &grants
    )
    .is_err());
    assert_eq!(
        walkdir::WalkDir::new(root.path().join("learning-data/note-previews"))
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .count(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_preview_insert")
        .unwrap();
    let p = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &raw,
        dest(),
        &sources,
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    let staged = walkdir::WalkDir::new(root.path().join("learning-data/note-previews"))
        .into_iter()
        .flatten()
        .find(|e| e.file_type().is_file())
        .unwrap()
        .path()
        .to_owned();
    fs::write(&staged, b"changed").unwrap();
    assert!(apply(&mut db, root.path(), &p.id, &grants).is_err());
    assert_eq!(fs::read(&staged).unwrap(), b"changed");
    fs::write(&staged, &original).unwrap();
    db.execute(
        "UPDATE learning_note_previews SET payload_json='{}' WHERE id=?1",
        [&p.id],
    )
    .unwrap();
    assert!(apply(&mut db, root.path(), &p.id, &grants).is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        note_archive::read_archive(&db, root.path(), &raw).unwrap(),
        b"![snapshot](a.png)\n"
    );
}

#[test]
fn learning_note_preview_media_identity_is_real_and_changed_original_blocks_commit() {
    let (root, mut db, grants) = fixture();
    let media = uuid::Uuid::new_v4().to_string();
    let file = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'video','synthetic',?2,'now','now')",rusqlite::params![media,file]).unwrap();
    db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,duration_ms,created_at) VALUES(?1,?2,'main','clip.mp4','clip.mp4','fixture.mp4',60000,'now')",rusqlite::params![file,media]).unwrap();
    fs::write(
        root.path().join("fixture.mp4"),
        b"synthetic source bytes, no decoder required",
    )
    .unwrap();
    let course = super::store::ensure_course(&db, &media).unwrap();
    let courses = CourseMap::from([("source-id".into(), course.id.clone())]);
    let raw = archive(
        &db,
        root.path(),
        b"[time](yingliu://video/source-id?t=1000)\n",
    );
    let p =
        prepare_note_preview(&db, root.path(), LIB, &raw, dest(), &[], &courses, &grants).unwrap();
    let attrs = &p.conversion.document.as_ref().unwrap()["content"][0]["attrs"];
    assert_eq!(attrs["mediaId"], media);
    assert_eq!(attrs["courseId"], course.id);
    assert_eq!(
        attrs["fingerprint"],
        hash(fs::read(root.path().join("fixture.mp4")).unwrap())
    );
    fs::write(root.path().join("fixture.mp4"), b"changed source").unwrap();
    assert!(apply(&mut db, root.path(), &p.id, &grants).is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn learning_note_preview_batch_ranges_are_atomic_and_reuse_ids() {
    let (root, db, _grants) = fixture();
    let source = "原字节😀\r\n".repeat(40_000);
    let raw = archive(&db, root.path(), source.as_bytes());
    assert_eq!("原字节😀\r\n".len(), 15);
    let items = (0..1000)
        .map(|i| (i * 15, (i + 1) * 15, "raw span"))
        .collect::<Vec<_>>();
    let first = note_archive::add_archive_items(&db, root.path(), &raw, &items).unwrap();
    assert_eq!(first.len(), 1000);
    let again = note_archive::add_archive_items(&db, root.path(), &raw, &items).unwrap();
    assert_eq!(first[99].id, again[99].id);
    let before: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM learning_note_archive_items",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(note_archive::add_archive_items(
        &db,
        root.path(),
        &raw,
        &[(15000, 15015, "valid"), (15016, 15017, "splits unicode")]
    )
    .is_err());
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM learning_note_archive_items",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        before
    );
    db.execute_batch("CREATE TRIGGER fail_second_span BEFORE INSERT ON learning_note_archive_items WHEN NEW.start_byte=15015 BEGIN SELECT RAISE(ABORT,'span failure'); END").unwrap();
    assert!(note_archive::add_archive_items(
        &db,
        root.path(),
        &raw,
        &[
            (15000, 15015, "first new row"),
            (15015, 15030, "second new row")
        ]
    )
    .is_err());
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM learning_note_archive_items",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        before
    );
}

const LIB: &str = "596d257d-3e5c-46c4-a464-187cf180c6bd";
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn fixture() -> (
    tempfile::TempDir,
    Connection,
    crate::media_access::LocalMediaGrants,
) {
    let root = tempfile::tempdir().unwrap();
    let db = crate::db::open_database(&root.path().join("library.db")).unwrap();
    fs::write(
        root.path().join("library.json"),
        serde_json::to_vec(&crate::state::LibraryMarker {
            format_version: 1,
            library_id: LIB.into(),
            name: "isolated".into(),
            created_at: "now".into(),
        })
        .unwrap(),
    )
    .unwrap();
    let grants = crate::media_access::LocalMediaGrants::new(&root.path().join("test-config"));
    (root, db, grants)
}
fn archive(db: &Connection, base: &std::path::Path, source: &[u8]) -> String {
    note_archive::archive_bytes(
        db,
        base,
        &ArchiveSource {
            profile_id: LIB,
            source_key: "vault/学习.md",
            version_key: "current",
        },
        source,
        &hash(source),
    )
    .unwrap()
    .id
}
fn dest() -> Destination {
    Destination::New {
        title: "迁入课程".into(),
        notebook_id: None,
    }
}
fn input(note: &knowledge::Note, text: &str) -> NoteInput {
    NoteInput {
        id: Some(note.id.clone()),
        expected_revision: Some(note.revision),
        notebook_id: note.notebook_id.clone(),
        title: note.title.clone(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]}),
        tags: note.tags.clone(),
        favorite: note.favorite,
        journal_date: note.journal_date.clone(),
    }
}
fn apply(
    db: &mut Connection,
    base: &std::path::Path,
    id: &str,
    grants: &crate::media_access::LocalMediaGrants,
) -> Result<NoteCommitReceipt, String> {
    commit_note_preview(db, base, LIB, id, grants, |tx| {
        tx.commit().map_err(|e| e.to_string())
    })
}

#[test]
fn learning_note_preview_commits_raw_nodes_once_and_does_not_overwrite_later_edits() {
    let (root, mut db, grants) = fixture();
    let raw = b"# lesson\r\n\r\n<!-- keep -->\r\n";
    let archive_id = archive(&db, root.path(), raw);
    let preview = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &archive_id,
        dest(),
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    assert_eq!(preview.status, "ready");
    assert!(preview.conversion.pending);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let duplicate = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &archive_id,
        dest(),
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    assert_eq!(duplicate.id, preview.id);
    let receipt = apply(&mut db, root.path(), &preview.id, &grants).unwrap();
    assert!(receipt.pending);
    let note = knowledge::get(&db, &receipt.note_id).unwrap();
    let item = super::note_nodes::archive_ids(&note.document)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(
        super::note_nodes::read_preserved(&db, root.path(), &note.id, &item).unwrap(),
        "<!-- keep -->\r\n"
    );
    let edited = knowledge::save(&mut db, input(&note, "用户后续修改")).unwrap();
    drop(db);
    let mut db = crate::db::open_database(&root.path().join("library.db")).unwrap();
    let again = apply(&mut db, root.path(), &preview.id, &grants).unwrap();
    assert_eq!(again.note_id, receipt.note_id);
    assert_eq!(again.revision, receipt.revision);
    assert_eq!(
        knowledge::get(&db, &note.id).unwrap().document,
        edited.document
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn learning_note_preview_conflicts_and_final_gate_failures_keep_both_materials() {
    let (root, mut db, grants) = fixture();
    let archive_id = archive(&db, root.path(), b"new source\n");
    let first = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &archive_id,
        dest(),
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    let receipt = apply(&mut db, root.path(), &first.id, &grants).unwrap();
    let old = knowledge::get(&db, &receipt.note_id).unwrap();
    let second = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &archive_id,
        Destination::Replace {
            note_id: old.id.clone(),
            expected_revision: old.revision,
        },
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    let edited = knowledge::save(&mut db, input(&old, "keep newer")).unwrap();
    assert!(apply(&mut db, root.path(), &second.id, &grants).is_err());
    assert_eq!(
        knowledge::get(&db, &old.id).unwrap().document,
        edited.document
    );
    let third = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &archive_id,
        Destination::New {
            title: "另存".into(),
            notebook_id: None,
        },
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    assert!(
        commit_note_preview(&mut db, root.path(), LIB, &third.id, &grants, |_| Err(
            "素材库已切换".into()
        ))
        .is_err()
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(apply(&mut db, root.path(), &third.id, &grants).is_ok());
    assert_eq!(
        note_archive::read_archive(&db, root.path(), &archive_id).unwrap(),
        b"new source\n"
    );
}

#[test]
fn learning_note_preview_attachments_are_snapshots_and_failed_commit_cleans_only_new_files() {
    let (root, mut db, grants) = fixture();
    let archive_id = archive(&db, root.path(), b"![preview](image.png)\n");
    let file = root.path().join("source.png");
    image::RgbaImage::new(2, 2).save(&file).unwrap();
    let sha = hash(fs::read(&file).unwrap());
    let sources = vec![AttachmentSource {
        filename: None,
        source_key: "image.png".into(),
        path: file.clone(),
        sha256: sha,
    }];
    let preview = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &archive_id,
        dest(),
        &sources,
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    assert!(!preview.conversion.pending);
    fs::remove_file(file).unwrap();
    db.execute_batch("CREATE TRIGGER fail_note_update BEFORE UPDATE ON knowledge_notes BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(apply(&mut db, root.path(), &preview.id, &grants).is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_attachments", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let files = walkdir::WalkDir::new(root.path().join("note-attachments"))
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .count();
    assert_eq!(files, 0);
    db.execute_batch("DROP TRIGGER fail_note_update;").unwrap();
    let receipt = apply(&mut db, root.path(), &preview.id, &grants).unwrap();
    let note = knowledge::get(&db, &receipt.note_id).unwrap();
    let id = note.document["content"][0]["attrs"]["src"]
        .as_str()
        .unwrap()
        .strip_prefix("http://dcc-media.localhost/note/")
        .unwrap();
    let (path, _) = crate::knowledge_files::path(&db, root.path(), id).unwrap();
    assert!(path.is_file());
}

#[test]
fn learning_note_preview_cancel_library_identity_and_invalid_encoding_never_create_blank_success() {
    let (root, mut db, grants) = fixture();
    let archive_id = archive(&db, root.path(), b"note\n");
    let preview = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &archive_id,
        dest(),
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    let wrong = "5804245d-e1d3-4297-b08c-053a123706bd";
    let moved = tempfile::tempdir().unwrap();
    let (busy, log, checkpointed): (i64, i64, i64) = db
        .query_row("PRAGMA wal_checkpoint(FULL)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    assert_eq!(busy, 0);
    assert_eq!(log, checkpointed);
    fs::copy(
        root.path().join("library.db"),
        moved.path().join("library.db"),
    )
    .unwrap();
    fs::copy(
        root.path().join("library.json"),
        moved.path().join("library.json"),
    )
    .unwrap();
    let mut moved_db = crate::db::open_database(&moved.path().join("library.db")).unwrap();
    assert!(apply(&mut moved_db, moved.path(), &preview.id, &grants)
        .unwrap_err()
        .contains("位置"));
    assert!(
        commit_note_preview(&mut db, root.path(), wrong, &preview.id, &grants, |tx| tx
            .commit()
            .map_err(|e| e.to_string()))
        .is_err()
    );
    cancel_note_preview(&db, root.path(), LIB, &preview.id).unwrap();
    assert!(apply(&mut db, root.path(), &preview.id, &grants).is_err());
    let invalid = archive(&db, root.path(), &[0xff, 0x80]);
    let p = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &invalid,
        dest(),
        &[],
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    assert!(p.conversion.document.is_none());
    assert!(apply(&mut db, root.path(), &p.id, &grants).is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_notes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn learning_note_preview_lost_ack_keeps_committed_attachments_and_returns_saved_receipt() {
    let (root, mut db, grants) = fixture();
    let raw = archive(&db, root.path(), b"![snapshot](image.png)\n");
    let source = root.path().join("image.png");
    image::RgbaImage::new(2, 2).save(&source).unwrap();
    let sources = vec![AttachmentSource {
        filename: None,
        source_key: "image.png".into(),
        path: source.clone(),
        sha256: hash(fs::read(source).unwrap()),
    }];
    let preview = prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &raw,
        dest(),
        &sources,
        &CourseMap::new(),
        &grants,
    )
    .unwrap();
    let receipt = commit_note_preview(&mut db, root.path(), LIB, &preview.id, &grants, |tx| {
        tx.commit().map_err(|e| e.to_string())?;
        Err("simulated lost ack".into())
    })
    .unwrap();
    let note = knowledge::get(&db, &receipt.note_id).unwrap();
    let src = note.document["content"][0]["attrs"]["src"]
        .as_str()
        .unwrap();
    assert!(crate::knowledge_files::path(
        &db,
        root.path(),
        src.strip_prefix("http://dcc-media.localhost/note/")
            .unwrap()
    )
    .unwrap()
    .0
    .is_file());
    assert_eq!(
        apply(&mut db, root.path(), &preview.id, &grants)
            .unwrap()
            .note_id,
        receipt.note_id
    );
}

#[test]
fn learning_note_preview_checks_image_bytes_before_presenting_an_importable_preview() {
    let (root, db, grants) = fixture();
    let raw = archive(&db, root.path(), b"![bad](bad.png)\n");
    let path = root.path().join("bad.png");
    fs::write(&path, b"not an image").unwrap();
    let sources = vec![AttachmentSource {
        filename: None,
        source_key: "bad.png".into(),
        path,
        sha256: hash(b"not an image"),
    }];
    assert!(prepare_note_preview(
        &db,
        root.path(),
        LIB,
        &raw,
        dest(),
        &sources,
        &CourseMap::new(),
        &grants
    )
    .is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM learning_note_previews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        walkdir::WalkDir::new(root.path().join("learning-data/note-previews"))
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .count(),
        0
    );
}
