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
