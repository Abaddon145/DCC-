use super::{entities, intelligence_models::*, repository};
use rusqlite::{params, Connection};
pub fn upsert(db: &mut Connection, input: &PromptInput) -> Result<PromptAsset, String> {
    entities::validate(&input.name, &input.description)?;
    if input.content.chars().count() > 100000
        || input.model.chars().count() > 200
        || !input.parameters.is_object()
        || input.parameters.to_string().len() > 16384
    {
        return Err("Prompt 内容、模型或 JSON 对象参数超出限制".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let id = if let Some(id) = &input.asset_id {
        get(&tx, id)?;
        entities::update(
            &tx,
            id,
            input.expected_revision,
            &input.name,
            &input.description,
        )?;
        id.clone()
    } else {
        if input.expected_revision.is_some() {
            return Err("新资产不能携带旧修订号".into());
        }
        entities::create(&tx, "prompt", &input.name, &input.description)?
    };
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute("INSERT INTO prompts(asset_id,content,model,parameters,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) ON CONFLICT(asset_id) DO UPDATE SET content=excluded.content,model=excluded.model,parameters=excluded.parameters,updated_at=excluded.updated_at",params![id,input.content,input.model,input.parameters.to_string(),now]).map_err(|e|e.to_string())?;
    let result = get(&tx, &id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn get(db: &Connection, id: &str) -> Result<PromptAsset, String> {
    let asset = repository::read_asset(db, id, "zh-CN")?;
    if asset.asset_type != "prompt" {
        return Err("此资产不是 Prompt".into());
    }
    db.query_row("SELECT p.content,p.model,p.parameters,e.revision,p.created_at,p.updated_at FROM prompts p JOIN asset_core c ON c.id=p.asset_id JOIN asset_entities e ON e.id=c.entity_id WHERE p.asset_id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(|e|e.to_string()).and_then(|(content,model,parameters,revision,created_at,updated_at)|Ok(PromptAsset{asset,content,model,parameters:serde_json::from_str(&parameters).map_err(|e|e.to_string())?,revision,created_at,updated_at}))
}
