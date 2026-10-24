use super::{models::*, repository};
use rusqlite::{params, Connection};

const OLD_ID: &str = "06003f78-9a59-4fb8-abbc-14dc276f0b4a";

#[test]
fn review_scan_hash_survives_source_replacement_during_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("library");
    let root = dir.path().join("source");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("a.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
        .save(&source)
        .unwrap();
    let task = uuid::Uuid::new_v4().to_string();
    let manifest = super::scanner::manifest_path(&base, &task).unwrap();
    super::scanner::scan(&root, &base, &manifest, &|_| {}, &|| false).unwrap();
    let page = super::scanner::read_page(&manifest, 0, 1).unwrap();
    let req: CoreImportRequest = serde_json::from_value(
        serde_json::json!({"scanTaskId":task,"scanRowIds":[page.items[0].id]}),
    )
    .unwrap();
    let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
    let calls = std::cell::Cell::new(0);
    let result = super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| {
        calls.set(calls.get() + 1);
        // Sixth cancellation boundary is after outer hash and image dimensions.
        if calls.get() == 6 {
            image::RgbaImage::from_pixel(7, 5, image::Rgba([9, 8, 7, 255]))
                .save(&source)
                .unwrap();
        }
        false
    })
    .unwrap();
    assert!(calls.get() >= 6);
    assert_eq!((result.imported, result.failed), (0, 1));
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn review_metadata_failure_retry_repairs_existing_managed_original() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("library");
    std::fs::create_dir_all(&base).unwrap();
    let source = dir.path().join("a.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
        .save(&source)
        .unwrap();
    let req: CoreImportRequest =
        serde_json::from_value(serde_json::json!({"paths":[source]})).unwrap();
    let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_metadata BEFORE INSERT ON asset_metadata BEGIN SELECT RAISE(ABORT,'injected metadata failure'); END;").unwrap();
    let first =
        super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| false).unwrap();
    assert_eq!(first.failed, 1);
    let before: (String, String) = db
        .query_row(
            "SELECT id,rel_path FROM media_files WHERE role='main'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    db.execute_batch("DROP TRIGGER reject_metadata;").unwrap();
    let retry =
        super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| false).unwrap();
    assert_eq!((retry.skipped, retry.failed), (1, 0));
    assert_eq!(
        db.query_row(
            "SELECT id,rel_path FROM media_files WHERE role='main'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        )
        .unwrap(),
        before
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_metadata", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn review_import_rechecks_source_library_boundary_after_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("library");
    let root = dir.path().join("source");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("a.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
        .save(&source)
        .unwrap();
    let req: CoreImportRequest =
        serde_json::from_value(serde_json::json!({"paths":[source]})).unwrap();
    let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
    let calls = std::cell::Cell::new(0);
    let result = super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| {
        calls.set(calls.get() + 1);
        if calls.get() == 6 {
            std::fs::write(root.join("library.json"), "{}").unwrap();
            std::fs::write(root.join("library.db"), "").unwrap();
        }
        false
    })
    .unwrap();
    assert!(calls.get() >= 6);
    assert_eq!((result.imported, result.failed), (0, 1));
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn review_scans_and_direct_imports_reject_other_library_descendants() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("current");
    let other = dir.path().join("other");
    let child = other.join("media-library/image/entry/files");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(other.join("library.json"), "{}").unwrap();
    std::fs::write(other.join("library.db"), "").unwrap();
    let source = child.join("original.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
        .save(&source)
        .unwrap();
    assert!(super::scanner::validate_source(&source, &base).is_err());
    for root in [&other, &other.join("media-library"), &child] {
        assert!(
            super::scanner::scan(root, &base, &base.join("scan.jsonl"), &|_| {}, &|| false)
                .is_err()
        );
    }
    let summary = super::scanner::scan(
        dir.path(),
        &base,
        &base.join("scan.jsonl"),
        &|_| {},
        &|| false,
    )
    .unwrap();
    assert_eq!(summary.total, 0);
}

#[test]
fn review_legacy_video_preview_uses_proxy_without_changing_version_original() {
    let dir = tempfile::tempdir().unwrap();
    let db = old_fixture();
    std::fs::write(dir.path().join("original.mov"), "original").unwrap();
    std::fs::write(dir.path().join("proxy.webm"), "proxy").unwrap();
    db.execute("INSERT INTO asset_media(id,asset_id,kind,original_name,original_rel_path,proxy_rel_path,mime_type,created_at,updated_at) VALUES('clip',?1,'video','original.mov','original.mov','proxy.webm','video/quicktime','old','old')", [OLD_ID]).unwrap();
    super::migrate(&db).unwrap();
    let core = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "legacy".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    let detail = repository::get(&db, dir.path(), &core, "zh-CN").unwrap();
    let original = detail
        .files
        .iter()
        .find(|f| f.source_ref.id == "clip" && f.role == "original")
        .unwrap();
    assert_eq!(original.source_ref.role, "original");
    assert_eq!(original.preview.as_ref().unwrap().role, "proxy");
    assert_eq!(
        repository::file_path(&db, dir.path(), original.preview.as_ref().unwrap()).unwrap(),
        dir.path().join("proxy.webm").canonicalize().unwrap()
    );
    std::fs::remove_file(dir.path().join("proxy.webm")).unwrap();
    let fallback = repository::get(&db, dir.path(), &core, "zh-CN").unwrap();
    assert_eq!(
        fallback
            .files
            .iter()
            .find(|f| f.id == original.id)
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .role,
        "original"
    );
}

fn old_fixture() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    db.execute_batch(include_str!("../schema.sql")).unwrap();
    db.execute_batch(include_str!("../notes_schema.sql"))
        .unwrap();
    db.execute(
        "INSERT INTO schema_meta(key,value) VALUES('schema_version','20')",
        [],
    )
    .unwrap();
    db.execute("INSERT INTO assets(id,name,description,share_url,created_at,updated_at) VALUES(?1,'旧资产','原说明','','old','old')", [OLD_ID]).unwrap();
    db.execute("INSERT INTO asset_localizations(asset_id,locale,name,description) VALUES(?1,'en','Old asset','Original description')", [OLD_ID]).unwrap();
    db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,'image','旧图片','09003f78-9a59-4fb8-abbc-14dc276f0b4a','old','old')", [OLD_ID]).unwrap();
    db.execute("INSERT INTO media_files(id,entry_id,role,logical_path,original_name,rel_path,file_size,checksum,created_at) VALUES('09003f78-9a59-4fb8-abbc-14dc276f0b4a',?1,'main','原图.png','原图.png','media-library/image/旧图/原图.png',123,'known-hash','old')", [OLD_ID]).unwrap();
    db.execute("INSERT INTO projects(id,name,project_type,status,created_at,updated_at) VALUES('project','旧项目','scene','active','old','old')",[]).unwrap();
    db.execute("INSERT INTO project_assets(project_id,asset_id,status,created_at,updated_at) VALUES('project',?1,'candidate','old','old')",[OLD_ID]).unwrap();
    db.execute("INSERT INTO knowledge_notes(id,title,document_json,plain_text,created_at,updated_at) VALUES('note','学习笔记','{}','原正文','old','old')",[]).unwrap();
    db.execute("INSERT INTO knowledge_references(note_id,target_kind,target_id,label) VALUES('note','asset',?1,'旧引用')",[OLD_ID]).unwrap();
    db
}

#[test]
fn database_open_installs_compatible_asset_registry() {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    let tables: i64 = db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('asset_core','asset_files','asset_metadata','asset_sources','asset_versions')", [], |r| r.get(0)).unwrap();
    assert_eq!(tables, 5, "all compatible core tables must exist");
    let version: String = db
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, "27");
}

#[test]
fn migration_is_idempotent_and_preserves_both_origins_and_paths() {
    let db = old_fixture();
    super::migrate(&db).unwrap();
    let legacy = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "legacy".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    let media = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "media".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    assert_ne!(legacy, media);
    let count: i64 = db
        .query_row("SELECT count(*) FROM asset_files", [], |r| r.get(0))
        .unwrap();
    super::migrate(&db).unwrap();
    assert_eq!(
        repository::register_origin(
            &db,
            &CoreOrigin {
                kind: "legacy".into(),
                id: OLD_ID.into()
            }
        )
        .unwrap(),
        legacy
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_files", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        count
    );
    assert_eq!(
        db.query_row("SELECT rel_path FROM media_files", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "media-library/image/旧图/原图.png"
    );
    db.execute("UPDATE assets SET name='即时修改' WHERE id=?1", [OLD_ID])
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        repository::get(&db, dir.path(), &legacy, "zh-CN")
            .unwrap()
            .asset
            .name,
        "即时修改"
    );
    assert_eq!(
        repository::get(&db, dir.path(), &legacy, "en")
            .unwrap()
            .asset
            .name,
        "Old asset"
    );
    let file = &repository::get(&db, dir.path(), &media, "zh-CN")
        .unwrap()
        .files[0];
    assert_eq!(file.size, 123);
    assert_eq!(file.hash, "known-hash");
    assert!(!file.available);
    assert_eq!(
        db.query_row("SELECT asset_id FROM project_assets", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        OLD_ID
    );
    assert_eq!(
        db.query_row("SELECT target_id FROM knowledge_references", [], |r| r
            .get::<_, String>(
            0
        ))
        .unwrap(),
        OLD_ID
    );
}

#[test]
fn core_constraints_and_migration_failure_roll_back() {
    let db = old_fixture();
    super::migrate(&db).unwrap();
    assert!(db
        .execute(
            "INSERT INTO asset_core(id,created_at) VALUES('bad','now')",
            []
        )
        .is_err());
    assert!(db.execute("INSERT INTO asset_core(id,legacy_asset_id,media_entry_id,created_at) VALUES('bad',?1,?1,'now')",[OLD_ID]).is_err());
    assert!(repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "legacy".into(),
            id: "missing".into()
        }
    )
    .is_err());
    let broken = old_fixture();
    broken.execute_batch("DROP TABLE media_files;").unwrap();
    assert!(super::migrate(&broken).is_err());
    assert_eq!(
        broken
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='asset_core'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        broken
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "20"
    );
}

fn request() -> CoreSearchRequest {
    serde_json::from_value(serde_json::json!({})).unwrap()
}

#[test]
fn mixed_search_is_bounded_and_reads_live_names_and_soft_deletion() {
    let db = old_fixture();
    super::migrate(&db).unwrap();
    super::intelligence_migration::migrate(&db).unwrap();
    let mut query = request();
    query.limit = 1;
    let page = repository::search(&db, &query).unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 1);
    query.offset = 1;
    assert_ne!(
        repository::search(&db, &query).unwrap().items[0].id,
        page.items[0].id
    );
    query.asset_type = Some("image".into());
    query.offset = 0;
    assert_eq!(repository::search(&db, &query).unwrap().total, 1);
    db.execute("UPDATE media_entries SET deleted_at='now'", [])
        .unwrap();
    assert_eq!(repository::search(&db, &query).unwrap().total, 0);
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_core", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    query.asset_type = Some("unexpected".into());
    assert!(repository::search(&db, &query).is_err());
    query.asset_type = None;
    query.source = Some("unexpected".into());
    assert!(repository::search(&db, &query).is_err());
}

#[test]
fn media_import_registers_files_in_the_same_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    let source = dir.path().join("导入.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
        .save(&source)
        .unwrap();
    let input = crate::models::MediaImportRequest {
        kind: crate::models::MediaKind::Image,
        paths: vec![source.to_string_lossy().into_owned()],
        folder_id: None,
        tags: vec![],
        favorite: false,
    };
    let report = crate::media_library::import(&mut db, dir.path(), None, input).unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_core", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        db.query_row("SELECT count(*) FROM asset_files", [], |r| r
            .get::<_, i64>(0))
            .unwrap()
            >= 2
    );
}

#[test]
fn version_file_must_belong_to_core_and_be_original_and_is_protected() {
    let db = old_fixture();
    super::migrate(&db).unwrap();
    let ids: Vec<String> = db
        .prepare("SELECT id FROM asset_core ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let (file, owner): (String, String) = db
        .query_row(
            "SELECT id,asset_id FROM asset_files WHERE role='original'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let other = ids.iter().find(|id| **id != owner).unwrap();
    assert!(db.execute("INSERT INTO asset_versions(id,asset_id,file_id,version,created_at) VALUES('v',?1,?2,'v001','now')",params![other,file]).is_err());
    db.execute("INSERT INTO asset_versions(id,asset_id,file_id,version,created_at) VALUES('v',?1,?2,'v001','now')",params![owner,file]).unwrap();
    let source = SourceFileRef {
        kind: "mediaFile".into(),
        id: "09003f78-9a59-4fb8-abbc-14dc276f0b4a".into(),
        role: "original".into(),
    };
    assert!(repository::protect_file_removal(&db, &[source.clone()], &[]).is_err());
    assert!(repository::protect_file_removal(&db, &[source], &[owner]).is_ok());
    assert!(db.execute("DELETE FROM media_files", []).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_versions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn purge_blocks_external_version_and_allows_both_owners_in_same_batch() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = old_fixture();
    super::migrate(&db).unwrap();
    crate::learning::migrate(&db).unwrap();
    let legacy = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "legacy".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    db.execute("INSERT INTO asset_files(id,asset_id,source_kind,source_id,role,filename,extension,size,hash,created_at) SELECT 'attached',?1,source_kind,source_id,role,filename,extension,size,hash,created_at FROM asset_files WHERE role='original'",[&legacy]).unwrap();
    db.execute("INSERT INTO asset_versions(id,asset_id,file_id,version,created_at) VALUES('v',?1,'attached','Final','now')",[legacy]).unwrap();
    db.execute("INSERT INTO deletion_batches(id,kind,label,created_at) VALUES('batch','media','旧图','now')",[]).unwrap();
    db.execute(
        "UPDATE media_entries SET deleted_at='now',delete_batch_id='batch'",
        [],
    )
    .unwrap();
    let error = crate::db::purge_trash_batch(&mut db, dir.path(), "batch").unwrap_err();
    assert!(error.contains("版本"), "{error}");
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM deletion_batches", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    db.execute(
        "UPDATE assets SET deleted_at='now',delete_batch_id='batch'",
        [],
    )
    .unwrap();
    crate::db::purge_trash_batch(&mut db, dir.path(), "batch").unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_core", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_versions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn registration_failure_rolls_back_real_import_and_keeps_original() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_core BEFORE INSERT ON asset_core BEGIN SELECT RAISE(ABORT,'injected core registration failure'); END;").unwrap();
    let source = dir.path().join("original.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]))
        .save(&source)
        .unwrap();
    let before = std::fs::read(&source).unwrap();
    let input = crate::models::MediaImportRequest {
        kind: crate::models::MediaKind::Image,
        paths: vec![source.to_string_lossy().into_owned()],
        folder_id: None,
        tags: vec![],
        favorite: false,
    };
    let result = crate::media_library::import(&mut db, dir.path(), None, input).unwrap();
    assert_eq!(result.imported, 0);
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_files", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(std::fs::read(source).unwrap(), before);
    let files = walkdir::WalkDir::new(dir.path().join("media-library"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .count();
    assert_eq!(files, 0);
}

#[test]
fn unregistered_file_and_unsafe_registered_path_are_rejected() {
    let db = old_fixture();
    super::migrate(&db).unwrap();
    let owner = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "media".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    assert!(db.execute("INSERT INTO asset_files(id,asset_id,source_kind,source_id,role,filename,extension,created_at) VALUES('bad',?1,'mediaFile','missing','original','file.png','png','now')",[owner]).is_err());
    db.execute("UPDATE media_files SET rel_path='../outside.png'", [])
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(repository::file_path(
        &db,
        dir.path(),
        &SourceFileRef {
            kind: "mediaFile".into(),
            id: "09003f78-9a59-4fb8-abbc-14dc276f0b4a".into(),
            role: "original".into()
        }
    )
    .is_err());
}

#[test]
fn old_asset_editor_cannot_remove_an_original_used_by_a_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    let mut input: crate::models::AssetInput = serde_json::from_value(serde_json::json!({
        "name":"旧素材", "description":"", "categoryId":null,"tags":[],"dccTools":[],
        "versions":[],"formats":[],"sizeBytes":null,"author":"","sourceUrl":"","license":"",
        "shareUrl":"","extractionCode":"","favorite":false,"images":[],"localizations":{},"contentLanguage":"zh-CN"
    })).unwrap();
    let saved = crate::db::upsert_asset(&mut db, dir.path(), input.clone()).unwrap();
    let owner = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "legacy".into(),
            id: saved.card.id.clone(),
        },
    )
    .unwrap();
    db.execute("INSERT INTO images(id,asset_id,original_name,original_rel_path,thumbnail_rel_path,created_at) VALUES('image',?1,'original.png','images/originals/original.png','images/thumbnails/original.webp','old')",[&saved.card.id]).unwrap();
    repository::register_files(&db, &owner).unwrap();
    db.execute("INSERT INTO asset_versions(id,asset_id,file_id,version,created_at) SELECT 'v',asset_id,id,'v001','now' FROM asset_files WHERE source_id='image' AND role='original'",[]).unwrap();
    input.id = Some(saved.card.id.clone());
    input.name = "不应保存的修改".into();
    let error = crate::db::upsert_asset(&mut db, dir.path(), input).unwrap_err();
    assert!(error.contains("版本"));
    assert_eq!(
        db.query_row("SELECT name FROM assets", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "旧素材"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM images", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn folder_scan_streams_supported_files_and_reports_duplicates_and_reserved_types() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("学习素材");
    let library = dir.path().join("library");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    for (name, bytes) in [
        ("a.png", b"image".as_slice()),
        ("b.png", b"image"),
        ("clip.mp4", b"video"),
        ("music.mp3", b"audio"),
        ("Mesh.uasset", b"unreal"),
        ("unknown.txt", b"text"),
    ] {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    std::fs::write(root.join(".git/ignored.png"), b"git").unwrap();
    let manifest = library.join("scan.jsonl");
    let summary = super::scanner::scan(&root, &library, &manifest, &|_| {}, &|| false).unwrap();
    assert_eq!(summary.total, 6);
    assert_eq!(summary.importable, 3);
    assert_eq!(summary.duplicate, 1);
    assert_eq!(summary.unsupported, 2);
    assert_eq!(summary.failed, 0);
}

#[test]
fn core_import_deduplicates_mixed_rows_and_preserves_source_without_semantic_tags() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library");
    std::fs::create_dir_all(&library).unwrap();
    let mut db = crate::db::open_database(&library.join("library.db")).unwrap();
    let source = dir.path().join("原件.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([4, 5, 6, 255]))
        .save(&source)
        .unwrap();
    let invalid = dir.path().join("损坏.png");
    std::fs::write(&invalid, b"broken").unwrap();
    let before = std::fs::read(&source).unwrap();
    let request: CoreImportRequest =
        serde_json::from_value(serde_json::json!({"paths":[source,source,invalid]})).unwrap();
    let result =
        super::service::import_files(&mut db, &library, None, &request, &|_, _| {}, &|| false)
            .unwrap();
    assert_eq!((result.imported, result.skipped, result.failed), (1, 1, 1));
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_core", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entry_tags", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM project_media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(std::fs::read(&source).unwrap(), before);
    db.execute("UPDATE media_entries SET deleted_at='now'", [])
        .unwrap();
    let result =
        super::service::import_files(&mut db, &library, None, &request, &|_, _| {}, &|| false)
            .unwrap();
    assert!(result.rows[0].message.contains("回收站"));
}

#[test]
fn scan_pages_cancel_safely_and_import_revalidates_changed_files_and_ids() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    let base = dir.path().join("library");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&base).unwrap();
    let source = root.join("a.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
        .save(&source)
        .unwrap();
    let task = uuid::Uuid::new_v4().to_string();
    let manifest = super::scanner::manifest_path(&base, &task).unwrap();
    super::scanner::scan(&root, &base, &manifest, &|_| {}, &|| false).unwrap();
    let page = super::scanner::read_page(&manifest, 0, 1).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items.len(), 1);
    assert_eq!(
        super::scanner::read_page(&manifest, 1, 1)
            .unwrap()
            .items
            .len(),
        0
    );
    image::RgbaImage::from_pixel(4, 4, image::Rgba([9, 8, 7, 255]))
        .save(&source)
        .unwrap();
    let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
    let mut req: CoreImportRequest = serde_json::from_value(
        serde_json::json!({"scanTaskId":task,"scanRowIds":[page.items[0].id]}),
    )
    .unwrap();
    let result =
        super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| false).unwrap();
    assert_eq!(result.failed, 1);
    assert!(result.rows[0].message.contains("发生变化"));
    req.scan_row_ids = vec![uuid::Uuid::new_v4().to_string()];
    assert!(
        super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| false).is_err()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(super::scanner::manifest_path(&base, "../secret").is_err());
    let stopped = super::scanner::scan(&root, &base, &manifest, &|_| {}, &|| true).unwrap();
    assert!(stopped.cancelled);
    assert_eq!(stopped.total, 0);
    assert_eq!(
        super::scanner::read_page(&manifest, 0, 20).unwrap().total,
        0
    );
}

#[test]
fn repeated_import_links_only_explicit_project_and_hashing_can_be_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("library");
    std::fs::create_dir_all(&base).unwrap();
    let mut db = crate::db::open_database(&base.join("library.db")).unwrap();
    db.execute("INSERT INTO projects(id,name,project_type,status,created_at,updated_at) VALUES('project','目标项目','scene','active','now','now')",[]).unwrap();
    let source = dir.path().join("a.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
        .save(&source)
        .unwrap();
    let mut req: CoreImportRequest =
        serde_json::from_value(serde_json::json!({"paths":[source]})).unwrap();
    super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| false).unwrap();
    req.project_id = Some("project".into());
    let result =
        super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| false).unwrap();
    assert_eq!(result.skipped, 1);
    assert_eq!(
        db.query_row("SELECT count(*) FROM project_media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    super::service::import_files(&mut db, &base, None, &req, &|_, _| {}, &|| false).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM project_media_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let file = dir.path().join("large.bin");
    std::fs::write(&file, vec![1_u8; 200_000]).unwrap();
    let checks = std::cell::Cell::new(0);
    assert!(crate::file_digest::digest_file_controlled(&file, &|| {
        checks.set(checks.get() + 1);
        checks.get() > 2
    })
    .is_err());
}

#[cfg(windows)]
#[test]
fn windows_junction_scan_does_not_follow_target() {
    use std::os::windows::process::CommandExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let target = dir.path().join("target");
    let library = dir.path().join("library");
    for path in [&root, &target, &library] {
        std::fs::create_dir_all(path).unwrap();
    }
    std::fs::write(target.join("hidden.png"), b"outside").unwrap();
    std::fs::write(root.join("visible.png"), b"inside").unwrap();
    let link = root.join("junction");
    let output = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(super::scanner::is_link(&link));
    let result = super::scanner::scan(
        &root,
        &library,
        &library.join("manifest.jsonl"),
        &|_| {},
        &|| false,
    )
    .unwrap();
    assert_eq!(result.total, 1);
    assert!(super::scanner::scan(
        &link,
        &library,
        &library.join("other.jsonl"),
        &|_| {},
        &|| false
    )
    .is_err());
    assert!(super::scanner::validate_source(&link.join("hidden.png"), &library).is_err());
}

#[test]
fn cancelled_atomic_copy_leaves_no_output_and_never_overwrites_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("large.bin");
    let target = dir.path().join("copy.bin");
    std::fs::write(&source, vec![42_u8; 200_000]).unwrap();
    let checks = std::cell::Cell::new(0);
    let control = crate::media::ProcessControl::new(move || {
        checks.set(checks.get() + 1);
        checks.get() > 1
    });
    assert!(crate::media::copy_atomic(&source, &target).is_err());
    assert!(!target.exists());
    drop(control);
    std::fs::write(&target, b"existing").unwrap();
    assert!(crate::media::copy_atomic(&source, &target).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"existing");
}

#[test]
fn image_metadata_and_thumbnail_are_managed_and_fail_safely() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("source.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([4, 5, 6, 255]))
        .save(&file)
        .unwrap();
    let data = super::metadata::analyze(&file, "image", None, &|| false).unwrap();
    assert_eq!((data.width, data.height), (Some(8), Some(6)));
    assert_eq!(data.duration, None);
    let id = uuid::Uuid::new_v4().to_string();
    let thumb =
        crate::media_engine::thumbnail::generate(dir.path(), None, &id, &file, "image", &|| false)
            .unwrap();
    assert!(thumb
        .relative_path
        .starts_with(&format!(".dcc/cache/thumbnails/{id}/")));
    assert!(dir.path().join(&thumb.relative_path).is_file());
    assert!(crate::media_engine::thumbnail::generate(
        dir.path(),
        None,
        "../bad",
        &file,
        "image",
        &|| false
    )
    .is_err());
    assert!(crate::media_engine::thumbnail::generate(
        dir.path(),
        None,
        &id,
        &file,
        "image",
        &|| true
    )
    .is_err());
    let original = std::fs::read(&file).unwrap();
    let invalid = dir.path().join("invalid.mp4");
    std::fs::write(&invalid, b"broken").unwrap();
    assert!(super::metadata::analyze(
        &invalid,
        "video",
        Some(&dir.path().join("missing")),
        &|| false
    )
    .is_err());
    assert_eq!(std::fs::read(&file).unwrap(), original);
    assert!(dir.path().join(thumb.relative_path).is_file());
}

#[test]
fn metadata_save_rejects_invalid_numbers_and_unknown_files() {
    let db = old_fixture();
    super::migrate(&db).unwrap();
    let id: String = db
        .query_row("SELECT id FROM asset_files LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let good = AssetMetadataValues {
        width: Some(8),
        height: Some(6),
        duration: Some(12.5),
        codec: Some("h264".into()),
        fps: Some(30000.0 / 1001.0),
    };
    super::metadata::save(&db, &id, &good).unwrap();
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        let mut bad = good.clone();
        bad.duration = Some(invalid);
        assert!(super::metadata::save(&db, &id, &bad).is_err());
        bad = good.clone();
        bad.fps = Some(invalid);
        assert!(super::metadata::save(&db, &id, &bad).is_err());
    }
    assert_eq!(
        db.query_row("SELECT duration FROM asset_metadata", [], |r| r
            .get::<_, f64>(0))
            .unwrap(),
        12.5
    );
    assert!(super::metadata::save(&db, "missing", &good).is_err());
}

#[test]
fn processing_reuses_old_thumbnail_and_replaces_only_committed_cache() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    let source = dir.path().join("fixture.png");
    image::RgbaImage::new(12, 9).save(&source).unwrap();
    let imported = crate::media_library::import(
        &mut db,
        dir.path(),
        None,
        crate::models::MediaImportRequest {
            kind: crate::models::MediaKind::Image,
            paths: vec![source.to_string_lossy().into_owned()],
            folder_id: None,
            tags: vec![],
            favorite: false,
        },
    )
    .unwrap();
    let entry = imported.rows[0].entry_id.as_ref().unwrap();
    let core = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "media".into(),
            id: entry.clone(),
        },
    )
    .unwrap();
    let old: String = db
        .query_row(
            "SELECT rel_path FROM media_files WHERE entry_id=?1 AND role='thumbnail'",
            [entry],
            |r| r.get(0),
        )
        .unwrap();
    super::metadata::process_asset(&db, dir.path(), None, &core, false, &|| false).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT rel_path FROM media_files WHERE entry_id=?1 AND role='thumbnail'",
            [entry],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        old
    );
    super::metadata::process_asset(&db, dir.path(), None, &core, true, &|| false).unwrap();
    let cache: String = db
        .query_row(
            "SELECT rel_path FROM media_files WHERE entry_id=?1 AND role='thumbnail'",
            [entry],
            |r| r.get(0),
        )
        .unwrap();
    assert!(cache.starts_with(".dcc/cache/thumbnails/"));
    super::metadata::process_asset(&db, dir.path(), None, &core, true, &|| false).unwrap();
    assert!(
        !dir.path().join(&cache).exists(),
        "superseded generated cache must not become orphaned"
    );
    let detail = repository::get(&db, dir.path(), &core, "zh-CN").unwrap();
    let original = detail.files.iter().find(|f| f.role == "original").unwrap();
    let original_path = repository::file_path(&db, dir.path(), &original.source_ref).unwrap();
    std::fs::write(&original_path, b"broken").unwrap();
    let report =
        super::metadata::process_asset(&db, dir.path(), None, &core, true, &|| false).unwrap();
    assert_eq!(report["rows"][0]["status"], "error");
    assert!(dir
        .path()
        .join(
            db.query_row(
                "SELECT rel_path FROM media_files WHERE entry_id=?1 AND role='thumbnail'",
                [entry],
                |r| r.get::<_, String>(0)
            )
            .unwrap()
        )
        .is_file());
    assert!(original_path.is_file());
}

#[test]
fn local_audio_video_metadata_and_short_clip_thumbnail_use_bundled_tools() {
    let dir = tempfile::tempdir().unwrap();
    let resource = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let ffmpeg = crate::runtime::tools::require(Some(&resource), "ffmpeg").unwrap();
    for (kind, filename, input, seconds) in [
        (
            "audio",
            "fixture.wav",
            "sine=frequency=440:sample_rate=48000",
            "12.5",
        ),
        (
            "video",
            "fixture.mp4",
            "color=c=blue:s=64x48:r=30000/1001",
            "0.2",
        ),
    ] {
        let path = dir.path().join(filename);
        let args = ["-y", "-f", "lavfi", "-i", input, "-t", seconds]
            .map(str::to_string)
            .into_iter()
            .chain(Some(path.to_string_lossy().into_owned()))
            .collect::<Vec<_>>();
        crate::runtime::process::capture(
            &ffmpeg,
            &args,
            std::time::Duration::from_secs(30),
            &|| false,
            &mut |_| {},
        )
        .unwrap();
        let data = super::metadata::analyze(&path, kind, Some(&resource), &|| false).unwrap();
        if kind == "audio" {
            assert_eq!(data.duration, Some(12.5));
            assert_eq!((data.width, data.height, data.fps), (None, None, None));
        } else {
            assert_eq!((data.width, data.height), (Some(64), Some(48)));
            assert!((data.fps.unwrap() - 30000.0 / 1001.0).abs() < 0.001);
        }
        let thumb = crate::media_engine::thumbnail::generate(
            dir.path(),
            Some(&resource),
            &uuid::Uuid::new_v4().to_string(),
            &path,
            kind,
            &|| false,
        )
        .unwrap();
        assert!(dir.path().join(thumb.relative_path).is_file());
        std::fs::write(&path, b"invalid media").unwrap();
        assert!(super::metadata::analyze(&path, kind, Some(&resource), &|| false).is_err());
    }
    let outside = tempfile::NamedTempFile::new().unwrap();
    assert!(crate::media_engine::thumbnail::generate(
        dir.path(),
        None,
        &uuid::Uuid::new_v4().to_string(),
        outside.path(),
        "image",
        &|| false
    )
    .is_err());
}

#[test]
fn shared_tags_keep_ids_languages_colors_and_media_only_usage() {
    let mut db = old_fixture();
    super::migrate(&db).unwrap();
    super::intelligence_migration::migrate(&db).unwrap();
    let a = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "legacy".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    let m = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "media".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    super::service::set_tags(&mut db, &a, "zh-CN", &["森林".into()]).unwrap();
    super::service::set_tags(&mut db, &m, "zh-CN", &["森林".into()]).unwrap();
    let id: String = db
        .query_row(
            "SELECT id FROM localized_tags WHERE name='森林'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        repository::search(
            &db,
            &CoreSearchRequest {
                query: "森林".into(),
                asset_type: None,
                source: None,
                project_id: None,
                locale: "zh-CN".into(),
                offset: 0,
                limit: 80
            }
        )
        .unwrap()
        .total,
        2
    );
    super::service::set_tags(&mut db, &a, "en", &["Forest".into()]).unwrap();
    super::service::set_tags(&mut db, &a, "zh-CN", &[]).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM asset_localized_tags WHERE asset_id=?1",
            [OLD_ID],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        crate::organization::delete_unused_tags(&db, "zh-CN")
            .unwrap()
            .removed_tags,
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT id FROM localized_tags WHERE name='森林'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        id
    );
    super::service::set_tag_color(&db, &id, "#AABBCC").unwrap();
    assert!(super::service::set_tag_color(&db, &id, "red; background:white").is_err());
    assert_eq!(
        db.query_row("SELECT color FROM localized_tags WHERE id=?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "#AABBCC"
    );
    let foreign = Connection::open_in_memory().unwrap();
    foreign
        .execute_batch(include_str!("../schema.sql"))
        .unwrap();
    super::migrate(&foreign).unwrap();
    assert!(super::service::set_tag_color(&foreign, "missing", "#AABBCC").is_err());
}

#[test]
fn core_project_links_reuse_both_origins_and_reject_archived_or_foreign_assets() {
    let mut db = old_fixture();
    super::migrate(&db).unwrap();
    db.execute("INSERT INTO projects(id,name,project_type,status,created_at,updated_at) VALUES('second','第二项目','scene','active','now','now')",[]).unwrap();
    for kind in ["legacy", "media"] {
        let core = repository::register_origin(
            &db,
            &CoreOrigin {
                kind: kind.into(),
                id: OLD_ID.into(),
            },
        )
        .unwrap();
        for _ in 0..2 {
            super::service::link_project(&mut db, &core, "project", true).unwrap();
            super::service::link_project(&mut db, &core, "second", true).unwrap();
        }
        assert_eq!(
            repository::get(&db, std::path::Path::new("."), &core, "zh-CN")
                .unwrap()
                .projects
                .len(),
            2
        );
        super::service::link_project(&mut db, &core, "second", false).unwrap();
        assert_eq!(
            repository::get(&db, std::path::Path::new("."), &core, "zh-CN")
                .unwrap()
                .projects
                .len(),
            1
        );
        db.execute(
            "UPDATE projects SET status='archived' WHERE id='second'",
            [],
        )
        .unwrap();
        assert!(super::service::link_project(&mut db, &core, "second", true).is_err());
        db.execute("UPDATE projects SET status='active' WHERE id='second'", [])
            .unwrap();
    }
    assert!(super::service::link_project(&mut db, "foreign", "project", true).is_err());
    db.execute("DELETE FROM projects WHERE id='project'", [])
        .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_files", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn shared_tag_merge_and_rename_preserve_media_relationships_and_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES('entry','image','图','file','now','now')",[]).unwrap();
    let core = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "media".into(),
            id: "entry".into(),
        },
    )
    .unwrap();
    super::service::set_tags(&mut db, &core, "zh-CN", &["森林".into(), "树林".into()]).unwrap();
    let forest: String = db
        .query_row(
            "SELECT id FROM localized_tags WHERE name='森林'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let woods: String = db
        .query_row(
            "SELECT id FROM localized_tags WHERE name='树林'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    crate::organization::merge_tags(&mut db, vec![forest], &woods).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entry_tags", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    crate::organization::rename_tag(&mut db, &woods, "绿色树林").unwrap();
    let text: String = db
        .query_row(
            "SELECT text FROM media_search WHERE entry_id='entry'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(text.contains("绿色树林"));
    db.execute("INSERT INTO assets(id,name,description,share_url,created_at,updated_at) VALUES('unrelated','无关','','','now','now')",[]).unwrap();
    crate::db::delete_asset(&mut db, dir.path(), "unrelated").unwrap();
    crate::db::empty_trash(&mut db, dir.path()).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM media_entry_tags", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn versions_attach_only_active_originals_and_deletion_preserves_files() {
    let dir = tempfile::tempdir().unwrap();
    let db = old_fixture();
    super::migrate(&db).unwrap();
    crate::learning::migrate(&db).unwrap();
    let m = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "media".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    let a = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "legacy".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    let rel = "media-library/image/旧图/原图.png";
    std::fs::create_dir_all(dir.path().join("media-library/image/旧图")).unwrap();
    std::fs::write(dir.path().join(rel), b"original").unwrap();
    let detail = repository::get(&db, dir.path(), &m, "zh-CN").unwrap();
    let f = &detail.files[0];
    let mut input = CoreVersionInput {
        asset_id: a.clone(),
        file_id: f.id.clone(),
        version: "v001".into(),
        note: "".into(),
    };
    assert!(super::service::create_version(&db, dir.path(), &input).is_err());
    let attached = super::service::attach_file(&db, dir.path(), &a, &f.source_ref).unwrap();
    assert_eq!(
        super::service::attach_file(&db, dir.path(), &a, &f.source_ref)
            .unwrap()
            .id,
        attached.id
    );
    input.file_id = attached.id.clone();
    for label in ["v001", "v002", "Final"] {
        input.version = label.into();
        super::service::create_version(&db, dir.path(), &input).unwrap();
    }
    assert!(super::service::create_version(&db, dir.path(), &input).is_err());
    let version = repository::get(&db, dir.path(), &a, "zh-CN")
        .unwrap()
        .versions[0]
        .id
        .clone();
    super::service::delete_version(&db, &version).unwrap();
    assert!(dir.path().join(rel).is_file());
    let mut invalid = f.source_ref.clone();
    invalid.role = "thumbnail".into();
    assert!(super::service::attach_file(&db, dir.path(), &a, &invalid).is_err());
    db.execute("UPDATE media_entries SET deleted_at='now'", [])
        .unwrap();
    assert!(super::service::attach_file(&db, dir.path(), &a, &f.source_ref).is_err());
    std::fs::remove_file(dir.path().join(rel)).unwrap();
    input.version = "v003".into();
    assert!(super::service::create_version(&db, dir.path(), &input).is_err());
}

#[test]
fn sources_aggregate_old_values_validate_urls_and_preserve_old_edits() {
    let db = old_fixture();
    super::migrate(&db).unwrap();
    db.execute(
        "UPDATE assets SET source_url='https://www.fab.com/listings/owned',author='Artist'",
        [],
    )
    .unwrap();
    let a = repository::register_origin(
        &db,
        &CoreOrigin {
            kind: "legacy".into(),
            id: OLD_ID.into(),
        },
    )
    .unwrap();
    let detail = repository::get(&db, std::path::Path::new("."), &a, "zh-CN").unwrap();
    assert_eq!(detail.sources.len(), 1);
    assert!(!detail.sources[0].editable);
    let mut input = CoreSourceInput {
        asset_id: a.clone(),
        source_type: "web".into(),
        url: "https://example.com/reference".into(),
        author: "创作者".into(),
    };
    let source = super::service::add_source(&db, &input).unwrap();
    assert_eq!(
        source.id,
        super::service::add_source(&db, &input).unwrap().id
    );
    for url in [
        "javascript:alert(1)",
        "file:///C:/secret",
        "https://name:password@example.com/",
    ] {
        input.url = url.into();
        assert!(super::service::add_source(&db, &input).is_err());
    }
    assert!(super::service::remove_source(&db, &a, &detail.sources[0].id).is_err());
    super::service::remove_source(&db, &a, &source.id).unwrap();
    assert_eq!(
        repository::get(&db, std::path::Path::new("."), &a, "zh-CN")
            .unwrap()
            .sources
            .len(),
        1
    );
    assert_eq!(detail.knowledge_links.len(), 1);
}

#[test]
fn source_search_uses_the_same_host_rules_and_download_sources_as_details() {
    let db = old_fixture();
    super::migrate(&db).unwrap();
    super::intelligence_migration::migrate(&db).unwrap();
    crate::downloader::service::migrate(&db).unwrap();
    db.execute(
        "UPDATE assets SET source_url='https://evil.example/fab.com/fake'",
        [],
    )
    .unwrap();
    db.execute(
        "UPDATE media_entries SET source_url='https://www.fab.com/listings/item'",
        [],
    )
    .unwrap();
    db.execute("INSERT INTO download_tasks(id,task_id,url,status,created_at) VALUES('download','task','https://youtube.com/watch?v=one','completed','now')",[]).unwrap();
    db.execute("INSERT INTO media_sources(id,media_id,platform,url,title,author,download_task_id,created_at) VALUES('source',?1,'youtube','https://YouTube.com:443/watch?v=one','视频','作者','download','now')",[OLD_ID]).unwrap();
    let search = |source: &str| {
        repository::search(
            &db,
            &CoreSearchRequest {
                query: String::new(),
                asset_type: None,
                source: Some(source.into()),
                project_id: None,
                locale: "zh-CN".into(),
                offset: 0,
                limit: 80,
            },
        )
        .unwrap()
    };
    let fab = search("fab");
    assert_eq!(fab.total, 1);
    assert_eq!(fab.items[0].origin.kind, "media");
    assert_eq!(search("youtube").total, 1);
    assert_eq!(search("web").total, 1);
    assert_eq!(search("local").total, 0);
}

#[test]
fn scan_summary_counts_existing_library_and_trash_duplicates_before_confirmation() {
    let source = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    let path = source.path().join("已有.png");
    std::fs::write(&path, b"same-content").unwrap();
    std::fs::write(source.path().join("新.png"), b"different").unwrap();
    let hash = crate::file_digest::digest_file(&path).unwrap().1;
    let db = old_fixture();
    super::migrate(&db).unwrap();
    db.execute("UPDATE media_files SET checksum=?1", [hash])
        .unwrap();
    let manifest =
        super::scanner::manifest_path(library.path(), &uuid::Uuid::new_v4().to_string()).unwrap();
    for deleted in [false, true] {
        db.execute(
            "UPDATE media_entries SET deleted_at=?1",
            [if deleted { Some("now") } else { None }],
        )
        .unwrap();
        let summary =
            super::scanner::scan(source.path(), library.path(), &manifest, &|_| {}, &|| false)
                .unwrap();
        let summary = super::service::annotate_scan(&db, &manifest, summary, &|| false).unwrap();
        assert_eq!((summary.importable, summary.duplicate), (1, 1));
        let page = super::scanner::read_page(&manifest, 0, 100).unwrap();
        let existing = page.items.iter().find(|r| r.status == "duplicate").unwrap();
        assert!(existing.duplicate_asset_id.is_some());
        assert_eq!(existing.message.contains("回收站"), deleted);
    }
}
