use super::{models::*, repository, scanner, service};
use crate::state::AppState;
use tauri::{Manager, State};
fn uuid(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "资产或文件标识无效".into())
}
fn current<T>(result: Result<Option<T>, String>) -> Result<T, String> {
    result?.ok_or("素材库已切换，未读取或修改旧库".into())
}
#[tauri::command]
pub fn upsert_prompt_asset(
    state: State<AppState>,
    expected_library_id: String,
    input: super::intelligence_models::PromptInput,
) -> Result<super::intelligence_models::PromptAsset, String> {
    current(state.with_library_mut_if_id(&expected_library_id, |db, _| {
        super::prompt::upsert(db, &input)
    }))
}
#[tauri::command]
pub fn get_prompt_asset(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
) -> Result<super::intelligence_models::PromptAsset, String> {
    uuid(&asset_id)?;
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        super::prompt::get(db, &asset_id)
    }))
}
#[tauri::command]
pub fn trash_text_assets(
    state: State<AppState>,
    expected_library_id: String,
    asset_ids: Vec<String>,
) -> Result<crate::models::DeleteResult, String> {
    current(state.with_library_mut_if_id(&expected_library_id, |db, _| {
        super::entities::trash(db, &asset_ids)
    }))
}
#[tauri::command]
pub async fn prepare_asset_analysis(
    app: tauri::AppHandle,
    expected_library_id: String,
    asset_id: String,
    mode: super::intelligence_models::AnalysisMode,
) -> Result<super::intelligence_models::AnalysisInputPreview, String> {
    uuid(&asset_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let base =
            current(state.with_library_if_id(&expected_library_id, |_, base| Ok(base.to_owned())))?;
        let db = crate::tasks::open_media_worker_connection(&base.join("library.db"))?;
        let resource = app.path().resource_dir().ok();
        let stop =
            || state.active_library_id().ok().as_deref() != Some(expected_library_id.as_str());
        let preview =
            crate::ai::analyzer::prepare(&db, &base, resource.as_deref(), &asset_id, mode, &stop)?;
        if stop() {
            return Err("素材库已切换，请在当前库重新预览".into());
        }
        Ok(preview)
    })
    .await
    .map_err(|_| "分析资料准备中断，请重试".to_string())?
}
#[tauri::command]
pub fn get_asset_analysis(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
    analysis_id: Option<String>,
) -> Result<Option<super::intelligence_models::AssetAnalysis>, String> {
    uuid(&asset_id)?;
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        if let Some(id) = &analysis_id {
            super::analysis::by_id(db, &asset_id, id)
        } else {
            super::analysis::latest(db, &asset_id)
        }
    }))
}
#[tauri::command]
pub fn apply_asset_analysis_tags(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
    analysis_id: String,
    locale: String,
    selected: Vec<String>,
) -> Result<(), String> {
    uuid(&asset_id)?;
    uuid(&analysis_id)?;
    current(state.with_library_mut_if_id(&expected_library_id, |db, _| {
        super::analysis::apply_keywords(db, &asset_id, &analysis_id, &locale, &selected)
    }))
}
#[tauri::command]
pub fn create_asset_relation(
    state: State<AppState>,
    expected_library_id: String,
    input: super::intelligence_models::RelationInput,
) -> Result<super::intelligence_models::AssetRelation, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        super::relations::create(db, &input)
    }))
}
#[tauri::command]
pub fn delete_asset_relation(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
    relation_id: String,
) -> Result<(), String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        super::relations::delete(db, &asset_id, &relation_id)
    }))
}
#[tauri::command]
pub fn get_asset_graph(
    state: State<AppState>,
    expected_library_id: String,
    request: super::intelligence_models::GraphRequest,
) -> Result<super::intelligence_models::AssetGraph, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        super::relations::graph(db, &request)
    }))
}
#[tauri::command]
pub fn search_core_assets(
    state: State<AppState>,
    expected_library_id: String,
    request: CoreSearchRequest,
) -> Result<Page<Asset>, String> {
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        repository::search(db, &request)
    }))
}
#[tauri::command]
pub fn get_core_asset(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
    locale: String,
) -> Result<CoreAssetDetail, String> {
    uuid(&asset_id)?;
    let mut detail = current(state.with_library_if_id(&expected_library_id, |db, base| {
        repository::get(db, base, &asset_id, &locale)
    }))?;
    let ids = [asset_id.as_str(), detail.asset.origin.id.as_str()];
    for task in state.tasks.list(&expected_library_id)? {
        let full = state.tasks.get(&task.id)?;
        let payload = serde_json::to_value(&full.payload).map_err(|e| e.to_string())?;
        if references(&payload, &ids) || full.result.as_ref().is_some_and(|v| references(v, &ids)) {
            detail.processing_history.push(serde_json::json!({"id":task.id,"label":task.label,"status":task.status,"phase":task.phase,"message":task.message,"updatedAt":task.updated_at}));
            if detail.processing_history.len() == 40 {
                break;
            }
        }
    }
    Ok(detail)
}
fn references(value: &serde_json::Value, ids: &[&str]) -> bool {
    match value {
        serde_json::Value::String(s) => ids.contains(&s.as_str()),
        serde_json::Value::Array(a) => a.iter().any(|v| references(v, ids)),
        serde_json::Value::Object(o) => o.values().any(|v| references(v, ids)),
        _ => false,
    }
}
#[tauri::command]
pub fn get_asset_scan_page(
    state: State<AppState>,
    expected_library_id: String,
    task_id: String,
    offset: usize,
    limit: usize,
) -> Result<Page<ScanRow>, String> {
    uuid(&task_id)?;
    let task = state.tasks.get(&task_id)?;
    if task.library_id != expected_library_id
        || !matches!(task.payload, crate::tasks::TaskRequest::AssetScan { .. })
        || !matches!(task.status.as_str(), "completed" | "stopped")
    {
        return Err("扫描任务不属于当前库或尚未完成".into());
    }
    current(state.with_library_if_id(&expected_library_id, |db, base| {
        let mut page = scanner::read_page(&scanner::manifest_path(base, &task_id)?, offset, limit)?;
        for row in &mut page.items {
            service::mark_scan_duplicate(db, row)?;
        }
        Ok(page)
    }))
}
#[tauri::command]
pub fn set_core_asset_tags(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
    locale: String,
    tags: Vec<String>,
) -> Result<(), String> {
    uuid(&asset_id)?;
    current(state.with_library_mut_if_id(&expected_library_id, |db, _| {
        service::set_tags(db, &asset_id, &locale, &tags)
    }))
}
#[tauri::command]
pub fn set_core_tag_color(
    state: State<AppState>,
    expected_library_id: String,
    tag_id: String,
    color: String,
) -> Result<(), String> {
    uuid(&tag_id)?;
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        service::set_tag_color(db, &tag_id, &color)
    }))
}
#[tauri::command]
pub fn link_core_asset_project(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
    project_id: String,
    link: bool,
) -> Result<(), String> {
    uuid(&asset_id)?;
    uuid(&project_id)?;
    current(state.with_library_mut_if_id(&expected_library_id, |db, _| {
        service::link_project(db, &asset_id, &project_id, link)
    }))
}
#[tauri::command]
pub fn attach_core_asset_file(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
    source: SourceFileRef,
) -> Result<AssetFile, String> {
    uuid(&asset_id)?;
    uuid(&source.id)?;
    current(state.with_library_if_id(&expected_library_id, |db, base| {
        service::attach_file(db, base, &asset_id, &source)
    }))
}
#[tauri::command]
pub fn create_core_asset_version(
    state: State<AppState>,
    expected_library_id: String,
    input: CoreVersionInput,
) -> Result<AssetVersion, String> {
    uuid(&input.asset_id)?;
    uuid(&input.file_id)?;
    current(state.with_library_if_id(&expected_library_id, |db, base| {
        service::create_version(db, base, &input)
    }))
}
#[tauri::command]
pub fn delete_core_asset_version(
    state: State<AppState>,
    expected_library_id: String,
    version_id: String,
) -> Result<(), String> {
    uuid(&version_id)?;
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        service::delete_version(db, &version_id)
    }))
}
#[tauri::command]
pub fn add_core_asset_source(
    state: State<AppState>,
    expected_library_id: String,
    input: CoreSourceInput,
) -> Result<AssetSource, String> {
    uuid(&input.asset_id)?;
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        service::add_source(db, &input)
    }))
}
#[tauri::command]
pub fn remove_core_asset_source(
    state: State<AppState>,
    expected_library_id: String,
    asset_id: String,
    source_id: String,
) -> Result<(), String> {
    uuid(&asset_id)?;
    uuid(&source_id)?;
    current(state.with_library_if_id(&expected_library_id, |db, _| {
        service::remove_source(db, &asset_id, &source_id)
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_command_rejects_switched_library() {
        assert!(current::<()>(Ok(None)).is_err());
        assert!(uuid("../bad").is_err());
        assert!(references(
            &serde_json::json!({"rows":[{"assetId":"a"}]}),
            &["a"]
        ));
        assert!(!references(
            &serde_json::json!({"url":"https://example/a"}),
            &["a"]
        ));
    }
}
