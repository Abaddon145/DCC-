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
