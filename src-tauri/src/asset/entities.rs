use super::{models::CoreOrigin, repository, service};
use rusqlite::{params, Connection};
pub fn validate(name: &str, description: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > 200 || description.chars().count() > 10000 {
        Err("名称为 1–200 字，描述最多 10000 字".into())
    } else {
        Ok(())
    }
}
pub fn create(
    db: &Connection,
    kind: &str,
    name: &str,
    description: &str,
) -> Result<String, String> {
    validate(name, description)?;
    if !matches!(kind, "prompt" | "shot") {
        return Err("未知文本资产类型".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    db.execute("INSERT INTO asset_entities(id,kind,name,description,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",params![id,kind,name.trim(),description,now]).map_err(|e|e.to_string())?;
    repository::register_origin(
        db,
        &CoreOrigin {
            kind: "entity".into(),
            id,
        },
    )
}
pub fn update(
    db: &Connection,
    core_id: &str,
    revision: Option<i64>,
    name: &str,
    description: &str,
) -> Result<(), String> {
    validate(name, description)?;
    let origin = service::active_origin(db, core_id)?;
    if origin.kind != "entity" {
        return Err("此资产不是文本资产".into());
    }
    if db.execute("UPDATE asset_entities SET name=?1,description=?2,revision=revision+1,updated_at=?3 WHERE id=?4 AND revision=?5",params![name.trim(),description,chrono::Utc::now().to_rfc3339(),origin.id,revision]).map_err(|e|e.to_string())?==0{return Err("资产已被修改，请重新读取后保存；当前草稿仍可复制".into());}
    Ok(())
}
pub fn trash(db: &mut Connection, ids: &[String]) -> Result<crate::models::DeleteResult, String> {
    if ids.is_empty() || ids.len() > 500 {
        return Err("请选择 1–500 项文本资产".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let mut owners = std::collections::HashSet::new();
    for id in ids {
        let origin = service::active_origin(&tx, id)?;
        if origin.kind != "entity" {
            return Err("只可从此入口删除文本资产".into());
        }
        owners.insert(origin.id);
    }
    let label = format!("{} 项文本资产", owners.len());
    tx.execute("INSERT INTO deletion_batches(id,kind,label,asset_count,category_count,created_at) VALUES(?1,'assets',?2,?3,0,?4)",params![batch_id,label,owners.len(),now]).map_err(|e|e.to_string())?;
    for id in &owners {
        tx.execute("UPDATE asset_entities SET deleted_at=?1,delete_batch_id=?2,revision=revision+1 WHERE id=?3",params![now,batch_id,id]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(crate::models::DeleteResult {
        batch_id,
        label,
        asset_count: owners.len(),
        category_count: 0,
        media_count: 0,
        media_folder_count: 0,
    })
}
