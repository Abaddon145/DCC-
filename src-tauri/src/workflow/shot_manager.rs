use crate::{asset::intelligence_models::ShotInput, models::ProjectUnitInput, projects};
use rusqlite::Connection;
pub fn bind_or_create(db: &Connection, input: &ShotInput) -> Result<Option<String>, String> {
    if db.is_autocommit() {
        return Err("镜头登记必须在事务内执行".into());
    }
    let project = input.project_id.as_deref().ok_or("请先选择项目")?;
    if let Some(id) = &input.project_unit_id {
        let unit = projects::get_project_unit(db, id)?;
        if unit.kind != "shot" || unit.project_id != project {
            return Err("所选镜头不属于此项目".into());
        }
        return Ok(Some(id.clone()));
    }
    let parent = input.parent_scene_id.clone().ok_or("请先选择父场景")?;
    let unit = projects::upsert_project_unit(
        db,
        ProjectUnitInput {
            id: None,
            project_id: project.into(),
            parent_id: Some(parent),
            kind: "shot".into(),
            name: input.name.clone(),
            description: input.description.clone(),
            start_frame: Some(1),
            end_frame: Some(100),
            resolution_width: None,
            resolution_height: None,
            frame_rate: None,
            sort_order: None,
        },
    )?;
    Ok(Some(unit.id))
}
