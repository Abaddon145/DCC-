use super::{intelligence_models::*, service};
use crate::ai::analyzer::PreparedAnalysis;
use rusqlite::{params, Connection, OptionalExtension, Row};
pub const FORMAT_VERSION: i64 = 1;
const COLUMNS:&str="id,asset_id,description,keywords,scene,style,camera,lighting,mood,model,mode,input_fingerprint,format_version,created_at,actual_tokens";
fn read(r: &Row<'_>) -> rusqlite::Result<AssetAnalysis> {
    let json: String = r.get(3)?;
    let mode: String = r.get(10)?;
    let parse_error =
        |e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e));
    Ok(AssetAnalysis {
        id: r.get(0)?,
        asset_id: r.get(1)?,
        description: r.get(2)?,
        keywords: serde_json::from_str(&json).map_err(parse_error)?,
        scene: r.get(4)?,
        style: r.get(5)?,
        camera: r.get(6)?,
        lighting: r.get(7)?,
        mood: r.get(8)?,
        model: r.get(9)?,
        mode: if mode == "visual" {
            AnalysisMode::Visual
        } else {
            AnalysisMode::Text
        },
        input_fingerprint: r.get(11)?,
        format_version: r.get(12)?,
        created_at: r.get(13)?,
        actual_tokens: r.get(14)?,
    })
}
pub fn latest(db: &Connection, asset_id: &str) -> Result<Option<AssetAnalysis>, String> {
    service::active_origin(db, asset_id)?;
    db.query_row(&format!("SELECT {COLUMNS} FROM asset_ai_analysis WHERE asset_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1"),[asset_id],read).optional().map_err(|e|e.to_string())
}
pub fn cached(
    db: &Connection,
    asset_id: &str,
    fingerprint: &str,
    mode: AnalysisMode,
) -> Result<Option<AssetAnalysis>, String> {
    service::active_origin(db, asset_id)?;
    db.query_row(&format!("SELECT {COLUMNS} FROM asset_ai_analysis WHERE asset_id=?1 AND input_fingerprint=?2 AND model=?3 AND mode=?4 AND format_version=?5 ORDER BY created_at DESC,id DESC LIMIT 1"),params![asset_id,fingerprint,crate::translation_service::MODEL,mode.as_str(),FORMAT_VERSION],read).optional().map_err(|e|e.to_string())
}
pub fn validate_answer(answer: &AnalysisAnswer) -> Result<(), String> {
    let text = [
        &answer.description,
        &answer.scene,
        &answer.style,
        &answer.camera,
        &answer.lighting,
        &answer.mood,
    ];
    if text.iter().any(|s| s.chars().count() > 8000)
        || answer.keywords.len() > 100
        || answer
            .keywords
            .iter()
            .any(|s| s.trim().is_empty() || s.chars().count() > 100)
        || text.iter().all(|s| s.trim().is_empty()) && answer.keywords.is_empty()
    {
        return Err("分析内容为空或超出字段限制".into());
    }
    Ok(())
}
pub fn save(
    db: &Connection,
    input: &PreparedAnalysis,
    answer: &AnalysisAnswer,
    tokens: Option<u64>,
) -> Result<AssetAnalysis, String> {
    service::active_origin(db, &input.preview.asset_id)?;
    validate_answer(answer)?;
    if tokens.is_some_and(|v| v > i64::MAX as u64) {
        return Err("分析用量无效".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let mut seen = std::collections::HashSet::new();
    let keywords: Vec<_> = answer
        .keywords
        .iter()
        .map(|s| s.trim())
        .filter(|s| seen.insert(s.to_lowercase()))
        .collect();
    db.execute("INSERT INTO asset_ai_analysis(id,asset_id,description,keywords,scene,style,camera,lighting,mood,model,mode,input_fingerprint,format_version,created_at,actual_tokens) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![id,input.preview.asset_id,answer.description,serde_json::to_string(&keywords).map_err(|e|e.to_string())?,answer.scene,answer.style,answer.camera,answer.lighting,answer.mood,crate::translation_service::MODEL,input.preview.mode.as_str(),input.preview.fingerprint,FORMAT_VERSION,chrono::Utc::now().to_rfc3339(),tokens]).map_err(|e|e.to_string())?;
    db.query_row(
        &format!("SELECT {COLUMNS} FROM asset_ai_analysis WHERE id=?1"),
        [id],
        read,
    )
    .map_err(|e| e.to_string())
}
