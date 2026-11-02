use super::{intelligence_models::*, service};
use crate::ai::analyzer::PreparedAnalysis;
use rusqlite::{params, Connection, OptionalExtension, Row};
pub const FORMAT_VERSION: i64 = 1;
pub fn get_checked(
    db: &Connection,
    base: &std::path::Path,
    asset_id: &str,
    analysis_id: Option<&str>,
    stop: &dyn Fn() -> bool,
) -> Result<Option<AssetAnalysis>, String> {
    if stop() {
        return Err("分析核验已停止".into());
    }
    let Some(mut saved) = (if let Some(id) = analysis_id {
        by_id(db, asset_id, id)
    } else {
        latest(db, asset_id)
    })?
    else {
        return Ok(None);
    };
    let snapshot: String = db
        .query_row(
            "SELECT input_snapshot FROM asset_ai_analysis WHERE id=?1",
            [&saved.id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if snapshot.is_empty() {
        saved.validity_message = "旧结果没有核验快照，请重新预览资料".into();
    } else if saved.model != crate::translation_service::MODEL
        || saved.format_version != FORMAT_VERSION
    {
        saved.validity = "stale".into();
        saved.validity_message = "模型或分析格式版本已变化".into();
    } else {
        match crate::ai::analyzer::validate_saved_snapshot(db, base, asset_id, &snapshot, stop) {
            Ok(()) => {
                saved.validity = "current".into();
                saved.validity_message = "与当前资料一致".into();
            }
            Err(reason) => {
                saved.validity = "stale".into();
                saved.validity_message = reason;
            }
        }
    }
    if stop() {
        return Err("分析核验已停止".into());
    }
    Ok(Some(saved))
}
pub fn by_id(
    db: &Connection,
    asset_id: &str,
    analysis_id: &str,
) -> Result<Option<AssetAnalysis>, String> {
    service::active_origin(db, asset_id)?;
    uuid::Uuid::parse_str(analysis_id).map_err(|_| "分析结果标识无效")?;
    db.query_row(
        &format!("SELECT {COLUMNS} FROM asset_ai_analysis WHERE asset_id=?1 AND id=?2"),
        params![asset_id, analysis_id],
        read,
    )
    .optional()
    .map_err(|e| e.to_string())
}
pub fn complete_confirmed(
    db: &mut Connection,
    base: &std::path::Path,
    request: &AnalysisRequest,
    raw: &str,
    tokens: Option<u64>,
    stop: &dyn Fn() -> bool,
) -> Result<AssetAnalysis, String> {
    let input = crate::ai::analyzer::load_confirmed_controlled(db, base, request, stop)?;
    commit_answer(db, &input, raw, tokens, stop)
}
fn commit_answer(
    db: &mut Connection,
    input: &PreparedAnalysis,
    raw: &str,
    tokens: Option<u64>,
    stop: &dyn Fn() -> bool,
) -> Result<AssetAnalysis, String> {
    let answer = crate::ai::analyzer::parse_answer(raw)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    crate::ai::analyzer::validate_text(&tx, input)?;
    if stop() {
        return Err("分析已停止，结果未写入".into());
    }
    let result = save(&tx, input, &answer, tokens)?;
    if stop() {
        return Err("分析已停止，结果未写入".into());
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn apply_keywords(
    db: &mut Connection,
    asset_id: &str,
    analysis_id: &str,
    locale: &str,
    selected: &[String],
) -> Result<(), String> {
    uuid::Uuid::parse_str(analysis_id).map_err(|_| "分析结果标识无效")?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    service::active_origin(&tx, asset_id)?;
    let saved = tx
        .query_row(
            &format!("SELECT {COLUMNS} FROM asset_ai_analysis WHERE id=?1 AND asset_id=?2"),
            params![analysis_id, asset_id],
            read,
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("分析结果不属于当前资产")?;
    if selected.len() > 100
        || selected
            .iter()
            .any(|s| !saved.keywords.iter().any(|t| t == s.trim()))
    {
        return Err("仅可追加当前分析建议中的标签".into());
    }
    let mut tags = super::repository::read_asset(&tx, asset_id, locale)?.tags;
    let mut seen: std::collections::HashSet<_> =
        tags.iter().map(|t| t.trim().to_lowercase()).collect();
    for tag in selected.iter().map(|s| s.trim()) {
        if seen.insert(tag.to_lowercase()) {
            tags.push(tag.into());
        }
    }
    service::set_tags_in_tx(&tx, asset_id, locale, &tags)?;
    tx.commit().map_err(|e| e.to_string())
}

pub async fn run(
    app: tauri::AppHandle,
    task: crate::tasks::LibraryTask,
    request: AnalysisRequest,
) -> Result<serde_json::Value, String> {
    use std::sync::atomic::Ordering;
    use tauri::Manager;
    let prepare_app = app.clone();
    let prepare_task = task.clone();
    let prepare_request = request.clone();
    let (input, cached) = tauri::async_runtime::spawn_blocking(move || {
        let state = prepare_app.state::<crate::state::AppState>();
        let base = state
            .with_library_if_id(&prepare_task.library_id, |_, base| Ok(base.to_owned()))?
            .ok_or("素材库已切换")?;
        let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
        let stop = || {
            state.tasks.should_stop(&prepare_task.id)
                || state.active_library_id().ok().as_deref()
                    != Some(prepare_task.library_id.as_str())
        };
        let input =
            crate::ai::analyzer::load_confirmed_controlled(&db, &base, &prepare_request, &stop)?;
        let cached = if prepare_request.force {
            None
        } else {
            cached(
                &db,
                &prepare_request.asset_id,
                &input.preview.fingerprint,
                input.preview.mode,
            )?
        };
        if stop() {
            return Err("分析已停止".into());
        }
        Ok::<_, String>((input, cached))
    })
    .await
    .map_err(|_| "分析资料核验中断")??;
    let state = app.state::<crate::state::AppState>();
    if let Some(cached) = cached {
        return state
            .with_library_if_id(&task.library_id, |db, _| {
                crate::ai::analyzer::validate_text(db, &input)?;
                if state.tasks.should_stop(&task.id) {
                    return Err("分析已停止".into());
                }
                Ok(task_result(&cached, true))
            })?
            .ok_or("素材库已切换，未返回旧库分析".into());
    }
    let (_lease, cancel) = crate::ai::acquire_request(&task.id)?;
    if state.tasks.should_stop(&task.id) || state.active_library_id()? != task.library_id {
        return Err("分析已停止或素材库已切换".into());
    }
    state
        .tasks
        .update(&task.id, 0, 1, String::new(), "analyzing".into(), None);
    let body = crate::ai::analyzer::request_body(&input);
    let watcher = async {
        loop {
            if state.tasks.should_stop(&task.id)
                || state.active_library_id().ok().as_deref() != Some(task.library_id.as_str())
            {
                cancel.store(true, Ordering::SeqCst);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }
    };
    let network = state.translation_service.creative_analysis_json_with_usage(
        &body,
        state.translation_preferences()?.deepseek_monthly_limit,
        &cancel,
        input.preview.images.len(),
    );
    let reply = tokio::select! { reply=network=>reply?, _=watcher=>return Err("分析已停止或素材库已切换；已发送请求可能产生用量".into()) };
    let raw = reply.content?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::state::AppState>();
        let base = state
            .with_library_if_id(&task.library_id, |_, base| Ok(base.to_owned()))?
            .ok_or("素材库已切换")?;
        let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
        let stop = || {
            cancel.load(Ordering::SeqCst)
                || state.tasks.should_stop(&task.id)
                || state.active_library_id().ok().as_deref() != Some(task.library_id.as_str())
        };
        let confirmed =
            crate::ai::analyzer::load_confirmed_controlled(&db, &base, &request, &stop)?;
        if confirmed.preview.fingerprint != input.preview.fingerprint {
            return Err("资料已改变，结果未写入".into());
        }
        state
            .with_library_mut_if_id(&task.library_id, |db, _| {
                let stopped = || cancel.load(Ordering::SeqCst) || state.tasks.should_stop(&task.id);
                let saved = commit_answer(db, &confirmed, &raw, reply.used, &stopped)?;
                Ok(task_result(&saved, false))
            })?
            .ok_or("素材库已切换，结果未写入".into())
    })
    .await
    .map_err(|_| "分析结果保存中断")?
}
fn task_result(saved: &AssetAnalysis, cached: bool) -> serde_json::Value {
    serde_json::json!({"assetId":saved.asset_id,"analysisId":saved.id,"cached":cached,"rows":[{"assetId":saved.asset_id,"status":"completed","message":if cached {"复用本地分析，未请求在线服务"} else {"分析已保存；建议标签需勾选后追加"}}]})
}
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
        validity: "unknown".into(),
        validity_message: "尚未核验当前资料".into(),
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
    db.execute("INSERT INTO asset_ai_analysis(id,asset_id,description,keywords,scene,style,camera,lighting,mood,model,mode,input_fingerprint,format_version,created_at,actual_tokens,input_snapshot) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",params![id,input.preview.asset_id,answer.description,serde_json::to_string(&keywords).map_err(|e|e.to_string())?,answer.scene,answer.style,answer.camera,answer.lighting,answer.mood,crate::translation_service::MODEL,input.preview.mode.as_str(),input.preview.fingerprint,FORMAT_VERSION,chrono::Utc::now().to_rfc3339(),tokens,crate::ai::analyzer::saved_snapshot(input)?]).map_err(|e|e.to_string())?;
    db.query_row(
        &format!("SELECT {COLUMNS} FROM asset_ai_analysis WHERE id=?1"),
        [id],
        read,
    )
    .map_err(|e| e.to_string())
}
