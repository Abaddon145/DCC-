use super::repository;
use rusqlite::{params, Connection};

fn fixture() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::open_database(&dir.path().join("library.db")).unwrap();
    (dir, db)
}

fn asset(db: &Connection, name: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO assets(id,name,share_url,created_at,updated_at) VALUES(?1,?2,'','now','now')",
        params![id, name],
    )
    .unwrap();
    repository::register_origin(
        db,
        &super::models::CoreOrigin {
            kind: "legacy".into(),
            id,
        },
    )
    .unwrap()
}

#[test]
fn relations_migration_installs_indexed_foreign_key_table() {
    let (_dir, db) = fixture();
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='asset_relations'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "database opening must install asset relations");
    let foreign_keys: i64 = db.query_row("SELECT count(*) FROM pragma_foreign_key_list('asset_relations') WHERE \"table\"='asset_core' AND on_delete='CASCADE'", [], |r|r.get(0)).unwrap();
    assert_eq!(foreign_keys, 2);
    let indexes: i64 = db.query_row("SELECT count(*) FROM sqlite_master WHERE type='index' AND tbl_name='asset_relations' AND name IN ('idx_asset_relations_source','idx_asset_relations_target')", [], |r|r.get(0)).unwrap();
    assert_eq!(indexes, 2);
    // IDs and schema stay intact on re-opening a partially implemented development phase.
    let old = asset(&db, "既有资产");
    let reopened = crate::db::open_database(&_dir.path().join("library.db")).unwrap();
    assert_eq!(repository::origin(&reopened, &old).unwrap().kind, "legacy");
}

fn relation(
    db: &Connection,
    source: &str,
    target: &str,
    kind: &str,
) -> super::intelligence_models::AssetRelation {
    super::relations::create(
        db,
        &super::intelligence_models::RelationInput {
            source_asset_id: source.into(),
            target_asset_id: target.into(),
            relation_type: kind.into(),
        },
    )
    .unwrap()
}

#[test]
fn relations_normalize_symmetric_edges_and_keep_direction() {
    let (_dir, db) = fixture();
    let a = asset(&db, "参考图");
    let b = asset(&db, "生成结果");
    for kind in ["related_to", "variant_of"] {
        let first = relation(&db, &a, &b, kind);
        let second = relation(&db, &b, &a, kind);
        assert_eq!(first.id, second.id);
        assert!(first.source_asset_id < first.target_asset_id);
    }
    let forward = relation(&db, &a, &b, "generated_from");
    let reverse = relation(&db, &b, &a, "generated_from");
    assert_ne!(forward.id, reverse.id);
    assert_eq!(
        (
            forward.source_asset_id.as_str(),
            forward.target_asset_id.as_str()
        ),
        (a.as_str(), b.as_str())
    );
    assert_eq!(relation(&db, &a, &b, "generated_from").id, forward.id);
    for kind in ["inspired_by", "derived_from", "used_in", "reference_of"] {
        relation(&db, &a, &b, kind);
    }
    for (source, target, kind) in [
        (&a, &a, "related_to"),
        (&a, &b, "unknown"),
        (&a, &uuid::Uuid::new_v4().to_string(), "related_to"),
        (&a, &"../outside".to_string(), "related_to"),
    ] {
        assert!(super::relations::create(
            &db,
            &super::intelligence_models::RelationInput {
                source_asset_id: source.clone(),
                target_asset_id: target.clone(),
                relation_type: kind.into()
            }
        )
        .is_err());
    }
    let owner = repository::origin(&db, &b).unwrap();
    db.execute("UPDATE assets SET deleted_at='now' WHERE id=?1", [owner.id])
        .unwrap();
    assert!(super::relations::create(
        &db,
        &super::intelligence_models::RelationInput {
            source_asset_id: a,
            target_asset_id: b,
            relation_type: "reference_of".into()
        }
    )
    .is_err());
}

#[test]
fn graph_pages_81_neighbors_and_marks_deleted_endpoints() {
    let (_dir, db) = fixture();
    let center = asset(&db, "中心");
    let mut neighbors = vec![];
    for n in 0..81 {
        let id = asset(&db, &format!("邻居{n}"));
        relation(&db, &id, &center, "used_in");
        neighbors.push(id);
    }
    neighbors.sort();
    relation(&db, &center, &neighbors[0], "inspired_by"); // parallel edges still one neighbor
    let request = |offset, limit| super::intelligence_models::GraphRequest {
        asset_id: center.clone(),
        locale: "zh-CN".into(),
        offset,
        limit,
    };
    let page = super::relations::graph(&db, &request(0, 500)).unwrap();
    assert_eq!(
        (
            page.total,
            page.limit,
            page.nodes.len(),
            page.relations.len()
        ),
        (81, 80, 81, 81)
    );
    assert_eq!(page.nodes[0].id, center);
    assert_eq!(page.nodes[1].id, neighbors[0]);
    let next = super::relations::graph(&db, &request(80, 80)).unwrap();
    assert_eq!((next.nodes.len(), next.relations.len()), (2, 1));
    assert_eq!(next.nodes[1].id, neighbors[80]);
    let owner = repository::origin(&db, &neighbors[0]).unwrap();
    db.execute(
        "UPDATE assets SET deleted_at='now' WHERE id=?1",
        [&owner.id],
    )
    .unwrap();
    assert!(super::relations::graph(&db, &request(0, 80)).unwrap().nodes[1].deleted);
    db.execute("UPDATE assets SET deleted_at=NULL WHERE id=?1", [&owner.id])
        .unwrap();
    assert!(!super::relations::graph(&db, &request(0, 80)).unwrap().nodes[1].deleted);
    db.execute("DELETE FROM assets WHERE id=?1", [owner.id])
        .unwrap();
    let page = super::relations::graph(&db, &request(0, 80)).unwrap();
    assert_eq!(
        (page.total, page.nodes.len(), page.relations.len()),
        (80, 81, 80)
    );
    assert!(super::relations::graph(&db, &request(usize::MAX, 80)).is_err());
    let edge = page.relations[0].clone();
    let stranger = asset(&db, "无关联资产");
    assert!(super::relations::delete(&db, &stranger, &edge.id).is_err());
    super::relations::delete(&db, &center, &edge.id).unwrap();
    assert_eq!(
        super::relations::graph(&db, &request(0, 80)).unwrap().total,
        79
    );
    let mut invalid = request(0, 80);
    invalid.locale = "other".into();
    assert!(super::relations::graph(&db, &invalid).is_err());
}

fn managed_image(db: &Connection, base: &std::path::Path, owner: &str) {
    let source = repository::origin(db, owner).unwrap().id;
    let path = base.join("selected.png");
    image::RgbaImage::from_pixel(2048, 1024, image::Rgba([90, 110, 130, 255]))
        .save(&path)
        .unwrap();
    db.execute("INSERT INTO images(id,asset_id,original_name,original_rel_path,thumbnail_rel_path,created_at) VALUES(?1,?2,'selected.png','selected.png','selected.png','now')",params![uuid::Uuid::new_v4().to_string(),source]).unwrap();
    repository::register_files(db, owner).unwrap();
}

#[test]
fn analysis_preview_contains_only_selected_bounded_material() {
    use super::intelligence_models::AnalysisMode;
    use base64::Engine;
    let (dir, db) = fixture();
    let owner = asset(&db, "选中的图片");
    managed_image(&db, dir.path(), &owner);
    let _other = asset(&db, "不应发送的素材内容");
    let preview =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Visual, &|| {
            false
        })
        .unwrap();
    let json = serde_json::to_string(&preview).unwrap();
    assert!(!json.contains("不应发送"));
    assert!(!json.contains(&dir.path().to_string_lossy().to_string()));
    assert_eq!(preview.images.len(), 1);
    assert!(preview.images.len() <= 4);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            preview.images[0]
                .data_url
                .strip_prefix("data:image/jpeg;base64,")
                .unwrap(),
        )
        .unwrap();
    assert!(bytes.len() <= 256 * 1024);
    let resized = image::load_from_memory(&bytes).unwrap();
    assert!(resized.width() <= 1024 && resized.height() <= 1024);
    assert!(preview.warnings.iter().any(|w| w.contains("预览")));
    assert_eq!(
        crate::ai::analyzer::video_times(100.0).unwrap(),
        vec![10.0, 35.0, 65.0, 90.0]
    );
    assert!(crate::ai::analyzer::video_times(f64::NAN).is_err());
    assert!(crate::ai::analyzer::prepare(
        &db,
        dir.path(),
        None,
        &owner,
        AnalysisMode::Visual,
        &|| true
    )
    .is_err());
}

#[test]
fn analysis_confirmed_input_rejects_replaced_original_and_changed_text() {
    use super::intelligence_models::{AnalysisMode, AnalysisRequest};
    let (dir, db) = fixture();
    let owner = asset(&db, "图片");
    managed_image(&db, dir.path(), &owner);
    let preview =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Visual, &|| {
            false
        })
        .unwrap();
    let request = AnalysisRequest {
        asset_id: owner.clone(),
        preview_id: preview.id.clone(),
        force: false,
    };
    assert!(crate::ai::analyzer::load_confirmed(&db, dir.path(), &request).is_ok());
    let origin = repository::origin(&db, &owner).unwrap().id;
    db.execute(
        "UPDATE assets SET description='用户新增资料' WHERE id=?1",
        [&origin],
    )
    .unwrap();
    assert!(crate::ai::analyzer::load_confirmed(&db, dir.path(), &request).is_err());
    db.execute("UPDATE assets SET description='' WHERE id=?1", [origin])
        .unwrap();
    image::RgbaImage::from_pixel(20, 20, image::Rgba([20, 30, 40, 255]))
        .save(dir.path().join("selected.png"))
        .unwrap();
    assert!(crate::ai::analyzer::load_confirmed(&db, dir.path(), &request).is_err());
    std::fs::remove_file(dir.path().join("selected.png")).unwrap();
    assert!(crate::ai::analyzer::load_confirmed(&db, dir.path(), &request).is_err());
    assert!(crate::ai::analyzer::prepare(
        &db,
        dir.path(),
        None,
        &owner,
        AnalysisMode::Visual,
        &|| false
    )
    .is_err());
    let other = asset(&db, "另一个资产");
    assert!(crate::ai::analyzer::load_confirmed(
        &db,
        dir.path(),
        &AnalysisRequest {
            asset_id: other,
            ..request.clone()
        }
    )
    .is_err());
    assert!(crate::ai::analyzer::load_confirmed(
        &db,
        dir.path(),
        &AnalysisRequest {
            preview_id: "../escape".into(),
            ..request.clone()
        }
    )
    .is_err());
    let foreign = tempfile::tempdir().unwrap();
    assert!(crate::ai::analyzer::load_confirmed(&db, foreign.path(), &request).is_err());
    assert!(serde_json::from_value::<AnalysisRequest>(
        serde_json::json!({"assetId":owner,"previewId":preview.id,"force":false,"path":"C:/secret"})
    )
    .is_err());
}

#[test]
fn analysis_cache_is_keyed_by_fingerprint_model_mode_version() {
    use super::intelligence_models::{AnalysisAnswer, AnalysisMode, AnalysisRequest};
    let (dir, db) = fixture();
    let owner = asset(&db, "文字资产");
    let preview =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Text, &|| false)
            .unwrap();
    assert!(preview.images.is_empty());
    let request = AnalysisRequest {
        asset_id: owner.clone(),
        preview_id: preview.id,
        force: false,
    };
    let input = crate::ai::analyzer::load_confirmed(&db, dir.path(), &request).unwrap();
    let answer = AnalysisAnswer {
        description: "雨夜未来城市".into(),
        keywords: vec!["雨夜".into()],
        scene: "城市".into(),
        style: String::new(),
        camera: String::new(),
        lighting: String::new(),
        mood: String::new(),
    };
    let saved = super::analysis::save(&db, &input, &answer, Some(42)).unwrap();
    assert_eq!(
        super::analysis::cached(&db, &owner, &preview.fingerprint, AnalysisMode::Text)
            .unwrap()
            .unwrap()
            .id,
        saved.id
    );
    assert!(
        super::analysis::cached(&db, &owner, &preview.fingerprint, AnalysisMode::Visual)
            .unwrap()
            .is_none()
    );
    for (column, value) in [("model", "different-model"), ("format_version", "99")] {
        let old: String = db
            .query_row(
                &format!("SELECT CAST({column} AS TEXT) FROM asset_ai_analysis WHERE id=?1"),
                [&saved.id],
                |r| r.get(0),
            )
            .unwrap();
        db.execute(
            &format!("UPDATE asset_ai_analysis SET {column}=?1 WHERE id=?2"),
            params![value, saved.id],
        )
        .unwrap();
        assert!(
            super::analysis::cached(&db, &owner, &preview.fingerprint, AnalysisMode::Text)
                .unwrap()
                .is_none()
        );
        db.execute(
            &format!("UPDATE asset_ai_analysis SET {column}=?1 WHERE id=?2"),
            params![old, saved.id],
        )
        .unwrap();
    }
    let origin = repository::origin(&db, &owner).unwrap();
    db.execute(
        "UPDATE assets SET description='新的说明' WHERE id=?1",
        [origin.id],
    )
    .unwrap();
    let changed =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Text, &|| false)
            .unwrap();
    assert_ne!(changed.fingerprint, preview.fingerprint);
    assert!(changed.cached_analysis_id.is_none());
    assert_eq!(
        super::analysis::latest(&db, &owner).unwrap().unwrap().id,
        saved.id
    );
    assert!(super::analysis::save(
        &db,
        &input,
        &AnalysisAnswer {
            description: String::new(),
            keywords: vec![],
            scene: String::new(),
            style: String::new(),
            camera: String::new(),
            lighting: String::new(),
            mood: String::new()
        },
        None
    )
    .is_err());
}

#[test]
fn analysis_refuses_expired_corrupt_and_cancelled_previews() {
    use super::intelligence_models::{AnalysisMode, AnalysisRequest};
    let (dir, db) = fixture();
    let owner = asset(&db, "资料");
    let preview =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Text, &|| false)
            .unwrap();
    let request = AnalysisRequest {
        asset_id: owner,
        preview_id: preview.id,
        force: false,
    };
    assert!(
        crate::ai::analyzer::load_confirmed_controlled(&db, dir.path(), &request, &|| true)
            .is_err()
    );
    let path = dir
        .path()
        .join(".dcc/cache/analysis")
        .join(&request.preview_id)
        .join("input.json");
    let original = std::fs::read(&path).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
    json["created_at"] = serde_json::json!(chrono::Utc::now().timestamp() - 86401);
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(
        crate::ai::analyzer::load_confirmed(&db, dir.path(), &request)
            .err()
            .unwrap()
            .contains("过期")
    );
    std::fs::write(&path, b"not json").unwrap();
    assert!(
        crate::ai::analyzer::load_confirmed(&db, dir.path(), &request)
            .err()
            .unwrap()
            .contains("损坏")
    );
}

#[test]
fn analysis_refuses_corrupt_or_oversized_images() {
    use super::intelligence_models::AnalysisMode;
    let (dir, db) = fixture();
    let owner = asset(&db, "图片");
    managed_image(&db, dir.path(), &owner);
    std::fs::write(dir.path().join("selected.png"), b"broken png").unwrap();
    assert!(crate::ai::analyzer::prepare(
        &db,
        dir.path(),
        None,
        &owner,
        AnalysisMode::Visual,
        &|| false
    )
    .is_err());
    image::RgbImage::new(4001, 4000)
        .save(dir.path().join("selected.png"))
        .unwrap();
    assert!(crate::ai::analyzer::prepare(
        &db,
        dir.path(),
        None,
        &owner,
        AnalysisMode::Visual,
        &|| false
    )
    .is_err());
    assert!(!dir.path().join(".dcc/cache/analysis").exists());
}

#[cfg(windows)]
#[test]
fn analysis_cache_directory_cannot_redirect_through_junction() {
    use super::intelligence_models::AnalysisMode;
    let (dir, db) = fixture();
    let owner = asset(&db, "文字");
    let other = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".dcc").join("cache")).unwrap();
    let status = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(dir.path().join(".dcc").join("cache").join("analysis"))
        .arg(other.path())
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(crate::ai::analyzer::prepare(
        &db,
        dir.path(),
        None,
        &owner,
        AnalysisMode::Text,
        &|| false
    )
    .unwrap_err()
    .contains("链接"));
    assert_eq!(std::fs::read_dir(other.path()).unwrap().count(), 0);
}
