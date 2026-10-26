use super::note_archive::*;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::fs;

const PROFILE: &str = "596d257d-3e5c-46c4-a464-187cf180c6bd";
fn source() -> ArchiveSource<'static> {
    ArchiveSource {
        profile_id: PROFILE,
        source_key: "vault-id/笔记.md",
        version_key: "current",
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture() -> (tempfile::TempDir, Connection) {
    let root = tempfile::tempdir().unwrap();
    let db = crate::db::open_database(&root.path().join("library.db")).unwrap();
    (root, db)
}
fn stored(db: &Connection, root: &std::path::Path, bytes: &[u8]) -> NoteArchive {
    archive_bytes(db, root, &source(), bytes, &digest(bytes)).unwrap()
}
fn archive_path(root: &std::path::Path, id: &str) -> std::path::PathBuf {
    root.join(format!("learning-data/archives/{id}.bin"))
}

#[test]
fn learning_note_archive_preserves_bom_crlf_html_and_exact_utf8_segments() {
    let (root, db) = fixture();
    let bytes =
        "\u{feff}---\r\n标题: UE5\r\n---\r\n<script>unsafe()</script>\r\n![[missing.png]]\r\n"
            .as_bytes();
    let archived = stored(&db, root.path(), bytes);
    assert_eq!(read_archive(&db, root.path(), &archived.id).unwrap(), bytes);
    assert_eq!(
        fs::read(archive_path(root.path(), &archived.id)).unwrap(),
        bytes
    );
    assert_eq!(archived.size, bytes.len() as u64);
    let item = add_archive_item(&db, root.path(), &archived.id, 8, 14, "原始属性").unwrap();
    assert_eq!(
        read_archive_item(&db, root.path(), &item.id).unwrap(),
        "标题".as_bytes()
    );
    assert_eq!(
        add_archive_item(&db, root.path(), &archived.id, 8, 14, "别名")
            .unwrap()
            .id,
        item.id
    );
    // The second byte lies inside the BOM; accepting it would irreversibly misdecode the protected block.
    assert!(add_archive_item(&db, root.path(), &archived.id, 1, 14, "坏边界").is_err());
    assert!(add_archive_item(&db, root.path(), &archived.id, 14, 8, "反向").is_err());
    assert!(add_archive_item(&db, root.path(), &archived.id, 0, u64::MAX, "越界").is_err());
    assert!(add_archive_item(&db, root.path(), &archived.id, 0, 3, "\0").is_err());
}

#[test]
fn learning_note_archive_reuses_only_same_source_version_and_verified_bytes() {
    let (root, db) = fixture();
    let first = stored(&db, root.path(), b"first");
    assert_eq!(stored(&db, root.path(), b"first").id, first.id);
    let changed = stored(&db, root.path(), b"second");
    assert_ne!(changed.id, first.id);
    let history = archive_bytes(
        &db,
        root.path(),
        &ArchiveSource {
            version_key: "history-1",
            ..source()
        },
        b"first",
        &digest(b"first"),
    )
    .unwrap();
    let other = archive_bytes(
        &db,
        root.path(),
        &ArchiveSource {
            profile_id: "b01c829f-de85-4671-8464-5e7db03edf8a",
            ..source()
        },
        b"first",
        &digest(b"first"),
    )
    .unwrap();
    assert_ne!(history.id, first.id);
    assert_ne!(other.id, first.id);
    fs::write(archive_path(root.path(), &first.id), b"wrong").unwrap();
    assert!(archive_bytes(&db, root.path(), &source(), b"first", &digest(b"first")).is_err());
    assert_eq!(
        fs::read(archive_path(root.path(), &first.id)).unwrap(),
        b"wrong"
    );
    assert_eq!(
        read_archive(&db, root.path(), &changed.id).unwrap(),
        b"second"
    );
}

#[test]
fn learning_note_archive_rejects_unregistered_cross_library_and_tampered_paths() {
    let (root, db) = fixture();
    let (other_root, other_db) = fixture();
    let archived = stored(&db, root.path(), b"secret original");
    assert!(read_archive(&other_db, other_root.path(), &archived.id).is_err());
    assert!(read_archive(&db, root.path(), "../../library.db").is_err());
    db.execute("UPDATE learning_note_archives SET relative_path='learning-data/archives/../../library.db' WHERE id=?1", [&archived.id]).unwrap();
    assert!(read_archive(&db, root.path(), &archived.id).is_err());
    assert!(add_archive_item(&db, root.path(), &archived.id, 0, 6, "坏路径").is_err());
}

#[test]
fn learning_note_archive_failed_publication_and_outer_rollback_leave_no_owned_file() {
    let (root, db) = fixture();
    db.execute_batch("CREATE TRIGGER reject_archive BEFORE INSERT ON learning_note_archives BEGIN SELECT RAISE(ABORT,'fixture database rejection'); END;").unwrap();
    assert!(archive_bytes(
        &db,
        root.path(),
        &source(),
        b"original",
        &digest(b"original")
    )
    .is_err());
    assert_eq!(
        fs::read_dir(root.path().join("learning-data/archives"))
            .unwrap()
            .count(),
        0
    );
    db.execute_batch("DROP TRIGGER reject_archive; BEGIN;")
        .unwrap();
    assert!(archive_bytes(
        &db,
        root.path(),
        &source(),
        b"original",
        &digest(b"original")
    )
    .is_err());
    db.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(
        fs::read_dir(root.path().join("learning-data/archives"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM learning_note_archives", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(stored(&db, root.path(), b"original").size, 8);
}

#[test]
fn learning_note_archive_real_six_hundred_kib_io_and_bounded_invalid_encoding() {
    let (root, db) = fixture();
    let bytes = b"# UE5\r\n"
        .iter()
        .copied()
        .cycle()
        .take(600 * 1024)
        .collect::<Vec<_>>();
    let archived = stored(&db, root.path(), &bytes);
    assert_eq!(read_archive(&db, root.path(), &archived.id).unwrap(), bytes);
    let item = add_archive_item(
        &db,
        root.path(),
        &archived.id,
        0,
        bytes.len() as u64,
        "全文",
    )
    .unwrap();
    assert_eq!(
        read_archive_item(&db, root.path(), &item.id).unwrap(),
        bytes
    );
    let invalid = stored(&db, root.path(), &[0xff, 0xfe, 0x00]);
    assert_eq!(
        read_archive(&db, root.path(), &invalid.id).unwrap(),
        [0xff, 0xfe, 0x00]
    );
    assert!(add_archive_item(&db, root.path(), &invalid.id, 0, 2, "不可解码").is_err());
    let huge = vec![0u8; 5 * 1024 * 1024 + 1];
    assert!(archive_bytes(&db, root.path(), &source(), &huge, &digest(&huge)).is_err());
    let exact = &huge[..5 * 1024 * 1024];
    let at_limit = stored(&db, root.path(), exact);
    assert_eq!(read_archive(&db, root.path(), &at_limit.id).unwrap(), exact);
    assert!(archive_bytes(
        &db,
        root.path(),
        &source(),
        b"changed",
        &digest(b"original")
    )
    .is_err());
    assert!(archive_bytes(
        &db,
        root.path(),
        &ArchiveSource {
            profile_id: "not-uuid",
            ..source()
        },
        b"",
        &digest(b"")
    )
    .is_err());
    assert!(archive_bytes(
        &db,
        root.path(),
        &ArchiveSource {
            source_key: "\0",
            ..source()
        },
        b"",
        &digest(b"")
    )
    .is_err());
    assert!(archive_bytes(
        &db,
        root.path(),
        &ArchiveSource {
            version_key: "",
            ..source()
        },
        b"",
        &digest(b"")
    )
    .is_err());
    assert_eq!(
        read_archive(&db, root.path(), &stored(&db, root.path(), b"").id).unwrap(),
        b""
    );
}

#[test]
fn learning_note_archive_v26_migration_does_not_rewrite_existing_note_documents() {
    let (root, mut db) = fixture();
    let document = serde_json::json!({"type":"doc","content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"既有 UE5 经验"}]}]});
    let saved = crate::knowledge::save(
        &mut db,
        crate::knowledge::NoteInput {
            id: None,
            expected_revision: None,
            notebook_id: None,
            title: "原有知识".into(),
            document: document.clone(),
            tags: vec!["UE5".into()],
            favorite: true,
            journal_date: None,
        },
    )
    .unwrap();
    db.execute_batch("DROP TABLE learning_note_archive_items; DROP TABLE learning_note_archives; UPDATE schema_meta SET value='26' WHERE key='schema_version';").unwrap();
    super::migrate(&db).unwrap();
    super::migrate(&db).unwrap();
    let after = crate::knowledge::get(&db, &saved.id).unwrap();
    assert_eq!(after.document, document);
    assert_eq!(after.revision, saved.revision);
    assert_eq!(after.updated_at, saved.updated_at);
    assert_eq!(stored(&db, root.path(), b"after migration").size, 15);
}

#[test]
fn learning_note_archive_migration_and_backup_restore_keep_bytes_and_registration() {
    let source_root = tempfile::tempdir().unwrap();
    let target_root = tempfile::tempdir().unwrap();
    let source_state = crate::state::test_state(&source_root.path().join("library"));
    let target_state = crate::state::test_state(&target_root.path().join("library"));
    let backup = source_root.path().join("notes.dccassetlib");
    let bytes = b"\xef\xbb\xbf# Source\r\n[[Unknown]]\r\n";
    let (archive_id, item_id) = source_state
        .with_library(|db, base| {
            db.execute(
                "UPDATE schema_meta SET value='26' WHERE key='schema_version'",
                [],
            )
            .unwrap();
            super::migrate(db)?;
            super::migrate(db)?;
            let version: String = db
                .query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(version, "29");
            let saved = stored(db, base, bytes);
            let item =
                add_archive_item(db, base, &saved.id, 3, bytes.len() as u64, "来源").unwrap();
            crate::backup::export_library(db, base, &backup)?;
            Ok((saved.id, item.id))
        })
        .unwrap();
    target_state
        .with_active_mut(|active| crate::backup::restore_library(active, &backup))
        .unwrap();
    target_state
        .with_library(|db, base| {
            assert_eq!(read_archive(db, base, &archive_id)?, bytes);
            assert_eq!(read_archive_item(db, base, &item_id)?, &bytes[3..]);
            assert_eq!(stored(db, base, bytes).id, archive_id);
            db.execute(
                "UPDATE learning_note_archive_items SET sha256=?1 WHERE id=?2",
                params![digest(b"corruption"), item_id],
            )
            .unwrap();
            assert!(read_archive_item(db, base, &item_id).is_err());
            Ok(())
        })
        .unwrap();
}

#[test]
#[cfg(windows)]
fn learning_note_archive_rejects_junction_without_writing_outside_library() {
    let (root, db) = fixture();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("learning-data")).unwrap();
    let link = root.path().join("learning-data").join("archives");
    let status = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .status()
        .unwrap();
    assert!(status.success());
    let result = archive_bytes(&db, root.path(), &source(), b"private", &digest(b"private"));
    fs::remove_dir(&link).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
