use crate::{
    backup, bilingual, db, fab, images, importer, link_checker, media, media_library, models::*,
    organization, projects, reference_boards, state::AppState, translation,
};
use std::path::Path;

#[tauri::command]
pub fn get_selection_labels(
    state: State<AppState>,
    ids: Vec<String>,
    media: bool,
) -> Result<Vec<serde_json::Value>, String> {
    if ids.len() > 5000 {
        return Err("单次最多读取 5000 项".into());
    }
    state.with_library(|connection,_| {
        let sql = if media {"SELECT name FROM media_entries WHERE id=?1"} else {"SELECT COALESCE((SELECT name FROM asset_localizations WHERE asset_id=assets.id AND locale='zh-CN' AND name<>''),name) FROM assets WHERE id=?1"};
        let mut statement=connection.prepare(sql).map_err(|e|e.to_string())?;
        Ok(ids.into_iter().map(|id| {let name=statement.query_row([&id],|r|r.get::<_,String>(0)).unwrap_or_default();serde_json::json!({"id":id,"name":name})}).collect())
    })
}

#[tauri::command]
pub fn search_media_entries(
    state: State<AppState>,
    request: MediaSearchRequest,
) -> Result<Page<MediaEntry>, String> {
    state.with_library(|connection, _| media_library::search(connection, &request))
}
#[tauri::command]
pub fn select_media_entry_ids(
    state: State<AppState>,
    request: MediaSearchRequest,
) -> Result<Vec<String>, String> {
    state.with_library(|connection, _| media_library::select_ids(connection, &request))
}
#[tauri::command]
pub fn get_media_entry(state: State<AppState>, id: String) -> Result<MediaEntryDetail, String> {
    state.with_library(|connection, _| media_library::get(connection, &id))
}
#[tauri::command]
pub async fn open_model_location(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_library(|connection, base| {
            crate::model_access::reveal_model(connection, base, &id)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_model_package(
    app: AppHandle,
    id: String,
    destination: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().with_library(|connection, base| {
            crate::model_access::export_model(connection, base, &id, Path::new(&destination))
                .map(|path| path.to_string_lossy().to_string())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn pending_model_thumbnails(
    state: State<AppState>,
) -> Result<Vec<media_library::ModelThumbnailJob>, String> {
    state.with_library(|connection, _| media_library::pending_model_thumbnails(connection))
}
#[tauri::command]
pub fn save_model_thumbnail(
    state: State<AppState>,
    entry_id: String,
    bytes: Vec<u8>,
    expected_library_id: Option<String>,
) -> Result<String, String> {
    state.with_active_mut(|active| {
        if expected_library_id
            .as_ref()
            .is_some_and(|id| id != &active.marker.library_id)
        {
            return Err("素材库已切换，未保存封面".into());
        }
        let connection = active
            .connection
            .as_mut()
            .ok_or("数据库正在维护，请稍后重试")?;
        media_library::save_model_thumbnail(connection, &active.base_dir, &entry_id, &bytes)
    })
}
#[tauri::command]
pub fn report_model_thumbnail_failure(
    state: State<AppState>,
    entry_id: String,
    message: String,
) -> Result<(), String> {
    state.with_library(|connection, _| {
        media_library::report_model_thumbnail_failure(connection, &entry_id, &message)
    })
}
#[tauri::command]
pub fn import_media_entries(
    app: AppHandle,
    state: State<AppState>,
    request: MediaImportRequest,
) -> Result<MediaImportReport, String> {
    let resources = app.path().resource_dir().ok();
    state.with_library_mut(|connection, base| {
        media_library::import(connection, base, resources.as_deref(), request)
    })
}
#[tauri::command]
pub fn collect_asset_previews_to_media(
    app: AppHandle,
    state: State<AppState>,
    asset_id: String,
) -> Result<MediaImportReport, String> {
    let resources = app.path().resource_dir().ok();
    state.with_library_mut(|c, base| {
        media_library::collect_asset_previews(c, base, resources.as_deref(), &asset_id)
    })
}
#[tauri::command]
pub fn add_media_images_to_board(
    state: State<AppState>,
    board_id: String,
    entry_ids: Vec<String>,
    placement: ReferencePlacement,
    expected_library_id: Option<String>,
) -> Result<ReferenceImageAddReport, String> {
    state.with_active_mut(|active| {
        if expected_library_id
            .as_ref()
            .is_some_and(|id| id != &active.marker.library_id)
        {
            return Err("素材库已切换，未加入参考板".into());
        }
        let connection = active
            .connection
            .as_mut()
            .ok_or("数据库正在维护，请稍后重试")?;
        reference_boards::add_media_images(
            connection,
            &active.base_dir,
            &board_id,
            &entry_ids,
            &placement,
        )
    })
}
#[tauri::command]
pub fn update_media_entry(
    state: State<AppState>,
    id: String,
    name: String,
    description: String,
    author: String,
    source_url: String,
    license: String,
    tags: Vec<String>,
    expected_library_id: Option<String>,
) -> Result<(), String> {
    state.with_active_mut(|active| {
        if expected_library_id
            .as_ref()
            .is_some_and(|id| id != &active.marker.library_id)
        {
            return Err("素材库已切换，未修改媒体".into());
        }
        let connection = active
            .connection
            .as_ref()
            .ok_or("数据库正在维护，请稍后重试")?;
        media_library::update(
            connection,
            &id,
            name,
            description,
            author,
            source_url,
            license,
            tags,
        )
    })
}
#[tauri::command]
pub fn delete_media_entries(state: State<AppState>, ids: Vec<String>) -> Result<usize, String> {
    state.with_library_mut(|c, _| media_library::delete_entries(c, ids))
}
#[tauri::command]
pub fn batch_update_media_entries(
    state: State<AppState>,
    update: MediaBatchUpdate,
    expected_library_id: Option<String>,
) -> Result<BatchUpdateReport, String> {
    state.with_active_mut(|active| {
        if expected_library_id
            .as_ref()
            .is_some_and(|id| id != &active.marker.library_id)
        {
            return Err("素材库已切换，未整理媒体".into());
        }
        let connection = active
            .connection
            .as_mut()
            .ok_or("数据库正在维护，请稍后重试")?;
        media_library::batch_update(connection, update)
    })
}
#[tauri::command]
pub fn list_media_folders(
    state: State<AppState>,
    kind: MediaKind,
) -> Result<Vec<MediaFolder>, String> {
    state.with_library(|c, _| media_library::list_folders(c, &kind))
}
#[tauri::command]
pub fn upsert_media_folder(
    state: State<AppState>,
    kind: MediaKind,
    id: Option<String>,
    parent_id: Option<String>,
    name: String,
) -> Result<MediaFolder, String> {
    state.with_library(|c, _| media_library::upsert_folder(c, kind, id, parent_id, name))
}
#[tauri::command]
pub fn move_media_folder(
    state: State<AppState>,
    id: String,
    target_parent_id: Option<String>,
    target_index: usize,
) -> Result<(), String> {
    state.with_library_mut(|c, _| {
        media_library::move_folder(c, &id, target_parent_id.as_deref(), target_index)
    })
}
#[tauri::command]
pub fn delete_media_folder(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library_mut(|c, _| media_library::delete_folder(c, &id))
}
#[tauri::command]
pub fn link_media_assets(
    state: State<AppState>,
    entry_ids: Vec<String>,
    asset_ids: Vec<String>,
) -> Result<usize, String> {
    state.with_library(|c, _| media_library::link_assets(c, entry_ids, asset_ids, true))
}
#[tauri::command]
pub fn unlink_media_assets(
    state: State<AppState>,
    entry_ids: Vec<String>,
    asset_ids: Vec<String>,
) -> Result<usize, String> {
    state.with_library(|c, _| media_library::link_assets(c, entry_ids, asset_ids, false))
}
#[tauri::command]
pub fn link_project_media(
    state: State<AppState>,
    entry_ids: Vec<String>,
    project_ids: Vec<String>,
    expected_library_id: Option<String>,
) -> Result<usize, String> {
    state.with_active_mut(|active| {
        if expected_library_id
            .as_ref()
            .is_some_and(|id| id != &active.marker.library_id)
        {
            return Err("素材库已切换，未加入项目".into());
        }
        let connection = active
            .connection
            .as_ref()
            .ok_or("数据库正在维护，请稍后重试")?;
        media_library::link_projects(connection, entry_ids, project_ids, true)
    })
}
#[tauri::command]
pub fn unlink_project_media(
    state: State<AppState>,
    entry_ids: Vec<String>,
    project_ids: Vec<String>,
) -> Result<usize, String> {
    state.with_library(|c, _| media_library::link_projects(c, entry_ids, project_ids, false))
}
use arboard::Clipboard;
use std::{collections::HashMap, path::PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;

#[tauri::command]
pub fn list_projects(
    state: State<AppState>,
    include_archived: Option<bool>,
) -> Result<Vec<ProjectSummary>, String> {
    state.with_library(|connection, _| {
        projects::list_projects(connection, include_archived.unwrap_or(false))
    })
}

#[tauri::command]
pub fn get_project(
    state: State<AppState>,
    id: String,
    content_language: Option<String>,
    mark_opened: Option<bool>,
    paged: Option<bool>,
) -> Result<CreativeProject, String> {
    state.with_library(|connection, _| {
        if paged.unwrap_or(false) {
            return projects::get_project_workspace(connection, &id, mark_opened.unwrap_or(true));
        }
        projects::get_project(
            connection,
            &id,
            content_language.as_deref().unwrap_or("zh-CN"),
            mark_opened.unwrap_or(true),
        )
    })
}

#[tauri::command]
pub fn list_project_assets(
    state: State<AppState>,
    id: String,
    content_language: String,
    query: String,
    status: Option<String>,
    offset: i64,
    limit: i64,
) -> Result<Page<ProjectAssetLink>, String> {
    state.with_library(|db, _| {
        projects::asset_page(
            db,
            &id,
            &content_language,
            &query,
            status.as_deref(),
            offset,
            limit,
        )
    })
}
#[tauri::command]
pub fn list_project_tasks(
    state: State<AppState>,
    id: String,
    status: Option<String>,
    offset: i64,
    limit: i64,
) -> Result<Page<ProjectTask>, String> {
    state.with_library(|db, _| projects::task_page(db, &id, status.as_deref(), offset, limit))
}
#[tauri::command]
pub fn get_project_unit_impact(
    state: State<AppState>,
    id: String,
) -> Result<serde_json::Value, String> {
    state.with_library(|db, _| projects::unit_impact(db, &id))
}

#[tauri::command]
pub fn upsert_project(
    state: State<AppState>,
    input: ProjectInput,
) -> Result<CreativeProject, String> {
    state.with_library_mut(|connection, _| projects::upsert_project(connection, input))
}

#[tauri::command]
pub fn archive_project(state: State<AppState>, id: String, archived: bool) -> Result<(), String> {
    state.with_library(|connection, _| projects::archive_project(connection, &id, archived))
}

#[tauri::command]
pub fn delete_project(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library(|connection, _| projects::delete_project(connection, &id))
}

#[tauri::command]
pub fn add_project_assets(
    state: State<AppState>,
    project_id: String,
    asset_ids: Vec<String>,
) -> Result<usize, String> {
    state.with_library_mut(|connection, _| {
        projects::add_project_assets(connection, &project_id, asset_ids)
    })
}

#[tauri::command]
pub fn update_project_assets(
    state: State<AppState>,
    update: ProjectAssetUpdate,
) -> Result<usize, String> {
    state.with_library_mut(|connection, _| projects::update_project_assets(connection, update))
}

#[tauri::command]
pub fn remove_project_assets(
    state: State<AppState>,
    project_id: String,
    asset_ids: Vec<String>,
) -> Result<usize, String> {
    state.with_library_mut(|connection, _| {
        projects::remove_project_assets(connection, &project_id, asset_ids)
    })
}

#[tauri::command]
pub fn upsert_project_unit(
    state: State<AppState>,
    input: ProjectUnitInput,
) -> Result<ProjectUnit, String> {
    state.with_library(|connection, _| projects::upsert_project_unit(connection, input))
}

#[tauri::command]
pub fn set_project_shot_video(
    state: State<AppState>,
    shot_id: String,
    entry_id: Option<String>,
) -> Result<(), String> {
    state.with_library_mut(|connection, _| {
        projects::set_project_shot_video(connection, &shot_id, entry_id.as_deref())
    })
}

#[tauri::command]
pub fn delete_project_unit(state: State<AppState>, id: String) -> Result<(i64, i64), String> {
    state.with_library(|connection, _| projects::delete_project_unit(connection, &id))
}

#[tauri::command]
pub fn reorder_project_units(
    state: State<AppState>,
    project_id: String,
    parent_id: Option<String>,
    ids: Vec<String>,
) -> Result<(), String> {
    state.with_library_mut(|connection, _| {
        projects::reorder_project_units(connection, &project_id, parent_id, ids)
    })
}

#[tauri::command]
pub fn upsert_project_task(
    state: State<AppState>,
    input: ProjectTaskInput,
) -> Result<ProjectTask, String> {
    state.with_library_mut(|connection, _| projects::upsert_project_task(connection, input))
}

#[tauri::command]
pub fn delete_project_task(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library(|connection, _| projects::delete_project_task(connection, &id))
}

#[tauri::command]
pub fn move_project_task(
    state: State<AppState>,
    id: String,
    status: String,
    target_index: i64,
) -> Result<(), String> {
    state.with_library_mut(|connection, _| {
        projects::move_project_task(connection, &id, &status, target_index)
    })
}

#[tauri::command]
pub fn set_project_task_assets(
    state: State<AppState>,
    task_id: String,
    asset_ids: Vec<String>,
) -> Result<(), String> {
    state.with_library_mut(|connection, _| {
        projects::set_project_task_assets(connection, &task_id, asset_ids)
    })
}

#[tauri::command]
pub fn link_project_board(
    state: State<AppState>,
    project_id: String,
    board_id: String,
    main: Option<bool>,
) -> Result<(), String> {
    state.with_library_mut(|connection, _| {
        projects::link_project_board(connection, &project_id, &board_id, main.unwrap_or(false))
    })
}

#[tauri::command]
pub fn unlink_project_board(
    state: State<AppState>,
    project_id: String,
    board_id: String,
) -> Result<(), String> {
    state.with_library(|connection, _| {
        projects::unlink_project_board(connection, &project_id, &board_id)
    })
}

#[tauri::command]
pub fn set_main_project_board(
    state: State<AppState>,
    project_id: String,
    board_id: String,
) -> Result<(), String> {
    state.with_library_mut(|connection, _| {
        projects::set_main_project_board(connection, &project_id, &board_id)
    })
}

#[tauri::command]
pub fn upsert_project_path(
    state: State<AppState>,
    input: ProjectPathInput,
) -> Result<ProjectPathShortcut, String> {
    state.with_library(|connection, _| projects::upsert_project_path(connection, input))
}

#[tauri::command]
pub fn delete_project_path(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library(|connection, _| projects::delete_project_path(connection, &id))
}

#[tauri::command]
pub fn check_project_path(state: State<AppState>, id: String) -> Result<ProjectPathCheck, String> {
    state.with_library(|connection, _| projects::check_project_path(connection, &id))
}

#[tauri::command]
pub fn open_project_path(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library(|connection, _| projects::open_project_path(connection, &id))
}

#[tauri::command]
pub fn get_library_meta(
    state: State<'_, AppState>,
    content_language: Option<String>,
) -> Result<LibraryMeta, String> {
    let language = content_language.unwrap_or_else(default_content_language);
    state.with_library(|connection, _| db::library_meta(connection, &language))
}

#[tauri::command]
pub fn search_assets(
    state: State<AppState>,
    request: SearchRequest,
) -> Result<Page<AssetCard>, String> {
    state.with_library(|connection, base_dir| {
        match organization::resolve_smart_collection(connection, &request)? {
            Some(request) => db::search_assets(connection, base_dir, &request),
            None => Ok(Page {
                items: Vec::new(),
                total: 0,
                offset: request.offset,
                limit: request.limit,
            }),
        }
    })
}

#[tauri::command]
pub fn select_asset_ids(
    state: State<AppState>,
    request: SearchRequest,
) -> Result<AssetSelection, String> {
    state.with_library(|connection, base_dir| {
        match organization::resolve_smart_collection(connection, &request)? {
            Some(request) => db::select_asset_ids(connection, base_dir, &request),
            None => Ok(AssetSelection {
                ids: Vec::new(),
                total: 0,
            }),
        }
    })
}

#[tauri::command]
pub fn get_personalization_settings(
    state: State<AppState>,
) -> Result<PersonalizationState, String> {
    let global = state.global_preferences()?;
    let library =
        state.with_library(|connection, _| organization::load_library_preferences(connection))?;
    Ok(PersonalizationState { global, library })
}

#[tauri::command]
pub fn save_global_preferences(
    app: AppHandle,
    state: State<AppState>,
    preferences: GlobalPreferences,
) -> Result<GlobalPreferences, String> {
    let value = state.save_global_preferences(preferences)?;
    app.emit("personalization-changed", &value)
        .map_err(|e| e.to_string())?;
    Ok(value)
}

#[tauri::command]
pub fn save_library_preferences(
    state: State<AppState>,
    mut preferences: LibraryPreferences,
) -> Result<LibraryPreferences, String> {
    state.with_library(|connection, _| {
        preferences.workspace_contexts =
            organization::load_library_preferences(connection)?.workspace_contexts;
        organization::save_library_preferences(connection, preferences)
    })
}

fn validate_workspace_module(module_id: &str) -> Result<(), String> {
    if [
        "navigation",
        "library",
        "favorites",
        "recent",
        "health",
        "image",
        "model",
        "audio",
        "video",
        "projects",
        "knowledge",
        "browserCapture",
        "ueLibrary",
    ]
    .contains(&module_id)
    {
        Ok(())
    } else {
        Err("未知的浏览模块".into())
    }
}

#[tauri::command]
pub fn get_workspace_context(
    state: State<AppState>,
    module_id: String,
) -> Result<serde_json::Value, String> {
    validate_workspace_module(&module_id)?;
    let id = state.active_library_id()?;
    state.with_library_if_id(&id, |connection, _| {
        let prefs = organization::load_library_preferences(connection)?;
        Ok(serde_json::json!({"libraryId":id,"enabled":prefs.remember_workspace,"value":prefs.workspace_contexts.get(&module_id)}))
    })?.ok_or("素材库已切换，请重新读取浏览状态".into())
}

#[tauri::command]
pub fn save_workspace_context(
    state: State<AppState>,
    library_id: String,
    module_id: String,
    value: serde_json::Value,
) -> Result<bool, String> {
    validate_workspace_module(&module_id)?;
    if value.to_string().len() > 131072 {
        return Err("浏览状态过大".into());
    }
    Ok(state
        .with_library_if_id(&library_id, |connection, _| {
            let mut prefs = organization::load_library_preferences(connection)?;
            if prefs.remember_workspace {
                prefs.workspace_contexts.insert(module_id, value);
                organization::save_library_preferences(connection, prefs)?;
            }
            Ok(true)
        })?
        .unwrap_or(false))
}

#[tauri::command]
pub fn reset_personalization_settings(
    app: AppHandle,
    state: State<AppState>,
    scope: String,
) -> Result<PersonalizationState, String> {
    if matches!(scope.as_str(), "global" | "all") {
        let value = state.save_global_preferences(GlobalPreferences::default())?;
        app.emit("personalization-changed", &value)
            .map_err(|e| e.to_string())?;
    }
    if matches!(scope.as_str(), "library" | "all") {
        state.with_library(|connection, _| {
            organization::save_library_preferences(connection, LibraryPreferences::default())
                .map(|_| ())
        })?;
    }
    get_personalization_settings(state)
}

#[tauri::command]
pub fn list_smart_collections(state: State<AppState>) -> Result<Vec<SmartCollection>, String> {
    state.with_library(|connection, _| organization::list_smart_collections(connection))
}

#[tauri::command]
pub fn create_smart_collection(
    state: State<AppState>,
    input: SmartCollectionInput,
) -> Result<SmartCollection, String> {
    state.with_library(|connection, _| {
        organization::upsert_smart_collection(
            connection,
            SmartCollectionInput { id: None, ..input },
        )
    })
}

#[tauri::command]
pub fn update_smart_collection(
    state: State<AppState>,
    id: String,
    input: SmartCollectionInput,
) -> Result<SmartCollection, String> {
    state.with_library(|connection, _| {
        organization::upsert_smart_collection(
            connection,
            SmartCollectionInput {
                id: Some(id),
                ..input
            },
        )
    })
}

#[tauri::command]
pub fn duplicate_smart_collection(
    state: State<AppState>,
    id: String,
) -> Result<SmartCollection, String> {
    state.with_library(|connection, _| organization::duplicate_smart_collection(connection, &id))
}

#[tauri::command]
pub fn delete_smart_collection(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library(|connection, _| organization::delete_smart_collection(connection, &id))
}

#[tauri::command]
pub fn reorder_smart_collections(state: State<AppState>, ids: Vec<String>) -> Result<(), String> {
    state.with_library_mut(|connection, _| organization::reorder_smart_collections(connection, ids))
}

#[tauri::command]
pub fn list_tags(
    state: State<AppState>,
    locale: String,
    query: String,
    offset: i64,
    limit: i64,
) -> Result<Page<TagUsage>, String> {
    state.with_library(|connection, _| {
        organization::list_tags(connection, &locale, &query, offset, limit)
    })
}

#[tauri::command]
pub fn rename_tag(
    state: State<AppState>,
    id: String,
    name: String,
) -> Result<TagMutationReport, String> {
    state.with_library_mut(|connection, _| organization::rename_tag(connection, &id, &name))
}

#[tauri::command]
pub fn merge_tags(
    state: State<AppState>,
    source_ids: Vec<String>,
    target_id: String,
) -> Result<TagMutationReport, String> {
    state.with_library_mut(|connection, _| {
        organization::merge_tags(connection, source_ids, &target_id)
    })
}

#[tauri::command]
pub fn delete_tags(state: State<AppState>, ids: Vec<String>) -> Result<TagMutationReport, String> {
    state.with_library_mut(|connection, _| organization::delete_tags(connection, ids))
}

#[tauri::command]
pub fn delete_unused_tags(
    state: State<AppState>,
    locale: String,
) -> Result<TagMutationReport, String> {
    state.with_library(|connection, _| organization::delete_unused_tags(connection, &locale))
}

#[tauri::command]
pub fn get_asset(
    state: State<AppState>,
    id: String,
    content_language: Option<String>,
) -> Result<AssetDetail, String> {
    let language = content_language.unwrap_or_else(default_content_language);
    state.with_library(|connection, _| db::get_asset(connection, &id, true, &language))
}

#[tauri::command]
pub fn upsert_asset(state: State<AppState>, input: AssetInput) -> Result<AssetDetail, String> {
    state.with_library_mut(|connection, base_dir| db::upsert_asset(connection, base_dir, input))
}

#[tauri::command]
pub fn delete_asset(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library_mut(|connection, base_dir| db::delete_asset(connection, base_dir, &id))
}

#[tauri::command]
pub fn set_favorite(state: State<AppState>, id: String, favorite: bool) -> Result<(), String> {
    state.with_library(|connection, _| db::set_favorite(connection, &id, favorite))
}

#[tauri::command]
pub fn upsert_category(
    state: State<AppState>,
    id: Option<String>,
    name: String,
    parent_id: Option<String>,
    preserve_parent: Option<bool>,
) -> Result<String, String> {
    state.with_library(|connection, _| {
        db::upsert_category(
            connection,
            id,
            name,
            parent_id,
            preserve_parent.unwrap_or(false),
        )
    })
}

#[tauri::command]
pub fn delete_category(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library_mut(|connection, _| {
        db::delete_library_items(
            connection,
            DeleteRequest {
                asset_ids: Vec::new(),
                category_id: Some(id),
            },
        )
        .map(|_| ())
    })
}

#[tauri::command]
pub fn get_delete_impact(
    state: State<AppState>,
    request: DeleteRequest,
) -> Result<DeleteResult, String> {
    state.with_library(|connection, _| db::delete_impact(connection, &request))
}

#[tauri::command]
pub fn delete_library_items(
    state: State<AppState>,
    request: DeleteRequest,
) -> Result<DeleteResult, String> {
    state.with_library_mut(|connection, _| db::delete_library_items(connection, request))
}

#[tauri::command]
pub fn list_trash(
    state: State<AppState>,
    offset: i64,
    limit: i64,
) -> Result<Page<TrashBatch>, String> {
    state.with_library(|connection, _| db::list_trash(connection, offset, limit))
}

#[tauri::command]
pub fn restore_trash_batch(
    state: State<AppState>,
    batch_id: String,
) -> Result<DeleteResult, String> {
    state.with_library_mut(|connection, _| db::restore_trash_batch(connection, &batch_id))
}

#[tauri::command]
pub fn purge_trash_batch(state: State<AppState>, batch_id: String) -> Result<(), String> {
    state.with_library_mut(|connection, base_dir| {
        db::purge_trash_batch(connection, base_dir, &batch_id)
    })
}

#[tauri::command]
pub fn empty_trash(state: State<AppState>) -> Result<usize, String> {
    state.with_library_mut(|connection, base_dir| db::empty_trash(connection, base_dir))
}

#[tauri::command]
pub fn prepare_reference_cover_ids(
    state: State<AppState>,
    ids: Vec<String>,
) -> Result<Vec<String>, String> {
    state.with_library(|connection, _| db::prepare_reference_cover_ids(connection, &ids))
}

#[tauri::command]
pub fn get_image_data(
    state: State<AppState>,
    image_id: String,
    thumbnail: bool,
) -> Result<String, String> {
    state.with_library(|connection, base_dir| {
        let relative = db::image_path(connection, &image_id, thumbnail)?;
        images::image_data_url(base_dir, &relative)
    })
}

#[tauri::command]
pub async fn import_asset_media(
    app: AppHandle,
    state: State<'_, AppState>,
    asset_id: String,
    paths: Vec<String>,
) -> Result<Vec<AssetMedia>, String> {
    let task = crate::tasks::start_library_task(
        app.clone(),
        state,
        crate::tasks::TaskRequest::AssetMediaImport { asset_id, paths },
    )?;
    serde_json::from_value(crate::tasks::await_task_result(&app, &task.id).await?)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_asset_media(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library_mut(|connection, base_dir| media::delete(connection, base_dir, &id))
}

#[tauri::command]
pub async fn retry_asset_media(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<AssetMedia, String> {
    let task = crate::tasks::start_library_task(
        app.clone(),
        state,
        crate::tasks::TaskRequest::AssetMediaRetry { id },
    )?;
    serde_json::from_value(crate::tasks::await_task_result(&app, &task.id).await?)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_asset_media(
    state: State<AppState>,
    asset_id: String,
    ids: Vec<String>,
) -> Result<(), String> {
    state.with_library_mut(|connection, _| media::reorder(connection, &asset_id, ids))
}

#[tauri::command]
pub fn set_asset_cover_media(
    state: State<AppState>,
    asset_id: String,
    media_id: Option<String>,
    image_id: Option<String>,
) -> Result<(), String> {
    state.with_library_mut(|connection, _| {
        media::set_cover(
            connection,
            &asset_id,
            media_id.as_deref(),
            image_id.as_deref(),
        )
    })
}

#[tauri::command]
pub fn copy_extraction_code(state: State<AppState>, id: String) -> Result<(), String> {
    let (_, code) = state.with_library(|connection, _| db::share_info(connection, &id))?;
    if code.is_empty() {
        return Err("该素材没有提取码".into());
    }
    Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(code))
        .map_err(|e| format!("复制提取码失败：{e}"))
}

#[tauri::command]
pub fn open_share_link(state: State<AppState>, id: String) -> Result<(), String> {
    let (link, code) = state.with_library(|connection, _| db::share_info(connection, &id))?;
    if link.trim().is_empty() {
        return Err("该素材未添加百度网盘链接".into());
    }
    if !code.is_empty() {
        Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(code))
            .map_err(|e| format!("复制提取码失败：{e}"))?;
    }
    open_http_url(&link)?;
    state.with_library(|connection, _| {
        connection
            .execute(
                "UPDATE assets SET last_viewed_at=?1 WHERE id=?2",
                rusqlite::params![chrono::Utc::now().to_rfc3339(), id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    open_http_url(&url)
}

fn open_http_url(raw: &str) -> Result<(), String> {
    let url = Url::parse(raw).map_err(|_| "链接无效")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("只允许打开 http/https 链接".into());
    }
    open::that(url.as_str()).map_err(|e| format!("打开浏览器失败：{e}"))
}

#[tauri::command]
pub fn preview_import(
    state: State<AppState>,
    path: String,
    mapping: HashMap<String, String>,
) -> Result<ImportPreview, String> {
    state.with_library(|connection, _| importer::preview(connection, &PathBuf::from(path), mapping))
}

#[tauri::command]
pub async fn import_assets(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    mapping: HashMap<String, String>,
    auto_translate: bool,
) -> Result<ImportReport, String> {
    let library_id = state.active_library_id()?;
    if importer::is_fab_import(&mapping) {
        let rows = importer::fab_rows(&PathBuf::from(&path), mapping)?;
        let total = rows.len();
        let mut report = ImportReport {
            imported: 0,
            skipped: 0,
            failed: 0,
            rows: Vec::new(),
        };
        for (index, row) in rows.into_iter().enumerate() {
            if state.tasks.stop_label("Fab / 表格导入") {
                break;
            }
            emit_import_progress(
                &app,
                index + 1,
                total,
                &report,
                &row.fab_url,
                "读取 Fab 元数据",
            );
            if row.fab_url.trim().is_empty() {
                report.failed += 1;
                report.rows.push(ImportRowResult {
                    row: row.row,
                    name: String::new(),
                    status: "error".into(),
                    messages: vec!["缺少 Fab URL".into()],
                    suggested_category_path: None,
                    actual_category_path: None,
                    duplicate_asset_id: None,
                    duplicate_source: None,
                    asset_id: None,
                });
                continue;
            }
            let listing_id = match fab::listing_id_from_url(&row.fab_url) {
                Ok(value) => value.to_string(),
                Err(error) => {
                    report.failed += 1;
                    report.rows.push(ImportRowResult {
                        row: row.row,
                        name: row.fab_url.clone(),
                        status: "error".into(),
                        messages: vec![error],
                        suggested_category_path: None,
                        actual_category_path: None,
                        duplicate_asset_id: None,
                        duplicate_source: None,
                        asset_id: None,
                    });
                    continue;
                }
            };
            let duplicate = state
                .with_library_if_id(&library_id, |connection, _| {
                    db::fab_duplicate_match(connection, &listing_id, None)
                })?
                .ok_or("导入过程中已切换素材库，已停止")?;
            if let Some(duplicate) = duplicate {
                report.skipped += 1;
                report.rows.push(ImportRowResult {
                    row: row.row,
                    name: duplicate.asset_name.clone(),
                    status: "duplicate".into(),
                    messages: vec![if duplicate.location == "trash" {
                        format!(
                            "Fab 素材“{}”已在回收站，请先恢复或永久删除",
                            duplicate.asset_name
                        )
                    } else {
                        format!(
                            "Fab 素材“{}”已存在于 {}",
                            duplicate.asset_name, duplicate.category_path
                        )
                    }],
                    suggested_category_path: None,
                    actual_category_path: Some(duplicate.category_path),
                    duplicate_asset_id: Some(duplicate.asset_id),
                    duplicate_source: Some(duplicate.location),
                    asset_id: None,
                });
                continue;
            }
            let metadata = match fab::fetch_metadata(&row.fab_url).await {
                Ok(value) => value,
                Err(error) => {
                    report.failed += 1;
                    report.rows.push(ImportRowResult {
                        row: row.row,
                        name: row.fab_url.clone(),
                        status: "error".into(),
                        messages: vec![error],
                        suggested_category_path: None,
                        actual_category_path: None,
                        duplicate_asset_id: None,
                        duplicate_source: None,
                        asset_id: None,
                    });
                    continue;
                }
            };
            let suggested_category = metadata.suggested_category_path.join(" / ");
            let category_existed = state
                .with_library_if_id(&library_id, |connection, _| {
                    db::auto_category_path_exists(connection, &metadata.suggested_category_path)
                })?
                .ok_or("导入过程中已切换素材库，已停止")?;
            emit_import_progress(
                &app,
                index + 1,
                total,
                &report,
                &metadata.name,
                "保存预览图与素材信息",
            );
            let parsed_share = if row.baidu_text.trim().is_empty() {
                ParsedShareText::default()
            } else {
                parse_share_text_value(&row.baidu_text).unwrap_or_else(|_| ParsedShareText {
                    share_url: row.baidu_text.trim().into(),
                    extraction_code: String::new(),
                })
            };
            let mut localizations = bilingual::fab_localizations(LocalizedAssetText {
                name: metadata.name.clone(),
                description: metadata.description.clone(),
                tags: metadata.tags.clone(),
                license: metadata.license.clone(),
            });
            let mut messages = Vec::new();
            if auto_translate {
                emit_import_progress(&app, index + 1, total, &report, &metadata.name, &{
                    let chars: usize = localizations
                        .values()
                        .map(|f| {
                            f.name.chars().count()
                                + f.description.chars().count()
                                + f.license.chars().count()
                                + f.tags.iter().map(|t| t.chars().count()).sum::<usize>()
                        })
                        .sum();
                    format!(
                        "在线翻译 · 约 {chars} 字符 / {} Token（预估）",
                        chars * 2 + 1600
                    )
                });
                messages.extend(translate_missing_localizations(&state, &mut localizations).await);
            }
            let pending = bilingual::pending_fields(&localizations);
            if !pending.is_empty() {
                messages.push(format!("待补译：{}", pending.join("、")));
            }
            let images = metadata
                .preview_images
                .iter()
                .enumerate()
                .map(|(image_index, image)| ImageInput {
                    id: None,
                    source_path: Some(image.source_path.clone()),
                    original_name: Some(image.original_name.clone()),
                    is_cover: image_index == 0,
                    sort_order: image_index as i64,
                })
                .collect();
            let input = AssetInput {
                id: None,
                name: metadata.name.clone(),
                description: metadata.description.clone(),
                category_id: None,
                tags: metadata.tags.clone(),
                dcc_tools: metadata.dcc_tools.clone(),
                versions: if row.ue_versions.is_empty() {
                    metadata.versions.clone()
                } else {
                    row.ue_versions.clone()
                },
                formats: metadata.formats.clone(),
                size_bytes: None,
                author: metadata.author.clone(),
                source_url: metadata.canonical_url.clone(),
                fab_listing_id: Some(metadata.listing_id.clone()),
                auto_category_path: metadata.suggested_category_path.clone(),
                license: metadata.license.clone(),
                share_url: parsed_share.share_url,
                extraction_code: parsed_share.extraction_code,
                favorite: false,
                images,
                localizations,
                content_language: if bilingual::has_han(&metadata.name) {
                    "zh-CN"
                } else {
                    "en"
                }
                .into(),
            };
            match state.with_library_mut_if_id(&library_id, |connection, base_dir| {
                Ok(db::upsert_asset(connection, base_dir, input))
            })? {
                Some(Ok(asset)) => {
                    report.imported += 1;
                    let mut has_warnings = !messages.is_empty();
                    if suggested_category == "其他 / 待整理" {
                        has_warnings = true;
                        messages.push(if category_existed {
                            "Fab 分类无法识别，已归入其他 / 待整理".into()
                        } else {
                            "Fab 分类无法识别，已新建并归入其他 / 待整理".into()
                        });
                    } else if !suggested_category.is_empty() {
                        messages.push(if category_existed {
                            format!("自动归类：{suggested_category}")
                        } else {
                            format!("新建分类并自动归类：{suggested_category}")
                        });
                    }
                    if let Some(warning) = metadata.image_warning {
                        has_warnings = true;
                        messages.push(warning);
                    }
                    report.rows.push(ImportRowResult {
                        row: row.row,
                        name: metadata.name,
                        status: if !pending.is_empty() {
                            "translationPending".into()
                        } else if has_warnings {
                            "warning".into()
                        } else {
                            "imported".into()
                        },
                        messages,
                        suggested_category_path: Some(suggested_category.clone()),
                        actual_category_path: Some(suggested_category),
                        duplicate_asset_id: None,
                        duplicate_source: None,
                        asset_id: Some(asset.card.id),
                    });
                }
                Some(Err(error)) => {
                    report.failed += 1;
                    report.rows.push(ImportRowResult {
                        row: row.row,
                        name: metadata.name,
                        status: "error".into(),
                        messages: vec![error],
                        suggested_category_path: Some(suggested_category),
                        actual_category_path: None,
                        duplicate_asset_id: None,
                        duplicate_source: None,
                        asset_id: None,
                    });
                }
                None => return Err("导入过程中已切换素材库，已停止以避免写入其他库".into()),
            }
        }
        emit_import_progress(&app, total, total, &report, "", "完成");
        return Ok(report);
    }
    let mut report = state
        .with_library_mut_if_id(&library_id, |connection, base_dir| {
            importer::commit(connection, base_dir, &PathBuf::from(path), mapping)
        })?
        .ok_or("导入过程中已切换素材库，已停止以避免写入其他库")?;
    let report_total = report.rows.len();
    for (index, row) in report.rows.iter_mut().enumerate() {
        if state.tasks.stop_label("Fab / 表格导入") {
            break;
        }
        let Some(asset_id) = row.asset_id.clone() else {
            continue;
        };
        let Some(asset) = state.with_library_if_id(&library_id, |connection, _| {
            db::get_asset(connection, &asset_id, false, "en")
        })?
        else {
            return Err("导入过程中已切换素材库，已停止翻译".into());
        };
        let mut localizations = asset.localizations;
        if auto_translate {
            let _ = app.emit(
                "fab-import-progress",
                ImportProgress {
                    current: index + 1,
                    total: report_total,
                    imported: report.imported,
                    skipped: report.skipped,
                    failed: report.failed,
                    current_name: row.name.clone(),
                    phase: "在线补齐双语信息".into(),
                },
            );
            row.messages
                .extend(translate_missing_localizations(&state, &mut localizations).await);
            localizations = state
                .with_library_mut_if_id(&library_id, |connection, _| {
                    db::fill_missing_localizations(connection, &asset_id, localizations)
                })?
                .ok_or("导入过程中已切换素材库，已停止翻译")?;
        }
        let pending = bilingual::pending_fields(&localizations);
        if !pending.is_empty() {
            row.status = "translationPending".into();
            row.messages.push(format!("待补译：{}", pending.join("、")));
        }
    }
    Ok(report)
}

async fn translate_missing_localizations(
    state: &AppState,
    localizations: &mut HashMap<String, LocalizedAssetText>,
) -> Vec<String> {
    if bilingual::pending_fields(localizations).is_empty() {
        return Vec::new();
    }
    if !state.translation_service.configured("baidu")
        && !state.translation_service.configured("deepseek")
    {
        return vec!["尚未配置在线翻译凭据".into()];
    }
    let mut messages = Vec::new();
    for (source, target) in [("en", "zh-CN"), ("zh-CN", "en")] {
        let Some(request) = bilingual::missing_request(localizations, source, target) else {
            continue;
        };
        let terms = match state.effective_translation_terms(source, target) {
            Ok(value) => value,
            Err(error) => {
                messages.push(error);
                continue;
            }
        };
        match translation::translate(state, request, &terms).await {
            Ok(preview) => {
                messages.extend(bilingual::merge_preview(localizations, target, preview))
            }
            Err(error) => messages.push(format!("{target} 自动翻译失败：{error}")),
        }
    }
    messages
}

#[tauri::command]
pub async fn batch_translate_missing_assets(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<BatchTranslationReport, String> {
    if ids.len() > 5000 {
        return Err("单次最多补译 5000 条素材".into());
    }
    let library_id = state.active_library_id()?;
    let mut report = BatchTranslationReport::default();
    let total = ids.len();
    for (index, id) in ids.into_iter().enumerate() {
        if state.tasks.stop_label("双语补译") {
            break;
        }
        let detail = state
            .with_library_if_id(&library_id, |connection, _| {
                Ok(db::get_asset(connection, &id, false, "en"))
            })?
            .ok_or("补译过程中已切换素材库，已停止")?;
        let asset = match detail {
            Ok(asset) => asset,
            Err(error) => {
                report.skipped += 1;
                report.rows.push(BatchTranslationRow {
                    asset_id: id,
                    name: String::new(),
                    status: "skipped".into(),
                    messages: vec![error],
                });
                continue;
            }
        };
        let name = asset.card.name;
        let mut localizations = asset.localizations;
        if bilingual::pending_fields(&localizations).is_empty() {
            report.skipped += 1;
            report.rows.push(BatchTranslationRow {
                asset_id: id,
                name,
                status: "skipped".into(),
                messages: vec!["双语字段已完整".into()],
            });
            continue;
        }
        let _ = app.emit(
            "batch-translation-progress",
            ImportProgress {
                current: index + 1,
                total,
                imported: report.completed,
                skipped: report.skipped,
                failed: report.pending,
                current_name: name.clone(),
                phase: "在线补齐双语信息".into(),
            },
        );
        state.tasks.progress(
            "双语补译",
            index + 1,
            total,
            name.clone(),
            "在线补齐双语信息".into(),
        );
        state.tasks.checkpoint(
            "双语补译",
            serde_json::to_value(&report).map_err(|e| e.to_string())?,
        );
        let mut messages = translate_missing_localizations(&state, &mut localizations).await;
        let persisted = state
            .with_library_mut_if_id(&library_id, |connection, _| {
                db::fill_missing_localizations(connection, &id, localizations)
            })?
            .ok_or("补译过程中已切换素材库，已停止")?;
        let pending = bilingual::pending_fields(&persisted);
        let status = if pending.is_empty() {
            report.completed += 1;
            "complete"
        } else {
            report.pending += 1;
            messages.push(format!("待补译：{}", pending.join("、")));
            "pending"
        };
        report.rows.push(BatchTranslationRow {
            asset_id: id,
            name,
            status: status.into(),
            messages,
        });
    }
    Ok(report)
}

fn emit_import_progress(
    app: &AppHandle,
    current: usize,
    total: usize,
    report: &ImportReport,
    current_name: &str,
    phase: &str,
) {
    app.state::<AppState>().tasks.progress(
        "Fab / 表格导入",
        current,
        total,
        current_name.into(),
        phase.into(),
    );
    if let Ok(result) = serde_json::to_value(report) {
        app.state::<AppState>()
            .tasks
            .checkpoint("Fab / 表格导入", result);
    }
    let _ = app.emit(
        "fab-import-progress",
        ImportProgress {
            current,
            total,
            imported: report.imported,
            skipped: report.skipped,
            failed: report.failed,
            current_name: current_name.into(),
            phase: phase.into(),
        },
    );
}

#[tauri::command]
pub fn export_import_template(path: String) -> Result<(), String> {
    importer::export_template(&PathBuf::from(path))
}

#[tauri::command]
pub fn export_backup(state: State<AppState>, path: String) -> Result<(), String> {
    state.with_library(|connection, base_dir| {
        backup::export_library(connection, base_dir, &PathBuf::from(path))
    })
}

#[tauri::command]
pub fn restore_backup(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    if state.tasks.busy() {
        return Err("任务仍在执行，请先等待或安全停止再恢复素材库".into());
    }
    if app.get_webview_window("reference-board").is_some()
        && state.reference_window_board_id()?.is_some()
    {
        return Err("请先收回悬浮参考板，再恢复素材库".into());
    }
    state.with_active_mut(|active| {
        if state.tasks.busy() {
            return Err("任务仍在执行，请先等待或安全停止再恢复素材库".into());
        }
        backup::restore_library(active, &PathBuf::from(path))
    })
}

#[tauri::command]
pub fn get_library_locations(state: State<AppState>) -> Result<LibraryLocationState, String> {
    state.locations()
}

#[tauri::command]
pub fn change_library(
    app: AppHandle,
    state: State<AppState>,
    request: StorageChangeRequest,
) -> Result<LibraryLocationState, String> {
    if state.tasks.busy() {
        return Err("任务仍在执行，请先等待或安全停止再切换素材库".into());
    }
    if app.get_webview_window("reference-board").is_some()
        && state.reference_window_board_id()?.is_some()
    {
        return Err("请先收回悬浮参考板，再切换素材库".into());
    }
    state.change_library(request)
}

#[tauri::command]
pub fn forget_recent_library(state: State<AppState>, path: String) -> Result<(), String> {
    state.forget_recent(&path)
}

#[tauri::command]
pub fn read_clipboard_text() -> Result<String, String> {
    Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_text())
        .map_err(|e| format!("读取剪贴板失败：{e}"))
}

#[tauri::command]
pub fn parse_share_text(text: String) -> Result<ParsedShareText, String> {
    parse_share_text_value(&text)
}

#[tauri::command]
pub async fn fetch_fab_metadata(url: String) -> Result<FabMetadata, String> {
    fab::fetch_metadata(&url).await
}

#[tauri::command]
pub fn check_fab_url(
    state: State<AppState>,
    url: String,
    exclude_asset_id: Option<String>,
) -> Result<Option<FabDuplicateMatch>, String> {
    let listing_id = fab::listing_id_from_url(&url)?.to_string();
    state.with_library(|connection, _| {
        db::fab_duplicate_match(connection, &listing_id, exclude_asset_id.as_deref())
    })
}

pub fn parse_share_text_value(text: &str) -> Result<ParsedShareText, String> {
    let lowercase = text.to_lowercase();
    let start = ["https://pan.baidu.com/", "http://pan.baidu.com/"]
        .iter()
        .filter_map(|needle| lowercase.find(needle))
        .min();
    let share_url = start
        .map(|start| {
            text[start..]
                .chars()
                .take_while(|ch| !ch.is_whitespace() && !matches!(ch, '，' | '。' | '；' | ';'))
                .collect::<String>()
                .trim_end_matches(|ch| matches!(ch, ')' | '）' | ']' | '】' | '.' | ','))
                .to_string()
        })
        .unwrap_or_default();
    if share_url.is_empty() {
        return Err("没有识别到百度网盘链接".into());
    }
    let parsed_url = Url::parse(&share_url).map_err(|_| "百度网盘链接格式无效")?;
    let query_code = parsed_url
        .query_pairs()
        .find(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "pwd" | "password" | "code"
            )
        })
        .map(|(_, value)| value.into_owned());
    let extraction_code = query_code
        .or_else(|| extract_code(&lowercase))
        .unwrap_or_default();
    Ok(ParsedShareText {
        share_url,
        extraction_code,
    })
}

fn extract_code(text: &str) -> Option<String> {
    for marker in ["提取码", "访问码", "密码", "pwd", "code"] {
        if let Some(index) = text.find(marker) {
            let code = text[index + marker.len()..]
                .trim_start_matches(|ch: char| ch.is_whitespace() || matches!(ch, ':' | '：' | '='))
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric())
                .take(32)
                .collect::<String>();
            if !code.is_empty() {
                return Some(code);
            }
        }
    }
    None
}

#[tauri::command]
pub fn check_share_url(
    state: State<AppState>,
    url: String,
) -> Result<Option<DuplicateMatch>, String> {
    state.with_library(|connection, _| db::duplicate_match(connection, &url))
}

#[tauri::command]
pub fn batch_update_assets(
    state: State<AppState>,
    update: BatchAssetUpdate,
) -> Result<BatchUpdateReport, String> {
    state.with_library_mut(|connection, _| db::batch_update_assets(connection, update))
}

#[tauri::command]
pub fn move_assets_to_category(
    state: State<AppState>,
    ids: Vec<String>,
    category_id: Option<String>,
) -> Result<MoveResult, String> {
    state.move_assets_to_category(ids, category_id)
}

#[tauri::command]
pub fn move_category(
    state: State<AppState>,
    request: MoveCategoryRequest,
) -> Result<MoveResult, String> {
    state.move_category(request)
}

#[tauri::command]
pub fn undo_library_move(state: State<AppState>, token: String) -> Result<UndoMoveResult, String> {
    state.undo_library_move(&token)
}

#[tauri::command]
pub fn get_library_health(state: State<AppState>) -> Result<HealthSummary, String> {
    state.with_library(|connection, base_dir| db::health_summary(connection, base_dir))
}

#[tauri::command]
pub fn list_health_issues(
    state: State<AppState>,
    request: HealthIssueRequest,
) -> Result<Page<AssetCard>, String> {
    state.with_library(|connection, base_dir| db::list_health_issues(connection, base_dir, request))
}

#[tauri::command]
pub async fn check_all_share_links(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LinkCheckReport, String> {
    let token = state.begin_link_check()?;
    let result = link_checker::check_all(&state, &app, token.clone()).await;
    state.finish_link_check(&token);
    result
}

#[tauri::command]
pub fn cancel_share_link_check(state: State<AppState>) -> Result<bool, String> {
    state.cancel_link_check()
}

#[tauri::command]
pub async fn check_asset_share_link(
    state: State<'_, AppState>,
    id: String,
) -> Result<LinkCheckResult, String> {
    let token = state.begin_link_check()?;
    let result = link_checker::check_one(&state, &id, token.clone()).await;
    state.finish_link_check(&token);
    result
}

#[tauri::command]
pub fn get_translation_settings(state: State<AppState>) -> Result<TranslationSettings, String> {
    let (content_language, fab_auto_translate) = state.preferences()?;
    let preferences = state.translation_preferences()?;
    let baidu_configured = state.translation_service.configured("baidu");
    let deepseek_configured = state.translation_service.configured("deepseek");
    Ok(TranslationSettings {
        provider: preferences.provider_order[0].clone(),
        configured: baidu_configured || deepseek_configured,
        baidu_configured,
        deepseek_configured,
        usage: state.translation_service.usage()?,
        preferences,
        fab_auto_translate,
        content_language,
    })
}

#[tauri::command]
pub fn save_deepseek_credentials(state: State<AppState>, api_key: String) -> Result<(), String> {
    crate::translation_service::save_deepseek_key(&api_key)?;
    state.translation_service.clear_blocks();
    Ok(())
}
#[tauri::command]
pub fn delete_deepseek_credentials(state: State<AppState>) -> Result<(), String> {
    crate::translation_service::delete_deepseek_key()?;
    state.translation_service.clear_blocks();
    Ok(())
}
#[tauri::command]
pub fn save_translation_preferences(
    state: State<AppState>,
    preferences: crate::translation_service::TranslationPreferences,
) -> Result<(), String> {
    state.save_translation_preferences(preferences)
}
#[tauri::command]
pub fn clear_translation_cache(state: State<AppState>) -> Result<(), String> {
    state.translation_service.clear_cache()
}
#[tauri::command]
pub fn estimate_missing_translations(
    state: State<AppState>,
    ids: Vec<String>,
) -> Result<crate::translation_service::TranslationEstimate, String> {
    if ids.len() > 5000 {
        return Err("单次最多预估 5000 条素材".into());
    }
    state.with_library(|connection, _| {
        let mut total = crate::translation_service::TranslationEstimate {
            characters: 0,
            estimated_tokens: 0,
        };
        for id in ids.into_iter().collect::<std::collections::HashSet<_>>() {
            let asset = db::get_asset(connection, &id, false, "en")?;
            for (source, target) in [("en", "zh-CN"), ("zh-CN", "en")] {
                if let Some(request) =
                    bilingual::missing_request(&asset.localizations, source, target)
                {
                    let estimate = crate::translation_service::estimate(&request);
                    total.characters = total.characters.saturating_add(estimate.characters);
                    total.estimated_tokens = total
                        .estimated_tokens
                        .saturating_add(estimate.estimated_tokens);
                }
            }
        }
        Ok(total)
    })
}

#[tauri::command]
pub fn estimate_translation(
    request: TranslationRequest,
) -> crate::translation_service::TranslationEstimate {
    crate::translation_service::estimate(&request)
}

#[tauri::command]
pub fn save_translation_credentials(
    state: State<AppState>,
    app_id: String,
    secret_key: String,
) -> Result<(), String> {
    translation::save_credentials(&app_id, &secret_key)?;
    state.translation_service.clear_blocks();
    Ok(())
}

#[tauri::command]
pub fn delete_translation_credentials(state: State<AppState>) -> Result<(), String> {
    translation::delete_credentials()?;
    state.translation_service.clear_blocks();
    Ok(())
}

#[tauri::command]
pub fn set_fab_auto_translate(state: State<AppState>, enabled: bool) -> Result<(), String> {
    state.set_fab_auto_translate(enabled)
}

#[tauri::command]
pub fn set_content_language(state: State<AppState>, language: String) -> Result<(), String> {
    state.set_content_language(&language)
}

#[tauri::command]
pub async fn test_translation_service(
    state: State<'_, AppState>,
    provider: Option<String>,
) -> Result<TranslationTestResult, String> {
    translation::test_service(&state, provider.as_deref().unwrap_or("baidu")).await
}

#[tauri::command]
pub async fn translate_asset_fields(
    state: State<'_, AppState>,
    request: TranslationRequest,
) -> Result<TranslationPreview, String> {
    let terms =
        state.effective_translation_terms(&request.source_language, &request.target_language)?;
    translation::translate(&state, request, &terms).await
}

#[tauri::command]
pub fn list_translation_terms(
    state: State<AppState>,
    query: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
    origin: Option<String>,
    enabled: Option<bool>,
) -> Result<Vec<TranslationTerm>, String> {
    state.list_translation_terms(
        query.as_deref(),
        source_language.as_deref(),
        target_language.as_deref(),
        origin.as_deref(),
        enabled,
    )
}

#[tauri::command]
pub fn upsert_translation_term(
    state: State<AppState>,
    input: TranslationTermInput,
) -> Result<TranslationTerm, String> {
    state.upsert_translation_term(input)
}

#[tauri::command]
pub fn delete_translation_term(state: State<AppState>, id: String) -> Result<(), String> {
    state.delete_translation_term(&id)
}

#[tauri::command]
pub fn set_translation_term_enabled(
    state: State<AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    state.set_translation_term_enabled(&id, enabled)
}

#[tauri::command]
pub fn reset_translation_term_overrides(state: State<AppState>) -> Result<(), String> {
    state.reset_translation_terms()
}

#[tauri::command]
pub fn import_translation_terms(
    state: State<AppState>,
    path: String,
) -> Result<TranslationTermImportReport, String> {
    state.import_translation_terms(&PathBuf::from(path))
}

#[tauri::command]
pub fn export_translation_terms(state: State<AppState>, path: String) -> Result<(), String> {
    state.export_translation_terms(&PathBuf::from(path))
}

#[tauri::command]
pub fn list_reference_boards(state: State<AppState>) -> Result<Vec<ReferenceBoardSummary>, String> {
    state.with_library_mut(|connection, base_dir| {
        reference_boards::purge_deleted(connection, base_dir)?;
        reference_boards::list(connection)
    })
}

#[tauri::command]
pub fn create_reference_board(
    state: State<AppState>,
    name: String,
) -> Result<ReferenceBoardDetail, String> {
    state.with_library(|connection, _| reference_boards::create(connection, &name))
}

#[tauri::command]
pub fn rename_reference_board(
    state: State<AppState>,
    id: String,
    name: String,
) -> Result<(), String> {
    state.with_library(|connection, _| reference_boards::rename(connection, &id, &name))
}

#[tauri::command]
pub fn duplicate_reference_board(
    state: State<AppState>,
    id: String,
) -> Result<ReferenceBoardDetail, String> {
    state.with_library_mut(|connection, base_dir| {
        reference_boards::duplicate(connection, base_dir, &id)
    })
}

#[tauri::command]
pub fn delete_reference_board(state: State<AppState>, id: String) -> Result<(), String> {
    state.with_library_mut(|connection, base_dir| {
        reference_boards::delete(connection, base_dir, &id)
    })
}

#[tauri::command]
pub fn get_reference_board(
    state: State<AppState>,
    id: String,
) -> Result<ReferenceBoardDetail, String> {
    state.with_library(|connection, _| reference_boards::get(connection, &id))
}

#[tauri::command]
pub fn add_asset_images_to_board(
    state: State<AppState>,
    board_id: String,
    image_ids: Vec<String>,
    placement: ReferencePlacement,
) -> Result<ReferenceImageAddReport, String> {
    state.with_library_mut(|connection, base_dir| {
        reference_boards::add_asset_images(connection, base_dir, &board_id, &image_ids, &placement)
    })
}

#[tauri::command]
pub fn import_reference_images(
    state: State<AppState>,
    board_id: String,
    paths: Vec<String>,
    placement: ReferencePlacement,
) -> Result<ReferenceImageAddReport, String> {
    state.with_library_mut(|connection, base_dir| {
        reference_boards::import_paths(connection, base_dir, &board_id, &paths, &placement)
    })
}

#[tauri::command]
pub fn paste_reference_clipboard_image(
    state: State<AppState>,
    board_id: String,
    placement: ReferencePlacement,
) -> Result<ReferenceBoardItem, String> {
    state.with_library_mut(|connection, base_dir| {
        reference_boards::paste_clipboard(connection, base_dir, &board_id, &placement)
    })
}

#[tauri::command]
pub fn duplicate_reference_items(
    state: State<AppState>,
    board_id: String,
    item_ids: Vec<String>,
) -> Result<Vec<ReferenceBoardItem>, String> {
    state.with_library_mut(|connection, base_dir| {
        reference_boards::duplicate_items(connection, base_dir, &board_id, &item_ids)
    })
}

#[tauri::command]
pub fn save_reference_board_changes(
    state: State<AppState>,
    changes: ReferenceBoardChanges,
) -> Result<(), String> {
    state.with_library_mut(|connection, base_dir| {
        reference_boards::save_changes(connection, base_dir, changes)
    })
}

#[tauri::command]
pub fn get_reference_image_data(
    state: State<AppState>,
    item_id: String,
    thumbnail: bool,
) -> Result<String, String> {
    state.with_library(|connection, base_dir| {
        let relative = reference_boards::image_path(connection, &item_id, thumbnail)?;
        images::image_data_url(base_dir, &relative)
    })
}

#[tauri::command]
pub fn export_reference_board(
    state: State<AppState>,
    board_id: String,
    path: String,
    options: ReferenceExportOptions,
) -> Result<(), String> {
    state.with_library(|connection, base_dir| {
        reference_boards::export_png(
            connection,
            base_dir,
            &board_id,
            &PathBuf::from(path),
            &options,
        )
    })
}

#[tauri::command]
pub fn open_reference_window(
    app: AppHandle,
    state: State<AppState>,
    board_id: String,
) -> Result<(), String> {
    state.with_library(|connection, _| reference_boards::get(connection, &board_id).map(|_| ()))?;
    let mut window = app.get_webview_window("reference-board");
    if window.is_some() && state.reference_window_board_id()?.is_some() {
        let window = window.expect("窗口刚刚存在");
        window.set_focus().map_err(|e| e.to_string())?;
        return Err("已有参考板正在悬浮窗口中".into());
    }
    if window.is_none() {
        state.set_reference_window_board(None)?;
        window = Some(
            tauri::WebviewWindowBuilder::new(
                &app,
                "reference-board",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("栈藏 · 参考板")
            .inner_size(1100.0, 760.0)
            .min_inner_size(480.0, 320.0)
            .decorations(true)
            .resizable(true)
            .always_on_top(true)
            .visible(false)
            .center()
            .build()
            .map_err(|e| format!("重新创建悬浮参考板失败：{e}"))?,
        );
    }
    state.set_reference_window_board(Some(board_id.clone()))?;
    let window = window.expect("悬浮参考板窗口应已创建");
    window.set_decorations(true).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    app.emit_to("reference-board", "reference-board-load", &board_id)
        .map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    app.emit_to("main", "reference-window-opened", board_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_reference_window_board(
    window: tauri::WebviewWindow,
    state: State<AppState>,
) -> Result<String, String> {
    if window.label() != "reference-board" {
        return Err("该命令只能由悬浮参考板窗口调用".into());
    }
    state.reference_window_board()
}

#[tauri::command]
pub fn reference_window_ready(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "reference-board" {
        return Err("该命令只能由悬浮参考板窗口调用".into());
    }
    window.set_decorations(false).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn attach_reference_window(
    app: AppHandle,
    state: State<AppState>,
    board_id: String,
) -> Result<(), String> {
    app.emit_to("main", "reference-window-attached", &board_id)
        .map_err(|e| e.to_string())?;
    state.set_reference_window_board(None)?;
    if let Some(window) = app.get_webview_window("reference-board") {
        app.emit_to("reference-board", "reference-board-unload", &board_id)
            .map_err(|e| e.to_string())?;
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_reference_window_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("reference-board")
        .ok_or("悬浮参考板未打开")?;
    window.set_always_on_top(enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn is_reference_window_open(app: AppHandle, state: State<AppState>) -> Result<bool, String> {
    Ok(app.get_webview_window("reference-board").is_some()
        && state.reference_window_board_id()?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn knowledge_workspace_context_is_a_registered_browsing_module() {
        assert_eq!(validate_workspace_module("knowledge"), Ok(()));
        assert_eq!(
            validate_workspace_module("not-a-module"),
            Err("未知的浏览模块".into())
        );
    }

    #[test]
    fn browser_capture_workspace_context_is_a_registered_browsing_module() {
        assert_eq!(validate_workspace_module("browserCapture"), Ok(()));
    }
    #[test]
    fn parses_baidu_share_text() {
        let parsed =
            parse_share_text_value("链接：https://pan.baidu.com/s/demo?pwd=a1b2 提取码：zz99")
                .unwrap();
        assert_eq!(parsed.share_url, "https://pan.baidu.com/s/demo?pwd=a1b2");
        assert_eq!(parsed.extraction_code, "a1b2");
    }
}
