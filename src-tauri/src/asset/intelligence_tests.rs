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

fn analysis_answer() -> serde_json::Value {
    serde_json::json!({"description":"雨夜未来城市","keywords":["雨夜","赛博朋克"],"scene":"城市","style":"电影感","camera":"广角","lighting":"霓虹","mood":"安静"})
}
#[test]
fn analysis_rejects_empty_invalid_or_oversized_output() {
    use crate::ai::analyzer::parse_answer;
    assert!(parse_answer(&analysis_answer().to_string()).is_ok());
    assert!(parse_answer("not json").is_err());
    let mut value = analysis_answer();
    value["command"] = serde_json::json!("delete");
    assert!(parse_answer(&value.to_string()).is_err());
    value = analysis_answer();
    value["description"] = serde_json::json!("字".repeat(8001));
    assert!(parse_answer(&value.to_string()).is_err());
    value = analysis_answer();
    value["keywords"] = serde_json::json!(vec!["a"; 101]);
    assert!(parse_answer(&value.to_string()).is_err());
    value["keywords"] = serde_json::json!(["a".repeat(101)]);
    assert!(parse_answer(&value.to_string()).is_err());
    for key in [
        "description",
        "scene",
        "style",
        "camera",
        "lighting",
        "mood",
    ] {
        value[key] = serde_json::json!("");
    }
    value["keywords"] = serde_json::json!([]);
    assert!(parse_answer(&value.to_string()).is_err());
    assert!(parse_answer(&format!("{}{}", analysis_answer(), " ".repeat(65536))).is_err());
}
#[test]
fn analysis_cancel_or_library_switch_never_commits_late_reply() {
    use super::intelligence_models::{AnalysisMode, AnalysisRequest};
    let (dir, mut db) = fixture();
    let owner = asset(&db, "资料");
    let preview =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Text, &|| false)
            .unwrap();
    let request = AnalysisRequest {
        asset_id: owner.clone(),
        preview_id: preview.id,
        force: false,
    };
    assert!(super::analysis::complete_confirmed(
        &mut db,
        dir.path(),
        &request,
        &analysis_answer().to_string(),
        Some(50),
        &|| true
    )
    .is_err());
    let foreign = tempfile::tempdir().unwrap();
    assert!(super::analysis::complete_confirmed(
        &mut db,
        foreign.path(),
        &request,
        &analysis_answer().to_string(),
        Some(50),
        &|| false
    )
    .is_err());
    assert!(super::analysis::latest(&db, &owner).unwrap().is_none());
    assert!(super::analysis::complete_confirmed(
        &mut db,
        dir.path(),
        &request,
        &analysis_answer().to_string(),
        Some(50),
        &|| false
    )
    .is_ok());
}
#[test]
fn analysis_appends_keywords_without_overwriting_concurrent_tags() {
    use super::intelligence_models::{AnalysisMode, AnalysisRequest};
    let (dir, mut db) = fixture();
    let owner = asset(&db, "图像");
    let preview =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Text, &|| false)
            .unwrap();
    let input = crate::ai::analyzer::load_confirmed(
        &db,
        dir.path(),
        &AnalysisRequest {
            asset_id: owner.clone(),
            preview_id: preview.id,
            force: false,
        },
    )
    .unwrap();
    let answer = serde_json::from_value(analysis_answer()).unwrap();
    let saved = super::analysis::save(&db, &input, &answer, None).unwrap();
    super::service::set_tags(
        &mut db,
        &owner,
        "zh-CN",
        &["原有".into(), "稍后编辑".into(), "雨夜".into()],
    )
    .unwrap();
    super::analysis::apply_keywords(
        &mut db,
        &owner,
        &saved.id,
        "zh-CN",
        &["雨夜".into(), "赛博朋克".into()],
    )
    .unwrap();
    let tags = repository::read_asset(&db, &owner, "zh-CN").unwrap().tags;
    assert_eq!(tags.len(), 4);
    assert!(tags.contains(&"稍后编辑".into()));
    let before = tags;
    assert!(super::analysis::apply_keywords(
        &mut db,
        &owner,
        &saved.id,
        "zh-CN",
        &["未被建议".into()]
    )
    .is_err());
    let other = asset(&db, "另一资产");
    assert!(
        super::analysis::apply_keywords(&mut db, &other, &saved.id, "zh-CN", &["雨夜".into()])
            .is_err()
    );
    assert_eq!(
        repository::read_asset(&db, &owner, "zh-CN").unwrap().tags,
        before
    );
}
#[test]
fn analysis_job_uses_approved_input_and_saves_valid_reply() {
    use super::intelligence_models::{AnalysisMode, AnalysisRequest};
    use std::io::{Read, Write};
    let (dir, mut db) = fixture();
    let owner = asset(&db, "选中图像");
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
    let input = crate::ai::analyzer::load_confirmed(&db, dir.path(), &request).unwrap();
    let body = crate::ai::analyzer::request_body(&input);
    assert!(body["messages"][1]["content"][1]["image_url"]["url"] == preview.images[0].data_url);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let expected = body.clone();
    let answer = analysis_answer().to_string();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let mut buf = [0u8; 4096];
        let start = loop {
            let n = socket.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break p + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..start]);
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|s| s.trim().parse().unwrap())
            })
            .unwrap();
        while bytes.len() < start + length {
            let n = socket.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
        }
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes[start..start + length]).unwrap(),
            expected
        );
        let reply=serde_json::json!({"usage":{"total_tokens":67},"choices":[{"finish_reason":"stop","message":{"content":answer}}]}).to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",reply.len()).unwrap();
    });
    let service =
        crate::translation_service::TranslationService::open(&dir.path().join("usage.db")).unwrap();
    let reply = tauri::async_runtime::block_on(service.creative_analysis_fixture(
        &body,
        preview.estimated_tokens,
        &std::sync::atomic::AtomicBool::new(false),
        1,
        &url,
    ))
    .unwrap();
    let saved = super::analysis::complete_confirmed(
        &mut db,
        dir.path(),
        &request,
        &reply.content.unwrap(),
        reply.used,
        &|| false,
    )
    .unwrap();
    assert_eq!(saved.actual_tokens, Some(67));
    assert_eq!(service.usage().unwrap().creative_tokens, 67);
    server.join().unwrap();
}
#[test]
fn analysis_task_is_typed_and_duplicate_submission_is_coalesced() {
    let dir = tempfile::tempdir().unwrap();
    let registry = crate::tasks::TaskRegistry::load(dir.path().join("tasks.json"));
    let payload:crate::tasks::TaskRequest=serde_json::from_value(serde_json::json!({"kind":"aiAnalyze","request":{"assetId":uuid::Uuid::new_v4().to_string(),"previewId":uuid::Uuid::new_v4().to_string(),"force":false}})).expect("analysis task supported");
    let (first, _) = registry.submit("library".into(), payload.clone()).unwrap();
    let (second, _) = registry.submit("library".into(), payload).unwrap();
    assert_eq!(first.id, second.id);
}
#[test]
fn analysis_cached_result_can_be_read_by_id_without_returning_newer_unrelated_input() {
    use super::intelligence_models::{AnalysisMode, AnalysisRequest};
    let (dir, db) = fixture();
    let owner = asset(&db, "输入");
    let preview =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Text, &|| false)
            .unwrap();
    let input = crate::ai::analyzer::load_confirmed(
        &db,
        dir.path(),
        &AnalysisRequest {
            asset_id: owner.clone(),
            preview_id: preview.id,
            force: false,
        },
    )
    .unwrap();
    let first = super::analysis::save(
        &db,
        &input,
        &serde_json::from_value(analysis_answer()).unwrap(),
        None,
    )
    .unwrap();
    let mut different = analysis_answer();
    different["description"] = serde_json::json!("另一版结果");
    let second = super::analysis::save(
        &db,
        &input,
        &serde_json::from_value(different).unwrap(),
        None,
    )
    .unwrap();
    db.execute(
        "UPDATE asset_ai_analysis SET created_at='2099-01-01' WHERE id=?1",
        [&second.id],
    )
    .unwrap();
    assert_eq!(
        super::analysis::by_id(&db, &owner, &first.id)
            .unwrap()
            .unwrap()
            .id,
        first.id
    );
    let other = asset(&db, "别的素材");
    assert!(super::analysis::by_id(&db, &other, &first.id)
        .unwrap()
        .is_none());
}

#[test]
fn search_matches_description_analysis_source_and_project_without_losing_filters() {
    use super::{
        intelligence_models::{AnalysisMode, AnalysisRequest},
        models::CoreSearchRequest,
    };
    let (dir, mut db) = fixture();
    let owner = asset(&db, "普通图像");
    let origin = repository::origin(&db, &owner).unwrap().id;
    db.execute("UPDATE assets SET description='用户描述',author='原作者',source_url='https://example.com/原来源' WHERE id=?1",[&origin]).unwrap();
    super::service::set_tags(&mut db, &owner, "en", &["English Tag".into()]).unwrap();
    super::service::add_source(
        &db,
        &super::models::CoreSourceInput {
            asset_id: owner.clone(),
            source_type: "fab".into(),
            url: "https://fab.com/listings/entry".into(),
            author: "附加作者".into(),
        },
    )
    .unwrap();
    let project = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO projects(id,name,project_type,status,created_at,updated_at) VALUES(?1,'雨夜创作','scene','active','now','now')",[&project]).unwrap();
    super::service::link_project(&mut db, &owner, &project, true).unwrap();
    let preview =
        crate::ai::analyzer::prepare(&db, dir.path(), None, &owner, AnalysisMode::Text, &|| false)
            .unwrap();
    let input = crate::ai::analyzer::load_confirmed(
        &db,
        dir.path(),
        &AnalysisRequest {
            asset_id: owner.clone(),
            preview_id: preview.id,
            force: false,
        },
    )
    .unwrap();
    let mut answer = analysis_answer();
    answer["description"] = serde_json::json!("分析描述");
    answer["keywords"] = serde_json::json!(["未来都市"]);
    super::analysis::save(&db, &input, &serde_json::from_value(answer).unwrap(), None).unwrap();
    let other = asset(&db, "不匹配");
    let mut req: CoreSearchRequest = serde_json::from_value(serde_json::json!({})).unwrap();
    for query in [
        "用户描述",
        "分析描述",
        "未来都市",
        "原作者",
        "原来源",
        "附加作者",
        "listings/entry",
        "雨夜创作",
        "english tag",
    ] {
        req.query = query.into();
        let found = repository::search(&db, &req).unwrap();
        assert_eq!(
            found.items.iter().map(|a| &a.id).collect::<Vec<_>>(),
            vec![&owner],
            "field: {query}"
        );
    }
    req.query = String::new();
    req.project_id = Some(project.clone());
    req.source = Some("fab".into());
    req.asset_type = Some("asset".into());
    assert_eq!(repository::search(&db, &req).unwrap().total, 1);
    req.asset_type = Some("image".into());
    assert_eq!(repository::search(&db, &req).unwrap().total, 0);
    req.asset_type = None;
    req.project_id = Some(uuid::Uuid::new_v4().to_string());
    assert_eq!(repository::search(&db, &req).unwrap().total, 0);
    req.project_id = Some(project);
    db.execute("UPDATE assets SET deleted_at='now' WHERE id=?1", [&origin])
        .unwrap();
    assert_eq!(repository::search(&db, &req).unwrap().total, 0);
    assert_eq!(
        repository::search(&db, &serde_json::from_value(serde_json::json!({})).unwrap())
            .unwrap()
            .items[0]
            .id,
        other
    );
    let (_, foreign) = fixture();
    assert_eq!(repository::search(&foreign, &req).unwrap().total, 0);
}
#[test]
fn search_treats_quotes_wildcards_and_empty_query_literally() {
    let (_, db) = fixture();
    let exact = asset(&db, "文件 50%_完成 ' \"引用\"");
    asset(&db, "文件 500完成");
    for query in ["50%_", "'", "\"引用\""] {
        let req = serde_json::from_value(serde_json::json!({"query":query})).unwrap();
        let page = repository::search(&db, &req).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, exact);
    }
    assert_eq!(
        repository::search(
            &db,
            &serde_json::from_value(serde_json::json!({"query":"  "})).unwrap()
        )
        .unwrap()
        .total,
        2
    );
}
fn prompt_input() -> super::intelligence_models::PromptInput {
    serde_json::from_value(serde_json::json!({"assetId":null,"expectedRevision":null,"name":"雨夜城市 Prompt","description":"创作说明","content":"cinematic city","model":"记录模型","parameters":{"seed":42}})).unwrap()
}
#[test]
fn prompt_crud_requires_json_object_and_revision() {
    let (_dir, mut db) = fixture();
    let mut input = prompt_input();
    let saved = super::prompt::upsert(&mut db, &input).unwrap();
    assert_eq!(saved.asset.asset_type, "prompt");
    assert_eq!(saved.revision, 1);
    input.asset_id = Some(saved.asset.id.clone());
    input.expected_revision = Some(saved.revision);
    input.content = "new content".into();
    let updated = super::prompt::upsert(&mut db, &input).unwrap();
    assert_eq!(updated.revision, 2);
    assert_eq!(
        super::prompt::get(&db, &saved.asset.id).unwrap().content,
        "new content"
    );
    assert!(super::prompt::upsert(&mut db, &input).is_err());
    input.expected_revision = Some(2);
    input.parameters = serde_json::json!([]);
    assert!(super::prompt::upsert(&mut db, &input).is_err());
    input.parameters = serde_json::json!({});
    input.content = "字".repeat(100001);
    assert!(super::prompt::upsert(&mut db, &input).is_err());
    assert_eq!(
        super::prompt::get(&db, &saved.asset.id).unwrap().content,
        "new content"
    );
}
#[test]
fn prompt_trash_restore_purge_keeps_external_assets_and_shared_tags() {
    let (dir, mut db) = fixture();
    let other = asset(&db, "保留的普通素材");
    let p = super::prompt::upsert(&mut db, &prompt_input()).unwrap();
    super::service::set_tags(&mut db, &p.asset.id, "zh-CN", &["文本专属".into()]).unwrap();
    relation(&db, &p.asset.id, &other, "generated_from");
    let batch = super::entities::trash(&mut db, &[p.asset.id.clone()]).unwrap();
    assert!(
        repository::read_asset(&db, &p.asset.id, "zh-CN")
            .unwrap()
            .deleted
    );
    crate::db::restore_trash_batch(&mut db, &batch.batch_id).unwrap();
    assert_eq!(
        super::prompt::get(&db, &p.asset.id).unwrap().asset.tags,
        vec!["文本专属"]
    );
    let origin = repository::origin(&db, &other).unwrap();
    let b = crate::db::delete_library_items(
        &mut db,
        crate::models::DeleteRequest {
            asset_ids: vec![origin.id],
            category_id: None,
        },
    )
    .unwrap();
    crate::db::purge_trash_batch(&mut db, dir.path(), &b.batch_id).unwrap();
    assert_eq!(
        super::prompt::get(&db, &p.asset.id).unwrap().asset.tags,
        vec!["文本专属"]
    );
    let keeper = asset(&db, "另一素材");
    relation(&db, &p.asset.id, &keeper, "related_to");
    let batch = super::entities::trash(&mut db, &[p.asset.id.clone()]).unwrap();
    crate::db::purge_trash_batch(&mut db, dir.path(), &batch.batch_id).unwrap();
    assert!(super::prompt::get(&db, &p.asset.id).is_err());
    assert!(repository::origin(&db, &keeper).is_ok());
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_relations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
fn downgrade_registry_to_v21(db: &Connection) {
    // v21 has no Shot synchronization trigger; don't leave a v23 trigger in this old-schema fixture.
    db.execute_batch("DROP TRIGGER IF EXISTS shot_unit_sync; DROP TABLE IF EXISTS learning_origin_map; DROP TABLE IF EXISTS learning_source_vault_bindings; DROP TABLE learning_source_profiles;")
        .unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; DROP TRIGGER asset_core_version_cleanup; CREATE TABLE registry_v21(id TEXT PRIMARY KEY,legacy_asset_id TEXT UNIQUE REFERENCES assets(id) ON DELETE CASCADE,media_entry_id TEXT UNIQUE REFERENCES media_entries(id) ON DELETE CASCADE,created_at TEXT NOT NULL,CHECK((legacy_asset_id IS NULL)<>(media_entry_id IS NULL))); INSERT INTO registry_v21(id,legacy_asset_id,media_entry_id,created_at) SELECT id,legacy_asset_id,media_entry_id,created_at FROM asset_core; DROP TABLE asset_core; ALTER TABLE registry_v21 RENAME TO asset_core;").unwrap();
    db.execute_batch(include_str!("schema.sql")).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys=ON; UPDATE schema_meta SET value='21' WHERE key='schema_version';",
    )
    .unwrap();
}
#[test]
fn migration_preserves_v21_identity_files_versions_and_triggers() {
    let (dir, db) = fixture();
    let owner = asset(&db, "旧资产");
    let origin = repository::origin(&db, &owner).unwrap();
    let image = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO images(id,asset_id,original_name,original_rel_path,thumbnail_rel_path,is_cover,sort_order,created_at) VALUES(?1,?2,'原图.png','images/original.png','images/thumb.png',1,0,'old')",params![image,origin.id]).unwrap();
    repository::register_files(&db, &owner).unwrap();
    let file: String = db
        .query_row(
            "SELECT id FROM asset_files WHERE asset_id=?1 AND role='original'",
            [&owner],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::create_dir_all(dir.path().join("images")).unwrap();
    std::fs::write(dir.path().join("images/original.png"), b"original").unwrap();
    super::service::create_version(
        &db,
        dir.path(),
        &super::models::CoreVersionInput {
            asset_id: owner.clone(),
            file_id: file.clone(),
            version: "v001".into(),
            note: "old".into(),
        },
    )
    .unwrap();
    downgrade_registry_to_v21(&db);
    super::intelligence_migration::migrate(&db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('asset_core') WHERE name='entity_id'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(repository::origin(&db, &owner).unwrap().id, origin.id);
    assert_eq!(
        db.query_row(
            "SELECT file_id FROM asset_versions WHERE asset_id=?1",
            [&owner],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        file
    );
    assert_eq!(
        db.query_row(
            "SELECT original_rel_path FROM images WHERE id=?1",
            [&image],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "images/original.png"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    db.execute("DELETE FROM assets WHERE id=?1", [origin.id])
        .unwrap();
    assert!(repository::origin(&db, &owner).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_versions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(db);
    crate::db::open_database(&dir.path().join("library.db")).unwrap();
}
#[test]
fn migration_failure_rolls_back_and_restores_foreign_keys() {
    let (_dir, db) = fixture();
    let owner = asset(&db, "保留");
    downgrade_registry_to_v21(&db);
    db.execute_batch("PRAGMA foreign_keys=OFF; INSERT INTO asset_sources(id,asset_id,source_type,url,author,created_at) VALUES('orphan','missing','web','https://example.com','','old'); PRAGMA foreign_keys=ON;").unwrap();
    assert!(super::intelligence_migration::migrate(&db).is_err());
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM asset_core WHERE id=?1",
            [&owner],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('asset_core') WHERE name='entity_id'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn migration_is_idempotent_on_restored_v22_backup() {
    let (_dir, mut db) = fixture();
    let p = super::prompt::upsert(&mut db, &prompt_input()).unwrap();
    super::intelligence_migration::migrate(&db).unwrap();
    super::intelligence_migration::migrate(&db).unwrap();
    assert_eq!(super::prompt::get(&db, &p.asset.id).unwrap().revision, 1);
    assert_eq!(
        db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

fn shot_input(db: &Connection) -> super::intelligence_models::ShotInput {
    let project = uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO projects(id,name,project_type,status,created_at,updated_at) VALUES(?1,'短片','animation','active','now','now')",[&project]).unwrap();
    let scene = crate::projects::upsert_project_unit(
        db,
        serde_json::from_value(
            serde_json::json!({"projectId":project,"kind":"scene","name":"沙漠"}),
        )
        .unwrap(),
    )
    .unwrap();
    serde_json::from_value(serde_json::json!({"projectId":project,"parentSceneId":scene.id,"act":"ACT1","shotNumber":"001","name":"远景","description":"创作描述","status":"todo"})).unwrap()
}
fn saved_shot_input(
    s: &super::intelligence_models::ShotAsset,
) -> super::intelligence_models::ShotInput {
    serde_json::from_value(serde_json::json!({"assetId":s.asset.id,"expectedRevision":s.revision,"projectId":s.project_id,"projectUnitId":s.project_unit_id,"parentSceneId":s.unit.as_ref().and_then(|u|u.parent_id.clone()),"act":s.act,"shotNumber":s.shot_number,"name":s.asset.name,"description":s.asset.description,"status":s.status})).unwrap()
}
#[test]
fn shot_registration_reuses_existing_unit_and_core_identity() {
    let (_, mut db) = fixture();
    let input = shot_input(&db);
    let s = super::shot::upsert(&mut db, &input).unwrap();
    let mut bind = saved_shot_input(&s);
    bind.asset_id = None;
    bind.expected_revision = None;
    bind.name = "不应覆盖原镜头".into();
    let again = super::shot::upsert(&mut db, &bind).unwrap();
    assert_eq!(again.asset.id, s.asset.id);
    assert_eq!(again.asset.name, "远景");
    assert_eq!(again.revision, 1);
    assert_eq!(
        super::shot::list(&db, input.project_id.as_deref().unwrap(), 0, 200)
            .unwrap()
            .total,
        1
    );
    let req = serde_json::from_value(
        serde_json::json!({"assetType":"shot","projectId":input.project_id}),
    )
    .unwrap();
    assert_eq!(
        repository::search(&db, &req).unwrap().items[0].id,
        s.asset.id
    );
    let mut invalid = bind.clone();
    invalid.project_id = Some(uuid::Uuid::new_v4().to_string());
    assert!(super::shot::upsert(&mut db, &invalid).is_err());
    invalid = bind;
    invalid.act = "a".repeat(81);
    assert!(super::shot::upsert(&mut db, &invalid).is_err());
}
#[test]
fn shot_creation_failure_rolls_back_unit_and_entity() {
    let (_, mut db) = fixture();
    let input = shot_input(&db);
    db.execute_batch("CREATE TEMP TRIGGER fail_shot BEFORE INSERT ON shots BEGIN SELECT RAISE(ABORT,'故障注入'); END;").unwrap();
    assert!(super::shot::upsert(&mut db, &input).is_err());
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM project_units WHERE kind='shot'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM asset_entities WHERE kind='shot'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_shot").unwrap();
    assert!(super::shot::upsert(&mut db, &input).is_ok());
}
#[test]
fn shot_and_legacy_unit_edits_remain_consistent() {
    let (_, mut db) = fixture();
    let input = shot_input(&db);
    let s = super::shot::upsert(&mut db, &input).unwrap();
    let old = saved_shot_input(&s);
    let u = s.unit.unwrap();
    crate::projects::upsert_project_unit(&db,serde_json::from_value(serde_json::json!({"id":u.id,"projectId":u.project_id,"parentId":u.parent_id,"kind":"shot","name":"旧入口新名称","description":"旧入口新描述","startFrame":10,"endFrame":80,"frameRate":30})).unwrap()).unwrap();
    let updated = super::shot::get(&db, &s.asset.id).unwrap();
    assert_eq!(updated.asset.name, "旧入口新名称");
    assert_eq!(updated.asset.description, "旧入口新描述");
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.unit.as_ref().unwrap().end_frame, Some(80));
    assert!(super::shot::upsert(&mut db, &old).is_err());
    let mut edit = saved_shot_input(&updated);
    edit.name = "Shot 编辑新名称".into();
    edit.status = "review".into();
    let saved = super::shot::upsert(&mut db, &edit).unwrap();
    assert_eq!(saved.revision, 3);
    assert_eq!(saved.unit.as_ref().unwrap().name, saved.asset.name);
    assert_eq!(saved.unit.as_ref().unwrap().frame_rate, Some(30.0));
    edit.expected_revision = Some(3);
    edit.name = saved.asset.name.clone();
    edit.description = format!(" {} ", saved.asset.description);
    edit.status = "done".into();
    let trimmed = super::shot::upsert(&mut db, &edit).unwrap();
    assert_eq!(
        trimmed.asset.description,
        trimmed.unit.unwrap().description,
        "bound text must stay authoritative even when legacy editor trims whitespace"
    );
    edit.expected_revision = Some(4);
    edit.status = "invalid".into();
    assert!(super::shot::upsert(&mut db, &edit).is_err());
}
#[test]
fn shot_project_or_unit_delete_preserves_asset_network() {
    let (_, mut db) = fixture();
    let input = shot_input(&db);
    let s = super::shot::upsert(&mut db, &input).unwrap();
    let p = super::prompt::upsert(&mut db, &prompt_input()).unwrap();
    relation(&db, &p.asset.id, &s.asset.id, "used_in");
    crate::projects::delete_project_unit(&db, s.project_unit_id.as_deref().unwrap()).unwrap();
    let detached = super::shot::get(&db, &s.asset.id).unwrap();
    assert!(detached.unit.is_none());
    assert!(detached.project_unit_id.is_none());
    assert_eq!(detached.asset.name, "远景");
    let mut edit = saved_shot_input(&detached);
    edit.description = "独立镜头记录".into();
    assert!(super::shot::upsert(&mut db, &edit).is_ok());
    let project = input.project_id.unwrap();
    crate::projects::archive_project(&db, &project, true).unwrap();
    crate::projects::delete_project(&db, &project).unwrap();
    let detached = super::shot::get(&db, &s.asset.id).unwrap();
    assert!(detached.project_id.is_none());
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_relations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(super::prompt::get(&db, &p.asset.id).is_ok());
}
#[test]
fn shot_trash_does_not_delete_project_unit_or_references() {
    let (dir, mut db) = fixture();
    let input = shot_input(&db);
    let s = super::shot::upsert(&mut db, &input).unwrap();
    let p = super::prompt::upsert(&mut db, &prompt_input()).unwrap();
    relation(&db, &p.asset.id, &s.asset.id, "used_in");
    let b = super::entities::trash(&mut db, &[s.asset.id.clone()]).unwrap();
    assert!(super::shot::get(&db, &s.asset.id).unwrap().asset.deleted);
    assert_eq!(
        super::shot::list(&db, input.project_id.as_deref().unwrap(), 0, 200)
            .unwrap()
            .total,
        0
    );
    crate::db::purge_trash_batch(&mut db, dir.path(), &b.batch_id).unwrap();
    assert!(super::shot::get(&db, &s.asset.id).is_err());
    assert!(super::prompt::get(&db, &p.asset.id).is_ok());
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM project_units WHERE id=?1",
            [s.project_unit_id.unwrap()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn shot_bound_project_cannot_be_unlinked_but_extra_projects_can() {
    let (_, mut db) = fixture();
    let input = shot_input(&db);
    let shot = super::shot::upsert(&mut db, &input).unwrap();
    let project = input.project_id.unwrap();
    assert!(super::service::link_project(&mut db, &shot.asset.id, &project, false).is_err());
    let page = repository::search(
        &db,
        &serde_json::from_value(serde_json::json!({"projectId":project})).unwrap(),
    )
    .unwrap();
    assert!(page.items.iter().any(|a| a.id == shot.asset.id));
    assert_eq!(
        super::shot::get(&db, &shot.asset.id).unwrap().project_id,
        Some(project)
    );
    let extra = shot_input(&db).project_id.unwrap();
    super::service::link_project(&mut db, &shot.asset.id, &extra, true).unwrap();
    super::service::link_project(&mut db, &shot.asset.id, &extra, false).unwrap();
    assert!(repository::search(
        &db,
        &serde_json::from_value(serde_json::json!({"projectId":extra})).unwrap()
    )
    .unwrap()
    .items
    .is_empty());
}

#[test]
fn analysis_saved_validity_tracks_text_sources_versions_and_old_snapshots_offline() {
    let (dir, mut db) = fixture();
    let asset = media_asset(&db, dir.path(), "image");
    let preview = crate::ai::analyzer::prepare(
        &db,
        dir.path(),
        None,
        &asset,
        super::intelligence_models::AnalysisMode::Visual,
        &|| false,
    )
    .unwrap();
    let saved = super::analysis::complete_confirmed(
        &mut db,
        dir.path(),
        &super::intelligence_models::AnalysisRequest {
            asset_id: asset.clone(),
            preview_id: preview.id,
            force: false,
        },
        &analysis_answer().to_string(),
        Some(2),
        &|| false,
    )
    .unwrap();
    let validity = |db: &Connection| {
        serde_json::to_value(
            super::analysis::get_checked(db, dir.path(), &asset, Some(&saved.id), &|| false)
                .unwrap()
                .unwrap(),
        )
        .unwrap()["validity"]
            .as_str()
            .unwrap_or("")
            .to_owned()
    };
    assert_eq!(
        validity(&db),
        "current",
        "saved analysis must report its actual validity"
    );
    let owner = repository::read_asset(&db, &asset, "zh-CN")
        .unwrap()
        .origin
        .id;
    db.execute(
        "UPDATE media_entries SET name='新名称' WHERE id=?1",
        [&owner],
    )
    .unwrap();
    assert_eq!(validity(&db), "stale");
    db.execute(
        "UPDATE media_entries SET name='image' WHERE id=?1",
        [&owner],
    )
    .unwrap();
    super::service::set_tags(&mut db, &asset, "zh-CN", &["新标签".into()]).unwrap();
    assert_eq!(validity(&db), "stale");
    super::service::set_tags(&mut db, &asset, "zh-CN", &[]).unwrap();
    assert_eq!(validity(&db), "current");
    let file = repository::get(&db, dir.path(), &asset, "zh-CN")
        .unwrap()
        .files[0]
        .clone();
    let path = repository::file_path(&db, dir.path(), &file.source_ref).unwrap();
    image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 20, 30, 255]))
        .save(&path)
        .unwrap();
    assert_eq!(
        validity(&db),
        "stale",
        "replacement bytes cannot inherit current status"
    );
    assert_eq!(
        super::analysis::by_id(&db, &asset, &saved.id)
            .unwrap()
            .unwrap()
            .description,
        saved.description
    );
    assert!(
        super::analysis::get_checked(&db, dir.path(), &asset, Some(&saved.id), &|| true).is_err()
    );
    db.execute(
        "UPDATE asset_ai_analysis SET format_version=99 WHERE id=?1",
        [&saved.id],
    )
    .unwrap();
    assert_eq!(validity(&db), "stale");
    db.execute(
        "UPDATE asset_ai_analysis SET format_version=1 WHERE id=?1",
        [&saved.id],
    )
    .unwrap();
    db.execute_batch("ALTER TABLE asset_ai_analysis DROP COLUMN input_snapshot")
        .unwrap();
    super::migrate(&db).unwrap();
    assert_eq!(
        validity(&db),
        "unknown",
        "older snapshots remain visible but are not claimed current"
    );
}

fn media_asset(db: &Connection, base: &std::path::Path, kind: &str) -> String {
    let owner = uuid::Uuid::new_v4().to_string();
    let file = uuid::Uuid::new_v4().to_string();
    let ext = if kind == "image" { "png" } else { "mp4" };
    let relative = format!("media-library/{kind}/{owner}/original.{ext}");
    let absolute = base.join(&relative);
    std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    if kind == "image" {
        image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
            .save(&absolute)
            .unwrap();
    } else {
        std::fs::write(&absolute, b"database fixture video original").unwrap();
    }
    db.execute("INSERT INTO media_entries(id,kind,name,primary_file_id,created_at,updated_at) VALUES(?1,?2,?2,?3,'now','now')",params![owner,kind,file]).unwrap();
    db.execute("INSERT INTO media_files(id,entry_id,role,rel_path,logical_path,original_name,file_size,created_at) VALUES(?1,?2,'main',?3,?4,?4,?5,'now')",params![file,owner,relative,format!("original.{ext}"),std::fs::metadata(&absolute).unwrap().len()]).unwrap();
    let id = repository::register_origin(
        db,
        &super::models::CoreOrigin {
            kind: "media".into(),
            id: owner,
        },
    )
    .unwrap();
    repository::register_files(db, &id).unwrap();
    id
}
#[test]
fn acceptance_asset_network_backup_restores_identity_analysis_and_soft_delete() {
    let (dir, mut db) = fixture();
    let input = media_asset(&db, dir.path(), "image");
    let output = media_asset(&db, dir.path(), "video");
    let prompt = super::prompt::upsert(&mut db, &prompt_input()).unwrap();
    let shot_req = shot_input(&db);
    let shot = super::shot::upsert(&mut db, &shot_req).unwrap();
    relation(&db, &prompt.asset.id, &input, "generated_from");
    relation(&db, &output, &prompt.asset.id, "generated_from");
    relation(&db, &output, &shot.asset.id, "used_in");
    let preview = crate::ai::analyzer::prepare(
        &db,
        dir.path(),
        None,
        &input,
        super::intelligence_models::AnalysisMode::Text,
        &|| false,
    )
    .unwrap();
    let analysis = super::analysis::complete_confirmed(
        &mut db,
        dir.path(),
        &super::intelligence_models::AnalysisRequest {
            asset_id: input.clone(),
            preview_id: preview.id,
            force: false,
        },
        &analysis_answer().to_string(),
        Some(123),
        &|| false,
    )
    .unwrap();
    let batch = super::entities::trash(&mut db, &[prompt.asset.id.clone()]).unwrap();
    let original = repository::get(&db, dir.path(), &input, "zh-CN")
        .unwrap()
        .files[0]
        .clone();
    let archive = dir.path().join("network.dccassetlib");
    crate::backup::export_library(&db, dir.path(), &archive).unwrap();
    db.execute("DELETE FROM asset_relations", []).unwrap();
    db.execute("DELETE FROM asset_ai_analysis", []).unwrap();
    let mut active = crate::state::ActiveLibrary {
        base_dir: dir.path().to_owned(),
        db_path: dir.path().join("library.db"),
        marker: crate::state::LibraryMarker {
            format_version: 1,
            library_id: "acceptance".into(),
            name: "验收".into(),
            created_at: "now".into(),
        },
        connection: Some(db),
    };
    crate::backup::restore_library(&mut active, &archive).unwrap();
    let db = active.connection.as_mut().unwrap();
    assert_eq!(
        db.query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "42"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM asset_relations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        super::analysis::latest(db, &input).unwrap().unwrap().id,
        analysis.id
    );
    assert_eq!(
        super::shot::get(db, &shot.asset.id)
            .unwrap()
            .project_unit_id,
        shot.project_unit_id
    );
    assert!(
        super::prompt::get(db, &prompt.asset.id)
            .unwrap()
            .asset
            .deleted
    );
    crate::db::restore_trash_batch(db, &batch.batch_id).unwrap();
    assert_eq!(
        super::prompt::get(db, &prompt.asset.id).unwrap().content,
        prompt.content
    );
    let restored = repository::get(db, dir.path(), &input, "zh-CN")
        .unwrap()
        .files;
    assert_eq!(restored[0].id, original.id);
    assert!(restored[0].available);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}
#[test]
fn acceptance_10001_assets_keeps_search_and_high_degree_graph_bounded() {
    let (_, mut db) = fixture();
    let tx = db.transaction().unwrap();
    let center = asset(&tx, "中心");
    for i in 0..10000 {
        let id = asset(&tx, &format!("资产 {i}"));
        tx.execute("INSERT INTO asset_relations(id,source_asset_id,target_asset_id,relation_type,created_at) VALUES(?1,?2,?3,'inspired_by','now')",params![uuid::Uuid::new_v4().to_string(),center,id]).unwrap();
    }
    tx.commit().unwrap();
    let request = serde_json::from_value(serde_json::json!({"limit":10001})).unwrap();
    let start = std::time::Instant::now();
    let page = repository::search(&db, &request).unwrap();
    let search = start.elapsed();
    assert_eq!(page.total, 10001);
    assert_eq!(page.items.len(), 200);
    assert_eq!(page.limit, 200);
    let start = std::time::Instant::now();
    let graph = super::relations::graph(
        &db,
        &super::intelligence_models::GraphRequest {
            asset_id: center,
            locale: "zh-CN".into(),
            offset: 9920,
            limit: 10001,
        },
    )
    .unwrap();
    let graph_time = start.elapsed();
    assert_eq!(graph.total, 10000);
    assert_eq!(graph.nodes.len(), 81);
    assert_eq!(graph.limit, 80);
    assert_eq!(graph.relations.len(), 80);
    eprintln!("v0.23 benchmark: assets=10001, search={search:?}, graph80={graph_time:?}");
}
