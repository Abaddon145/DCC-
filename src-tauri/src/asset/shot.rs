use super::{entities, intelligence_models::*, models::Page, repository, service};
use rusqlite::{params, Connection, OptionalExtension};
pub fn upsert(db: &mut Connection, input: &ShotInput) -> Result<ShotAsset, String> {
    entities::validate(&input.name, &input.description)?;
    if input.act.chars().count() > 80
        || input.shot_number.chars().count() > 80
        || !matches!(
            input.status.as_str(),
            "todo" | "in_progress" | "review" | "done"
        )
    {
        return Err("ACT、镜头编号或状态无效".into());
    }
    for id in [
        &input.asset_id,
        &input.project_id,
        &input.project_unit_id,
        &input.parent_scene_id,
    ]
    .into_iter()
    .flatten()
    {
        uuid::Uuid::parse_str(id).map_err(|_| "项目或镜头标识无效".to_string())?;
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let id = if let Some(id) = &input.asset_id {
        let previous = get(&tx, id)?;
        service::active_origin(&tx, id)?;
        if Some(previous.revision) != input.expected_revision {
            return Err("资产已被修改，请重新读取；当前草稿仍可复制".into());
        }
        if input.project_id != previous.project_id
            || input.project_unit_id != previous.project_unit_id
        {
            return Err("镜头绑定已变化，请重新读取".into());
        }
        if let Some(unit) = previous.unit {
            let written = crate::projects::upsert_project_unit(
                &tx,
                crate::models::ProjectUnitInput {
                    id: Some(unit.id),
                    project_id: unit.project_id,
                    parent_id: unit.parent_id,
                    kind: "shot".into(),
                    name: input.name.clone(),
                    description: input.description.clone(),
                    start_frame: unit.start_frame,
                    end_frame: unit.end_frame,
                    resolution_width: unit.resolution_width,
                    resolution_height: unit.resolution_height,
                    frame_rate: unit.frame_rate,
                    sort_order: Some(unit.sort_order),
                },
            )?;
            // The shared unit owns bound text; status-only edits still advance revision once.
            if get(&tx, id)?.revision == previous.revision {
                entities::update(
                    &tx,
                    id,
                    input.expected_revision,
                    &written.name,
                    &written.description,
                )?;
            }
        } else {
            entities::update(
                &tx,
                id,
                input.expected_revision,
                &input.name,
                &input.description,
            )?;
        }
        tx.execute(
            "UPDATE shots SET act=?1,shot_number=?2,status=?3,updated_at=?4 WHERE asset_id=?5",
            params![
                input.act,
                input.shot_number,
                input.status,
                chrono::Utc::now().to_rfc3339(),
                id
            ],
        )
        .map_err(|e| e.to_string())?;
        id.clone()
    } else {
        if input.expected_revision.is_some() {
            return Err("新资产不能携带旧修订号".into());
        }
        let unit_id =
            crate::workflow::shot_manager::bind_or_create(&tx, input)?.ok_or("镜头不存在")?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT asset_id FROM shots WHERE project_unit_id=?1",
                [&unit_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(id) = existing {
            let saved = get(&tx, &id)?;
            if saved.asset.deleted {
                return Err("此镜头资产已在回收站，请先恢复".into());
            }
            return Ok(saved);
        }
        let unit = crate::projects::get_project_unit(&tx, &unit_id)?;
        let id = entities::create(&tx, "shot", &unit.name, &unit.description)?;
        let origin = repository::origin(&tx, &id)?;
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute("INSERT INTO shots(asset_id,project_id,project_unit_id,act,shot_number,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![id,unit.project_id,unit_id,input.act,input.shot_number,input.status,now]).map_err(|e|e.to_string())?;
        tx.execute(
            "INSERT INTO asset_entity_projects(entity_id,project_id) VALUES(?1,?2)",
            params![origin.id, unit.project_id],
        )
        .map_err(|e| e.to_string())?;
        id
    };
    let saved = get(&tx, &id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(saved)
}
pub fn get(db: &Connection, id: &str) -> Result<ShotAsset, String> {
    let asset = repository::read_asset(db, id, "zh-CN")?;
    if asset.asset_type != "shot" {
        return Err("此资产不是 Shot".into());
    }
    let (revision,project_id,project_unit_id,act,shot_number,status,created_at,updated_at)=db.query_row("SELECT e.revision,s.project_id,s.project_unit_id,s.act,s.shot_number,s.status,s.created_at,e.updated_at FROM shots s JOIN asset_core c ON c.id=s.asset_id JOIN asset_entities e ON e.id=c.entity_id WHERE s.asset_id=?1",[id],|r|Ok((r.get(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).map_err(|e|e.to_string())?;
    let unit = project_unit_id
        .as_deref()
        .map(|id| crate::projects::get_project_unit(db, id))
        .transpose()?;
    Ok(ShotAsset {
        asset,
        revision,
        project_id,
        project_unit_id,
        act,
        shot_number,
        status,
        unit,
        created_at,
        updated_at,
    })
}
pub fn list(
    db: &Connection,
    project: &str,
    offset: usize,
    limit: usize,
) -> Result<Page<ShotAsset>, String> {
    let limit = limit.clamp(1, 200);
    let offset = offset.min(i64::MAX as usize);
    let total=db.query_row("SELECT count(*) FROM shots s JOIN asset_core c ON c.id=s.asset_id JOIN asset_entities e ON e.id=c.entity_id WHERE s.project_id=?1 AND e.deleted_at IS NULL",[project],|r|r.get(0)).map_err(|e|e.to_string())?;
    let mut statement=db.prepare("SELECT s.asset_id FROM shots s JOIN asset_core c ON c.id=s.asset_id JOIN asset_entities e ON e.id=c.entity_id WHERE s.project_id=?1 AND e.deleted_at IS NULL ORDER BY s.created_at,s.asset_id LIMIT ?2 OFFSET ?3").map_err(|e|e.to_string())?;
    let ids = statement
        .query_map(params![project, limit, offset], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let items = ids
        .iter()
        .map(|id| get(db, id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Page {
        items,
        total,
        offset,
        limit,
    })
}
