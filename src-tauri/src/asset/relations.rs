use super::{intelligence_models::*, repository, service};
use rusqlite::{params, Connection, Row};

fn id(raw: &str) -> Result<String, String> {
    uuid::Uuid::parse_str(raw)
        .map(|u| u.to_string())
        .map_err(|_| "资产或关系标识无效".into())
}
fn read(row: &Row<'_>) -> rusqlite::Result<AssetRelation> {
    Ok(AssetRelation {
        id: row.get(0)?,
        source_asset_id: row.get(1)?,
        target_asset_id: row.get(2)?,
        relation_type: row.get(3)?,
        created_at: row.get(4)?,
    })
}
const COLUMNS: &str = "id,source_asset_id,target_asset_id,relation_type,created_at";

pub fn create(db: &Connection, input: &RelationInput) -> Result<AssetRelation, String> {
    let mut source = id(&input.source_asset_id)?;
    let mut target = id(&input.target_asset_id)?;
    if source == target {
        return Err("不能关联资产自身".into());
    }
    if !matches!(
        input.relation_type.as_str(),
        "inspired_by"
            | "generated_from"
            | "derived_from"
            | "used_in"
            | "reference_of"
            | "variant_of"
            | "related_to"
    ) {
        return Err("未知资产关系类型".into());
    }
    if matches!(input.relation_type.as_str(), "variant_of" | "related_to") && source > target {
        std::mem::swap(&mut source, &mut target);
    }
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    service::active_origin(&tx, &source)?;
    service::active_origin(&tx, &target)?;
    tx.execute("INSERT INTO asset_relations(id,source_asset_id,target_asset_id,relation_type,created_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(source_asset_id,target_asset_id,relation_type) DO NOTHING",
        params![uuid::Uuid::new_v4().to_string(),source,target,input.relation_type,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
    let result=tx.query_row(&format!("SELECT {COLUMNS} FROM asset_relations WHERE source_asset_id=?1 AND target_asset_id=?2 AND relation_type=?3"),params![source,target,input.relation_type],read).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn delete(db: &Connection, asset_id: &str, relation_id: &str) -> Result<(), String> {
    let asset_id = id(asset_id)?;
    let relation_id = id(relation_id)?;
    let changed=db.execute("DELETE FROM asset_relations WHERE id=?1 AND (source_asset_id=?2 OR target_asset_id=?2)",params![relation_id,asset_id]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("关系不存在或不属于指定资产".into());
    }
    Ok(())
}
pub fn graph(db: &Connection, request: &GraphRequest) -> Result<AssetGraph, String> {
    let center_id = id(&request.asset_id)?;
    if !matches!(request.locale.as_str(), "zh-CN" | "en") || request.offset > i64::MAX as usize {
        return Err("关系图查询参数无效".into());
    }
    let center = repository::read_asset(db, &center_id, &request.locale)?;
    let limit = request.limit.clamp(1, 80);
    let neighbors="SELECT target_asset_id AS id FROM asset_relations WHERE source_asset_id=?1 UNION SELECT source_asset_id FROM asset_relations WHERE target_asset_id=?1";
    let total = db
        .query_row(
            &format!("SELECT count(*) FROM ({neighbors})"),
            [&center_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut stmt = db
        .prepare(&format!(
            "SELECT id FROM ({neighbors}) ORDER BY id LIMIT ?2 OFFSET ?3"
        ))
        .map_err(|e| e.to_string())?;
    let ids = stmt
        .query_map(
            params![center_id, limit as i64, request.offset as i64],
            |r| r.get::<_, String>(0),
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut nodes = vec![center.clone()];
    let mut relations = vec![];
    let mut edges=db.prepare(&format!("SELECT {COLUMNS} FROM asset_relations WHERE (source_asset_id=?1 AND target_asset_id=?2) OR (source_asset_id=?2 AND target_asset_id=?1) ORDER BY created_at,id")).map_err(|e|e.to_string())?;
    for neighbor in ids {
        nodes.push(repository::read_asset(db, &neighbor, &request.locale)?);
        relations.extend(
            edges
                .query_map(params![center_id, neighbor], read)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(AssetGraph {
        center,
        nodes,
        relations,
        total,
        offset: request.offset,
        limit,
    })
}
