use crate::state::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTemplate {
    pub id: String,
    pub name: String,
    pub focus: String,
    pub version: i64,
}

pub(crate) fn get(db: &Connection, id: &str) -> Result<CustomTemplate, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "自定义模板标识无效")?;
    db.query_row(
        "SELECT id,name,focus,version FROM learning_analysis_templates WHERE id=?1",
        [id],
        |r| {
            Ok(CustomTemplate {
                id: r.get(0)?,
                name: r.get(1)?,
                focus: r.get(2)?,
                version: r.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "自定义模板不存在".into())
}

pub fn list(state: &AppState, library: &str) -> Result<Vec<CustomTemplate>, String> {
    state.with_library_if_id(library, |db,_| {
        let mut query = db.prepare("SELECT id,name,focus,version FROM learning_analysis_templates ORDER BY name,id LIMIT 100").map_err(|e| e.to_string())?;
        let rows = query.query_map([], |r| Ok(CustomTemplate { id:r.get(0)?, name:r.get(1)?, focus:r.get(2)?, version:r.get(3)? })).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        Ok(rows)
    })?.ok_or_else(|| "素材库已切换".into())
}

pub fn save(
    state: &AppState,
    library: &str,
    id: Option<&str>,
    version: Option<i64>,
    name: &str,
    focus: &str,
) -> Result<CustomTemplate, String> {
    let name = name.trim();
    let focus = focus.trim();
    if name.is_empty() || name.chars().count() > 80 || focus.is_empty() || focus.len() > 4000 {
        return Err("模板名称或分析要求无效".into());
    }
    state.with_library_mut_if_id(library, |db,_| {
        let tx=db.transaction().map_err(|e|e.to_string())?;
        let id = if let Some(id)=id {
            uuid::Uuid::parse_str(id).map_err(|_| "自定义模板标识无效")?;
            let changed=tx.execute("UPDATE learning_analysis_templates SET name=?1,focus=?2,version=version+1,updated_at=?3 WHERE id=?4 AND version=?5",params![name,focus,chrono::Utc::now().to_rfc3339(),id,version]).map_err(|e|e.to_string())?;
            if changed!=1 { return Err("模板已变化或删除，请重新读取".into()); }
            id.to_string()
        } else {
            if version.is_some() { return Err("新模板版本无效".into()); }
            let count:i64=tx.query_row("SELECT count(*) FROM learning_analysis_templates",[],|r|r.get(0)).map_err(|e|e.to_string())?;
            if count>=100 { return Err("最多保存 100 个自定义模板".into()); }
            let id=uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO learning_analysis_templates(id,name,focus,version,updated_at) VALUES(?1,?2,?3,1,?4)",params![id,name,focus,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
            id
        };
        let result=get(&tx,&id)?; tx.commit().map_err(|e|e.to_string())?; Ok(result)
    })?.ok_or_else(|| "素材库已切换".into())
}

pub fn remove(state: &AppState, library: &str, id: &str, version: i64) -> Result<(), String> {
    uuid::Uuid::parse_str(id).map_err(|_| "自定义模板标识无效")?;
    state
        .with_library_if_id(library, |db, _| {
            if db
                .execute(
                    "DELETE FROM learning_analysis_templates WHERE id=?1 AND version=?2",
                    params![id, version],
                )
                .map_err(|e| e.to_string())?
                != 1
            {
                return Err("模板已变化或删除，请重新读取".into());
            }
            Ok(())
        })?
        .ok_or_else(|| "素材库已切换".into())
}
