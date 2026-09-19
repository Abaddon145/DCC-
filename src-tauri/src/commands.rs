use crate::{
    backup, baidu_netdisk, db, fab, images, importer, link_checker, models::*, organization,
    reference_boards, state::AppState, translation,
};
use arboard::Clipboard;
use std::{collections::HashMap, path::PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;

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
    preferences: LibraryPreferences,
) -> Result<LibraryPreferences, String> {
    state.with_library(|connection, _| {
        organization::save_library_preferences(connection, preferences)
    })
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
pub fn prepare_baidu_save_tasks(
    state: State<AppState>,
    ids: Vec<String>,
) -> Result<Vec<BaiduSaveTask>, String> {
    state.with_library(|connection, _| db::prepare_baidu_save_tasks(connection, &ids))
}

#[tauri::command]
pub fn get_baidu_netdisk_settings() -> Result<BaiduNetdiskSettings, String> {
    baidu_netdisk::get_settings()
}

#[tauri::command]
pub fn save_baidu_netdisk_credentials(
    app_id: String,
    app_key: String,
    secret_key: String,
) -> Result<BaiduNetdiskSettings, String> {
    baidu_netdisk::save_credentials(&app_id, &app_key, &secret_key)?;
    baidu_netdisk::get_settings()
}

#[tauri::command]
pub fn delete_baidu_netdisk_credentials() -> Result<(), String> {
    baidu_netdisk::delete_credentials()
}

#[tauri::command]
pub fn disconnect_baidu_netdisk() -> Result<BaiduNetdiskSettings, String> {
    baidu_netdisk::disconnect()
}

#[tauri::command]
pub async fn start_baidu_netdisk_authorization() -> Result<BaiduDeviceAuthorization, String> {
    baidu_netdisk::start_authorization().await
}

#[tauri::command]
pub async fn complete_baidu_netdisk_authorization() -> Result<BaiduNetdiskSettings, String> {
    baidu_netdisk::complete_authorization().await
}

#[tauri::command]
pub async fn list_baidu_netdisk_folders(path: String) -> Result<Vec<BaiduNetdiskFolder>, String> {
    baidu_netdisk::list_folders(&path).await
}

#[tauri::command]
pub fn set_baidu_netdisk_default_path(path: String) -> Result<BaiduNetdiskSettings, String> {
    baidu_netdisk::set_default_path(&path)
}

#[tauri::command]
pub async fn transfer_baidu_save_task(
    state: State<'_, AppState>,
    asset_id: String,
    destination: String,
) -> Result<BaiduTransferResult, String> {
    let task = state.with_library(|connection, _| {
        db::prepare_baidu_save_tasks(connection, std::slice::from_ref(&asset_id))?
            .into_iter()
            .next()
            .ok_or_else(|| "素材没有可转存的百度网盘链接".to_string())
    })?;
    baidu_netdisk::transfer(&task, &destination).await
}

#[tauri::command]
pub async fn query_baidu_transfer_task(
    asset_id: String,
    task_id: String,
) -> Result<BaiduTransferResult, String> {
    baidu_netdisk::query_transfer(&asset_id, &task_id).await
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
) -> Result<ImportReport, String> {
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
                });
                continue;
            }
            let already_exists = state.with_library(|connection, _| connection.query_row("SELECT EXISTS(SELECT 1 FROM assets WHERE deleted_at IS NULL AND source_url=?1)", [&row.fab_url], |value| value.get::<_, bool>(0)).map_err(|error| error.to_string()))?;
            if already_exists {
                report.skipped += 1;
                report.rows.push(ImportRowResult {
                    row: row.row,
                    name: row.fab_url.clone(),
                    status: "skipped".into(),
                    messages: vec!["Fab 素材已存在".into()],
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
                    });
                    continue;
                }
            };
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
            let mut localizations = HashMap::new();
            localizations.insert(
                "en".into(),
                LocalizedAssetText {
                    name: metadata.name.clone(),
                    description: metadata.description.clone(),
                    tags: metadata.tags.clone(),
                    license: metadata.license.clone(),
                },
            );
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
                license: metadata.license.clone(),
                share_url: parsed_share.share_url,
                extraction_code: parsed_share.extraction_code,
                favorite: false,
                images,
                localizations,
                content_language: "en".into(),
            };
            match state.with_library_mut(|connection, base_dir| {
                db::upsert_asset(connection, base_dir, input)
            }) {
                Ok(_) => {
                    report.imported += 1;
                    let mut messages = Vec::new();
                    if let Some(warning) = metadata.image_warning {
                        messages.push(warning);
                    }
                    report.rows.push(ImportRowResult {
                        row: row.row,
                        name: metadata.name,
                        status: if messages.is_empty() {
                            "imported".into()
                        } else {
                            "warning".into()
                        },
                        messages,
                    });
                }
                Err(error) => {
                    report.failed += 1;
                    report.rows.push(ImportRowResult {
                        row: row.row,
                        name: metadata.name,
                        status: "error".into(),
                        messages: vec![error],
                    });
                }
            }
        }
        emit_import_progress(&app, total, total, &report, "", "完成");
        return Ok(report);
    }
    state.with_library_mut(|connection, base_dir| {
        importer::commit(connection, base_dir, &PathBuf::from(path), mapping)
    })
}

fn emit_import_progress(
    app: &AppHandle,
    current: usize,
    total: usize,
    report: &ImportReport,
    current_name: &str,
    phase: &str,
) {
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
    if app.get_webview_window("reference-board").is_some()
        && state.reference_window_board_id()?.is_some()
    {
        return Err("请先收回悬浮参考板，再恢复素材库".into());
    }
    state.with_active_mut(|active| backup::restore_library(active, &PathBuf::from(path)))
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
    Ok(TranslationSettings {
        provider: "baidu".into(),
        configured: translation::is_configured(),
        fab_auto_translate,
        content_language,
    })
}

#[tauri::command]
pub fn save_translation_credentials(app_id: String, secret_key: String) -> Result<(), String> {
    translation::save_credentials(&app_id, &secret_key)
}

#[tauri::command]
pub fn delete_translation_credentials() -> Result<(), String> {
    translation::delete_credentials()
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
pub async fn test_translation_service() -> Result<TranslationTestResult, String> {
    translation::test_service().await
}

#[tauri::command]
pub async fn translate_asset_fields(
    request: TranslationRequest,
) -> Result<TranslationPreview, String> {
    translation::translate(request).await
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
    fn parses_baidu_share_text() {
        let parsed =
            parse_share_text_value("链接：https://pan.baidu.com/s/demo?pwd=a1b2 提取码：zz99")
                .unwrap();
        assert_eq!(parsed.share_url, "https://pan.baidu.com/s/demo?pwd=a1b2");
        assert_eq!(parsed.extraction_code, "a1b2");
    }
}
