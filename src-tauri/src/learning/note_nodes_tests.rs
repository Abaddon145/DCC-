use super::note_archive::{self, ArchiveSource};
use crate::knowledge::{self, NoteInput};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn fixture() -> (tempfile::TempDir, Connection, String, Value) {
    let (root, db, source) = super::analysis_tests::stored_fixture();
    let bytes = b"---\r\nUE: 5\r\n---\r\n<script>notExecutable()</script>\r\n";
    let hash = format!("{:x}", Sha256::digest(bytes));
    let archive = note_archive::archive_bytes(
        &db,
        root.path(),
        &ArchiveSource {
            profile_id: "596d257d-3e5c-46c4-a464-187cf180c6bd",
            source_key: "vault/note.md",
            version_key: "current",
        },
        bytes,
        &hash,
    )
    .unwrap();
    let item = note_archive::add_archive_item(
        &db,
        root.path(),
        &archive.id,
        0,
        bytes.len() as u64,
        "原始块",
    )
    .unwrap();
    let media = super::store::course(&db, &source.preview.course_id)
        .unwrap()
        .media_id;
    let reference = json!({"type":"learningReference","attrs":{"courseId":source.preview.course_id,"mediaId":media,"fingerprint":"a".repeat(64),"ranges":[{"startMs":1000,"endMs":3000},{"startMs":5000,"endMs":6000}],"label":"材质课程摘录"}});
    (root, db, item.id, reference)
}
fn input(document: Value) -> NoteInput {
    NoteInput {
        id: None,
        expected_revision: None,
        notebook_id: None,
        title: "学习来源".into(),
        document,
        tags: vec![],
        favorite: false,
        journal_date: None,
    }
}
fn document(item: &str, reference: Value) -> Value {
    json!({"type":"doc","content":[reference,{"type":"preservedMarkdown","attrs":{"archiveItemId":item,"label":"保留原始属性"}}]})
}

#[test]
fn learning_note_nodes_save_search_media_backlink_and_immutable_block_survive_source_note_deletion()
{
    let (root, mut db, item, reference) = fixture();
    let note = knowledge::save(&mut db, input(document(&item, reference.clone()))).unwrap();
    assert!(note.plain_text.contains("材质课程摘录"));
    assert!(note.plain_text.contains("保留原始属性"));
    assert_eq!(
        knowledge::search(&db, "材质课程", None, false, false, 0, 20)
            .unwrap()
            .total,
        1
    );
    let refs = knowledge::references(&db, &note.id).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].kind, "media");
    assert_eq!(
        refs[0].target_id,
        reference["attrs"]["mediaId"].as_str().unwrap()
    );
    let deleted = knowledge::set_deleted(&mut db, &note.id, note.revision, true).unwrap();
    assert_eq!(deleted.document, note.document);
    assert_eq!(
        knowledge::search(&db, "材质课程", None, false, false, 0, 20)
            .unwrap()
            .total,
        0
    );
    assert!(note_archive::read_archive_item(&db, root.path(), &item).is_ok());
    let restored = knowledge::set_deleted(&mut db, &deleted.id, deleted.revision, false).unwrap();
    assert_eq!(restored.document, note.document);
    assert_eq!(
        knowledge::search(&db, "材质课程", None, false, false, 0, 20)
            .unwrap()
            .total,
        1
    );
    let note = restored;
    let target = knowledge::save(&mut db, input(json!({"type":"doc","content":[]}))).unwrap();
    let copied = crate::note_clipboard::transfer(
        &mut db,
        root.path(),
        &note.id,
        &target.id,
        json!({"content":note.document["content"],"openStart":0,"openEnd":0}),
    )
    .unwrap();
    let mut edited = input(json!({"type":"doc","content":copied["content"]}));
    edited.id = Some(target.id.clone());
    edited.expected_revision = Some(target.revision);
    let target = knowledge::save(&mut db, edited).unwrap();
    db.execute("DELETE FROM knowledge_notes WHERE id=?1", [note.id])
        .unwrap();
    assert_eq!(
        knowledge::get(&db, &target.id).unwrap().document["content"][1]["attrs"]["archiveItemId"],
        item
    );
    assert!(note_archive::read_archive_item(&db, root.path(), &item)
        .unwrap()
        .starts_with(b"---\r\n"));
}

#[test]
fn learning_note_nodes_reject_unregistered_archives_without_updating_existing_note() {
    let (_root, mut db, item, reference) = fixture();
    let mut doc = document(&item, reference);
    let note = knowledge::save(&mut db, input(doc.clone())).unwrap();
    doc["content"][1]["attrs"]["archiveItemId"] = json!(uuid::Uuid::new_v4().to_string());
    let mut value = input(doc);
    value.id = Some(note.id.clone());
    value.expected_revision = Some(note.revision);
    assert!(knowledge::save(&mut db, value).is_err());
    let after = knowledge::get(&db, &note.id).unwrap();
    assert_eq!(after.revision, note.revision);
    assert_eq!(after.document, note.document);
    assert_eq!(knowledge::revisions(&db, &note.id).unwrap().len(), 0);
}

#[test]
fn learning_note_nodes_validate_identifiers_fingerprint_ranges_and_atom_shape() {
    let (_root, _db, item, reference) = fixture();
    assert!(knowledge::inspect_document(&document(&item, reference.clone())).is_ok());
    for (field, value) in [
        ("courseId", json!("../../bad")),
        ("mediaId", json!("bad")),
        ("fingerprint", json!("G".repeat(64))),
        ("ranges", json!([])),
        ("ranges", json!([{"startMs":1.5,"endMs":3}])),
        ("ranges", json!([{"startMs":4,"endMs":3}])),
        ("ranges", json!([{"startMs":0,"endMs":604800001}])),
        (
            "ranges",
            json!([{"startMs":100,"endMs":300},{"startMs":200,"endMs":500}]),
        ),
        (
            "ranges",
            json!([{"startMs":0,"endMs":2,"path":"not-a-capability"}]),
        ),
        ("label", json!("")),
        ("label", json!("\0")),
        ("html", json!("<img src=x onerror=alert(1)>")),
    ] {
        let mut bad = reference.clone();
        bad["attrs"][field] = value;
        assert!(
            knowledge::inspect_document(&document(&item, bad)).is_err(),
            "accepted {field}"
        );
    }
    let mut too_many = reference.clone();
    too_many["attrs"]["ranges"] = json!((0..257)
        .map(|n| json!({"startMs":n*2,"endMs":n*2+1}))
        .collect::<Vec<_>>());
    assert!(knowledge::inspect_document(&document(&item, too_many)).is_err());
    let mut non_atom = reference.clone();
    non_atom["content"] = json!([{"type":"text","text":"concealed"}]);
    assert!(knowledge::inspect_document(&document(&item, non_atom)).is_err());
    let mut bad_preserved = document(&item, reference.clone());
    bad_preserved["content"][1]["attrs"]["text"] = json!("forged raw");
    assert!(knowledge::inspect_document(&bad_preserved).is_err());
    // Existing topic attributes are retained and validated by the normal document boundary.
    let mut good = reference;
    good["attrs"]["topicDepth"] = json!(0);
    good["attrs"]["topicId"] = json!(uuid::Uuid::new_v4().to_string());
    assert!(knowledge::inspect_document(&document(&item, good)).is_ok());
}

#[test]
fn learning_note_nodes_portable_package_remaps_raw_archives_across_libraries_and_duplicates() {
    let (root, mut db, item, reference) = fixture();
    let note = knowledge::save(&mut db, input(document(&item, reference))).unwrap();
    let destination = root.path().join("learning.dccnotes");
    crate::note_package::export(&db, root.path(), &note.id, &destination).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&destination).unwrap()).unwrap();
    let manifest: Value = serde_json::from_reader(zip.by_name("note.json").unwrap()).unwrap();
    assert_eq!(manifest["version"], 2);
    assert_eq!(manifest["archives"].as_array().unwrap().len(), 1);
    drop(zip);
    let target = tempfile::tempdir().unwrap();
    let mut target_db = crate::db::open_database(&target.path().join("library.db")).unwrap();
    let imported = crate::note_package::import(&mut target_db, target.path(), &destination)
        .unwrap()
        .note;
    let imported_item = imported.document["content"][1]["attrs"]["archiveItemId"]
        .as_str()
        .unwrap();
    assert_ne!(imported_item, item);
    assert_eq!(
        note_archive::read_archive_item(&target_db, target.path(), imported_item).unwrap(),
        note_archive::read_archive_item(&db, root.path(), &item).unwrap()
    );
    let duplicated =
        crate::note_package::duplicate(&mut db, root.path(), &note.id, note.revision).unwrap();
    assert!(note_archive::read_archive_item(
        &db,
        root.path(),
        duplicated.document["content"][1]["attrs"]["archiveItemId"]
            .as_str()
            .unwrap()
    )
    .is_ok());
}

#[test]
fn learning_note_nodes_package_includes_only_referenced_raw_segments() {
    use std::io::Read;
    let (root, mut db, _item, _reference) = fixture();
    let bytes = b"PRIVATE\nPUBLIC\nSECRET\n";
    let hash = format!("{:x}", Sha256::digest(bytes));
    let original = note_archive::archive_bytes(
        &db,
        root.path(),
        &ArchiveSource {
            profile_id: "596d257d-3e5c-46c4-a464-187cf180c6bd",
            source_key: "private-source.md",
            version_key: "current",
        },
        bytes,
        &hash,
    )
    .unwrap();
    let item =
        note_archive::add_archive_item(&db, root.path(), &original.id, 8, 15, "选中段落").unwrap();
    let note=knowledge::save(&mut db,input(json!({"type":"doc","content":[{"type":"preservedMarkdown","attrs":{"archiveItemId":item.id,"label":"选中段落"}}]}))).unwrap();
    let path = root.path().join("public.dccnotes");
    crate::note_package::export(&db, root.path(), &note.id, &path).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut raw = String::new();
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).unwrap();
        if file.name().starts_with("archives/") {
            file.read_to_string(&mut raw).unwrap();
        }
    }
    assert_eq!(
        raw, "PUBLIC\n",
        "copying a raw block must not export unrelated parts of its parent source"
    );
    assert_eq!(
        note_archive::read_archive(&db, root.path(), &original.id).unwrap(),
        bytes
    );
}

#[test]
fn learning_note_nodes_corrupt_portable_archives_are_rejected_before_note_or_archive_publication() {
    use std::io::{Read, Write};
    let (root, mut db, item, reference) = fixture();
    let note = knowledge::save(&mut db, input(document(&item, reference))).unwrap();
    let original = root.path().join("original.dccnotes");
    crate::note_package::export(&db, root.path(), &note.id, &original).unwrap();
    for case in [
        "raw_hash",
        "slice_hash",
        "range",
        "missing_reference",
        "unsafe_id",
    ] {
        let mut source = zip::ZipArchive::new(std::fs::File::open(&original).unwrap()).unwrap();
        let mut manifest: Value =
            serde_json::from_reader(source.by_name("note.json").unwrap()).unwrap();
        let mut raw = Vec::new();
        let path = format!(
            "archives/{}.bin",
            manifest["archives"][0]["id"].as_str().unwrap()
        );
        source
            .by_name(&path)
            .unwrap()
            .read_to_end(&mut raw)
            .unwrap();
        match case {
            "raw_hash" => raw.push(b'!'),
            "slice_hash" => manifest["archives"][0]["items"][0]["sha256"] = json!("b".repeat(64)),
            "range" => manifest["archives"][0]["items"][0]["endByte"] = json!(u64::MAX),
            "missing_reference" => {
                manifest["archives"][0]["items"][0]["id"] = json!(uuid::Uuid::new_v4().to_string())
            }
            "unsafe_id" => manifest["archives"][0]["id"] = json!("../../outside"),
            _ => unreachable!(),
        }
        let modified = root.path().join(format!("{case}.dccnotes"));
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&modified).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("note.json", options).unwrap();
        writer
            .write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        writer.start_file(path, options).unwrap();
        writer.write_all(&raw).unwrap();
        writer.finish().unwrap();
        let target = tempfile::tempdir().unwrap();
        let mut target_db = crate::db::open_database(&target.path().join("library.db")).unwrap();
        assert!(
            crate::note_package::import(&mut target_db, target.path(), &modified).is_err(),
            "accepted {case}"
        );
        assert_eq!(
            target_db
                .query_row("SELECT count(*) FROM knowledge_notes", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            target_db
                .query_row("SELECT count(*) FROM learning_note_archives", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn learning_note_nodes_markdown_export_does_not_reinterpret_markers_inside_preserved_bytes() {
    let (root, mut db, item, _reference) = fixture();
    let raw = format!("Literal [保留原始属性](dccpreserved://{item})\r\n");
    let hash = format!("{:x}", Sha256::digest(raw.as_bytes()));
    let archive = note_archive::archive_bytes(
        &db,
        root.path(),
        &ArchiveSource {
            profile_id: "596d257d-3e5c-46c4-a464-187cf180c6bd",
            source_key: "literal.md",
            version_key: "current",
        },
        raw.as_bytes(),
        &hash,
    )
    .unwrap();
    let literal = note_archive::add_archive_item(
        &db,
        root.path(),
        &archive.id,
        0,
        raw.len() as u64,
        "literal",
    )
    .unwrap();
    let note=knowledge::save(&mut db,input(json!({"type":"doc","content":[{"type":"preservedMarkdown","attrs":{"archiveItemId":literal.id,"label":"literal"}},{"type":"preservedMarkdown","attrs":{"archiveItemId":item,"label":"保留原始属性"}}]}))).unwrap();
    let path = root.path().join("literal.md");
    crate::note_markdown::export(
        &db,
        root.path(),
        &note.id,
        note.revision,
        &format!(
            "[literal](dccpreserved://{})\n\n[保留原始属性](dccpreserved://{item})",
            literal.id
        ),
        &path,
    )
    .unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.starts_with(&raw));
    assert_eq!(text.matches(&format!("dccpreserved://{item}")).count(), 1);
    assert!(text.contains("<script>notExecutable()</script>"));
}

#[test]
fn learning_note_nodes_exports_raw_markdown_plain_text_and_inert_html_without_silent_omission() {
    let (root, mut db, item, reference) = fixture();
    let note = knowledge::save(&mut db, input(document(&item, reference))).unwrap();
    let html = crate::note_export::html(&db, root.path(), &note.id, note.revision).unwrap();
    assert!(html.contains("&lt;script&gt;notExecutable()&lt;/script&gt;"));
    assert!(!html.contains("<script>notExecutable()"));
    let markdown = root.path().join("explicit.md");
    crate::note_markdown::export(
        &db,
        root.path(),
        &note.id,
        note.revision,
        &format!("[保留原始属性](dccpreserved://{item})"),
        &markdown,
    )
    .unwrap();
    let exported = std::fs::read(&markdown).unwrap();
    assert!(String::from_utf8(exported)
        .unwrap()
        .contains("---\r\nUE: 5\r\n---\r\n<script>notExecutable()</script>\r\n"));
    let txt = root.path().join("explicit.txt");
    crate::note_export::export(&db, root.path(), &note.id, note.revision, "txt", &txt).unwrap();
    assert!(std::fs::read_to_string(txt)
        .unwrap()
        .contains("<script>notExecutable()</script>"));
    db.execute(
        "UPDATE learning_note_archive_items SET sha256=?1 WHERE id=?2",
        rusqlite::params!["b".repeat(64), item],
    )
    .unwrap();
    assert!(crate::note_export::html(&db, root.path(), &note.id, note.revision).is_err());
    assert!(crate::note_package::export(
        &db,
        root.path(),
        &note.id,
        &root.path().join("broken.dccnotes")
    )
    .is_err());
    assert!(!root.path().join("broken.dccnotes").exists());
}

#[test]
fn learning_note_nodes_raw_reader_requires_note_ownership_but_preserves_saved_history() {
    let (root, mut db, item, reference) = fixture();
    let note = knowledge::save(&mut db, input(document(&item, reference))).unwrap();
    let other = knowledge::save(&mut db, input(json!({"type":"doc","content":[]}))).unwrap();
    assert!(super::note_nodes::read_preserved(&db, root.path(), &other.id, &item).is_err());
    assert!(
        super::note_nodes::read_preserved(&db, root.path(), &note.id, &item)
            .unwrap()
            .contains("<script>")
    );
    let mut edited = input(json!({"type":"doc","content":[]}));
    edited.id = Some(note.id.clone());
    edited.expected_revision = Some(note.revision);
    let saved = knowledge::save(&mut db, edited).unwrap();
    assert!(
        super::note_nodes::read_preserved(&db, root.path(), &saved.id, &item)
            .unwrap()
            .contains("<script>")
    );
}

#[test]
fn learning_note_nodes_jump_binds_library_course_media_duration_and_full_original_sha() {
    let (root, db, _item, node) = fixture();
    let bytes = b"actual registered synthetic video bytes";
    std::fs::write(root.path().join("fixture.mp4"), bytes).unwrap();
    let mut reference: super::note_nodes::LearningReference =
        serde_json::from_value(node["attrs"].clone()).unwrap();
    reference.fingerprint = format!("{:x}", Sha256::digest(bytes));
    drop(db);
    let state = crate::state::test_state(root.path());
    let library = state.active_library_id().unwrap();
    let jump = super::note_nodes::prepare_jump(&state, &library, &reference, 1).unwrap();
    assert_eq!(jump.position_ms, 5000);
    assert_eq!(jump.end_ms, 6000);
    assert_eq!(jump.course_id, reference.course_id);
    assert_eq!(jump.media_id, reference.media_id);
    assert!(super::note_nodes::prepare_jump(&state, "another-library", &reference, 1).is_err());
    assert!(super::note_nodes::prepare_jump(&state, &library, &reference, 2).is_err());
    let mut wrong = reference.clone();
    wrong.media_id = uuid::Uuid::new_v4().to_string();
    assert!(super::note_nodes::prepare_jump(&state, &library, &wrong, 0).is_err());
    let mut wrong = reference.clone();
    wrong.ranges[1].end_ms = 60001;
    assert!(super::note_nodes::prepare_jump(&state, &library, &wrong, 0).is_err());
    std::fs::write(root.path().join("fixture.mp4"), b"different source bytes").unwrap();
    assert!(super::note_nodes::prepare_jump(&state, &library, &reference, 0).is_err());
}
