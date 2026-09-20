use crate::{
    backup, db, fab, images, importer, link_checker, models::*, organization, projects,
    reference_boards, state::AppState, translation,
};
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
) -> Result<CreativeProject, String> {
    state.with_library(|connection, _| {
        projects::get_project(
            connection,
            &id,
            content_language.as_deref().unwrap_or("zh-CN"),
            mark_opened.unwrap_or(true),
        )
    })
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
        let (_, fab_auto_translate) = state.preferences()?;
        let glossary_terms = state.effective_translation_terms("en", "zh-CN")?;
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
                    suggested_category_path: None,
                    actual_category_path: None,
                    duplicate_asset_id: None,
                    duplicate_source: None,
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
                    });
                    continue;
                }
            };
            let duplicate = state.with_library(|connection, _| {
                db::fab_duplicate_match(connection, &listing_id, None)
            })?;
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
                    });
                    continue;
                }
            };
            let suggested_category = metadata.suggested_category_path.join(" / ");
            let category_existed = state.with_library(|connection, _| {
                db::auto_category_path_exists(connection, &metadata.suggested_category_path)
            })?;
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
            let mut messages = Vec::new();
            if fab_auto_translate {
                emit_import_progress(
                    &app,
                    index + 1,
                    total,
                    &report,
                    &metadata.name,
                    "在线翻译中文信息",
                );
                let request = TranslationRequest {
                    source_language: "en".into(),
                    target_language: "zh-CN".into(),
                    fields: LocalizedAssetText {
                        name: metadata.name.clone(),
                        description: metadata.description.clone(),
                        tags: metadata.tags.clone(),
                        license: metadata.license.clone(),
                    },
                };
                match translation::translate(request, &glossary_terms).await {
                    Ok(preview) => {
                        apply_fab_translation(&mut localizations, &mut messages, preview)
                    }
                    Err(error) => messages.push(format!("自动翻译跳过：{error}")),
                }
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
                content_language: "en".into(),
            };
            match state.with_library_mut(|connection, base_dir| {
                db::upsert_asset(connection, base_dir, input)
            }) {
                Ok(_) => {
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
                        status: if has_warnings {
                            "warning".into()
                        } else {
                            "imported".into()
                        },
                        messages,
                        suggested_category_path: Some(suggested_category.clone()),
                        actual_category_path: Some(suggested_category),
                        duplicate_asset_id: None,
                        duplicate_source: None,
                    });
                }
                Err(error) => {
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

fn apply_fab_translation(
    localizations: &mut HashMap<String, LocalizedAssetText>,
    messages: &mut Vec<String>,
    preview: TranslationPreview,
) {
    messages.extend(
        preview
            .warnings
            .into_iter()
            .map(|warning| format!("自动翻译：{warning}")),
    );
    let translated = preview.fields;
    if !translated.name.trim().is_empty()
        || !translated.description.trim().is_empty()
        || !translated.tags.is_empty()
        || !translated.license.trim().is_empty()
    {
        localizations.insert("zh-CN".into(), translated);
    } else if messages.is_empty() {
        messages.push("自动翻译未返回可保存的中文内容".into());
    }
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
    state: State<'_, AppState>,
    request: TranslationRequest,
) -> Result<TranslationPreview, String> {
    let terms =
        state.effective_translation_terms(&request.source_language, &request.target_language)?;
    translation::translate(request, &terms).await
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
    fn parses_baidu_share_text() {
        let parsed =
            parse_share_text_value("链接：https://pan.baidu.com/s/demo?pwd=a1b2 提取码：zz99")
                .unwrap();
        assert_eq!(parsed.share_url, "https://pan.baidu.com/s/demo?pwd=a1b2");
        assert_eq!(parsed.extraction_code, "a1b2");
    }

    #[test]
    fn keeps_partial_batch_translation_and_reports_failed_fields() {
        let mut localizations = HashMap::new();
        let mut messages = Vec::new();
        apply_fab_translation(
            &mut localizations,
            &mut messages,
            TranslationPreview {
                fields: LocalizedAssetText {
                    name: "未来城市".into(),
                    ..Default::default()
                },
                warnings: vec!["description：请求超时".into()],
                failed_fields: vec!["description".into()],
                character_count: 12,
                ..Default::default()
            },
        );
        assert_eq!(localizations["zh-CN"].name, "未来城市");
        assert!(messages[0].contains("请求超时"));
    }
}
