mod ai;
mod ai_store;
mod asset;
mod automatic_backup;
mod backup;
mod bilingual;
mod browser_capture;
mod commands;
mod db;
mod downloader;
mod epic;
mod fab;
mod file_digest;
mod glossary;
mod images;
mod importer;
mod knowledge;
mod knowledge_ai;
mod knowledge_commands;
mod knowledge_files;
mod learning;
mod link_checker;
mod media;
mod media_access;
mod media_engine;
mod media_library;
mod model_access;
mod models;
mod note_clipboard;
mod note_export;
mod note_markdown;
mod note_moves;
mod note_package;
mod note_remote_images;
mod organization;
mod projects;
mod reference_boards;
mod reliability;
mod runtime;
mod search;
mod search_syntax;
mod state;
mod tasks;
mod translation;
mod translation_service;
mod upgrade_backup;
mod workflow;

use commands::*;
use state::AppState;
use tauri::Manager;

#[cfg(all(windows, debug_assertions))]
pub fn run_learning_native_qa(root: std::path::PathBuf) -> Result<(), String> {
    learning::native_qa::run(root)
}
#[cfg(all(windows, debug_assertions))]
pub fn resume_learning_native_qa(root: std::path::PathBuf) -> Result<(), String> {
    learning::native_qa::resume(root)
}
#[cfg(all(windows, debug_assertions))]
pub fn verify_learning_native_qa(root: std::path::PathBuf) -> Result<(), String> {
    learning::native_qa::verify(root)
}
#[cfg(all(windows, debug_assertions))]
pub fn run_learning_workspace_qa(root: std::path::PathBuf) -> Result<(), String> {
    learning::native_qa::run_workspace(root)
}

pub fn run_browser_capture_host() -> Result<(), String> {
    browser_capture::native_host()
}

pub fn connect_browser_capture_extension(extension_id: String) -> Result<String, String> {
    browser_capture::register_browser_capture_host(extension_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainWindowAction {
    Ignore,
    ExitApplication,
}

fn maintenance_command_allowed(command: &str, backup: bool) -> bool {
    matches!(
        command,
        "list_library_tasks"
            | "get_library_task"
            | "cancel_library_task"
            | "cancel_ai_session"
            | "exit_when_tasks_idle"
            | "attach_reference_window"
            | "open_external_url"
            | "set_knowledge_editing"
            | "finish_knowledge_close"
            | "stop_learning_playback"
            | "set_learning_viewport"
            | "get_upgrade_backup_status"
    ) || (backup
        && command != "list_reference_boards"
        && (command.starts_with("get_")
            || command.starts_with("list_")
            || command.starts_with("search_")
            || command.starts_with("select_")
            || command == "read_clipboard_text"))
}

fn main_window_action(label: &str, close_requested: bool) -> MainWindowAction {
    if label == "main" && close_requested {
        MainWindowAction::ExitApplication
    } else {
        MainWindowAction::Ignore
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _instance = match browser_capture::application_lock() {
        Ok(Some(file)) => file,
        Ok(None) => {
            if !std::env::args().any(|arg| arg == "--browser-capture-background") {
                let _ = browser_capture::request_open();
            }
            return;
        }
        Err(error) => panic!("无法建立应用运行锁：{error}"),
    };
    let mut context = tauri::generate_context!();
    if std::env::args().any(|arg| arg == "--browser-capture-background") {
        if let Some(window) = context
            .config_mut()
            .app
            .windows
            .iter_mut()
            .find(|window| window.label == "main")
        {
            window.visible = false;
        }
    }
    if let Ok(entry) = keyring::Entry::new("DCCAssetLibrary.BaiduNetdisk", "default") {
        let _ = entry.delete_credential();
    }
    let state = AppState::initialize().unwrap_or_else(|error| panic!("无法启动素材库：{error}"));
    application_builder(state, None)
        .run(context)
        .expect("运行 Tauri 应用失败");
}

fn application_builder(
    state: AppState,
    qa_root: Option<std::path::PathBuf>,
) -> tauri::Builder<tauri::Wry> {
    let isolated_qa = qa_root.is_some();
    tauri::Builder::default()
        .register_uri_scheme_protocol("dcc-media", |context, request| {
            let path = request.uri().path().trim_matches('/');
            let mut parts = path.split('/');
            let id = parts.next().unwrap_or_default();
            let variant = parts.next().unwrap_or("stream");
            let range = request
                .headers()
                .get("range")
                .and_then(|value| value.to_str().ok());
            let state = context.app_handle().state::<AppState>();
            if state.tasks.maintenance().is_some_and(|(_, backup)| !backup) {
                return tauri::http::Response::builder()
                    .status(503)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .body("素材库正在安全维护".as_bytes().to_vec())
                    .unwrap();
            }
            match state.with_library(|connection, base_dir| {
                if id == "note" {
                    knowledge_files::response(connection, base_dir, variant)
                } else if id == "library" && variant == "file" {
                    media_library::protocol_response(
                        connection,
                        base_dir,
                        parts.next().unwrap_or_default(),
                        range,
                    )
                } else if id == "library" && variant == "bundle" {
                    let entry_id = parts.next().unwrap_or_default();
                    let raw = parts.collect::<Vec<_>>().join("/");
                    let logical = percent_encoding::percent_decode_str(&raw).decode_utf8_lossy();
                    media_library::bundle_response(connection, base_dir, entry_id, &logical, range)
                } else {
                    media::protocol_response(connection, base_dir, id, variant, range)
                }
            }) {
                Ok(response) => response,
                Err(error) => tauri::http::Response::builder()
                    .status(404)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .body(error.into_bytes())
                    .unwrap(),
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            #[cfg(all(windows, debug_assertions))]
            if let Some(root) = qa_root {
                tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .data_directory(root.join("webview"))
                .title("栈藏 · 隔离学习工作区验收")
                .inner_size(1000.0, 700.0)
                .min_inner_size(980.0, 680.0)
                .build()?;
                return Ok(());
            }
            if let Err(error) = downloader::auth::clean_stale() {
                eprintln!("下载Cookie临时文件清理失败：{error}");
            }
            if let Err(error) = browser_capture::refresh_registered_host() {
                eprintln!("浏览器连接需要重新设置：{error}");
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let handle = handle.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        // This path bypasses IPC, so the capture pump independently guards maintenance.
                        if let Ok(result) = browser_capture::pump_browser_captures(
                            handle.clone(),
                            handle.state::<AppState>(),
                        ) {
                            let _ = tauri::Emitter::emit(&handle, "browser-capture-state", result);
                        }
                    })
                    .await;
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Calling `AppHandle::exit` from `Destroyed` re-enters window destruction on
            // Windows. That recursion terminates the process with 0xc00000fd (stack overflow).
            if main_window_action(
                window.label(),
                matches!(event, tauri::WindowEvent::CloseRequested { .. }),
            ) == MainWindowAction::ExitApplication
            {
                if window
                    .state::<AppState>()
                    .knowledge_editing
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                    }
                    let _ =
                        tauri::Emitter::emit(window.app_handle(), "knowledge-close-requested", ());
                    return;
                }
                if window.state::<AppState>().tasks.busy() {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                    }
                    let _ = tauri::Emitter::emit(
                        window.app_handle(),
                        "library-task-exit-requested",
                        (),
                    );
                    return;
                }
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                }
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = learning::native_player::exit_application(&app, false) {
                        let _ = tauri::Emitter::emit(&app, "learning-close-failed", error);
                    }
                });
            }
        })
        .invoke_handler(move |invoke| {
            #[cfg(all(windows, debug_assertions))]
            if isolated_qa
                && !learning::native_qa::workspace_command_allowed(invoke.message.command())
            {
                invoke
                    .resolver
                    .reject("隔离验收禁止访问凭据、联网、安装或用户文件操作");
                return true;
            }
            let _ = isolated_qa;
            let webview = invoke.message.webview();
            let interrupted = webview
                .state::<AppState>()
                .with_active_mut(|active| Ok(active.base_dir.join(backup::RESTORE_MARKER).exists()))
                .unwrap_or(true);
            if interrupted
                && !matches!(
                    invoke.message.command(),
                    "get_library_locations"
                        | "get_personalization_settings"
                        | "get_automatic_backup_policy"
                        | "get_upgrade_backup_status"
                        | "get_interrupted_restore"
                        | "recover_interrupted_restore"
                        | "export_diagnostics"
                        | "list_library_tasks"
                        | "get_task_warning"
                        | "set_knowledge_editing"
                        | "finish_knowledge_close"
                )
            {
                invoke
                    .resolver
                    .reject("上次恢复中断，请在设置中心的数据与素材库页确认恢复原库");
                return true;
            }
            if let Some((_, backup)) = webview.state::<AppState>().tasks.maintenance() {
                if !maintenance_command_allowed(invoke.message.command(), backup) {
                    invoke
                        .resolver
                        .reject("素材库正在安全维护，请等待完成后再修改；原库和已完成任务保留");
                    return true;
                }
            }
            let handler: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool> =
                Box::new(tauri::generate_handler![
                    commands::preview_external_video,
                    learning::commands::start_learning_playback,
                    learning::commands::control_learning_playback,
                    learning::commands::get_learning_playback,
                    learning::commands::set_learning_viewport,
                    learning::commands::stop_learning_playback,
                    learning::commands::ensure_learning_course,
                    learning::commands::get_learning_course,
                    learning::commands::list_learning_courses,
                    learning::commands::list_learning_series,
                    learning::commands::upsert_learning_series,
                    learning::commands::set_learning_series_members,
                    learning::commands::delete_learning_series,
                    learning::commands::import_learning_subtitle,
                    learning::commands::list_learning_subtitles,
                    learning::commands::transcript_page,
                    learning::commands::load_learning_subtitle,
                    learning::commands::prepare_course_analysis,
                    learning::commands::get_course_analysis,
                    commands::get_external_backup_summary,
                    commands::commit_external_video,
                    commands::relocate_external_video,
                    asset::commands::search_core_assets,
                    asset::commands::create_asset_relation,
                    asset::commands::delete_asset_relation,
                    asset::commands::get_asset_graph,
                    asset::commands::prepare_asset_analysis,
                    asset::commands::upsert_prompt_asset,
                    asset::commands::upsert_shot_asset,
                    asset::commands::get_shot_asset,
                    asset::commands::list_project_shot_assets,
                    asset::commands::get_prompt_asset,
                    asset::commands::trash_text_assets,
                    asset::commands::get_asset_analysis,
                    asset::commands::apply_asset_analysis_tags,
                    asset::commands::get_core_asset,
                    asset::commands::get_asset_scan_page,
                    asset::commands::set_core_asset_tags,
                    asset::commands::set_core_tag_color,
                    asset::commands::link_core_asset_project,
                    asset::commands::attach_core_asset_file,
                    asset::commands::create_core_asset_version,
                    asset::commands::delete_core_asset_version,
                    asset::commands::add_core_asset_source,
                    asset::commands::remove_core_asset_source,
                    downloader::service::resolve_download,
                    downloader::service::get_download_tools,
                    downloader::service::get_download_thumbnail,
                    downloader::service::get_download_settings,
                    downloader::service::save_download_settings,
                    downloader::service::get_download_cookie_status,
                    downloader::service::import_download_cookies,
                    downloader::service::delete_download_cookies,
                    downloader::service::list_download_tasks,
                    downloader::service::open_download_output,
                    tasks::pause_download_task,
                    tasks::resume_download_task,
                    epic::get_fab_ue_options,
                    epic::list_ue_project_targets,
                    epic::inspect_ue_project,
                    epic::open_fab_in_epic,
                    browser_capture::get_browser_capture_connection,
                    browser_capture::pump_browser_captures,
                    browser_capture::register_browser_capture_host,
                    browser_capture::list_browser_captures,
                    browser_capture::organize_browser_capture,
                    browser_capture::create_browser_capture_note,
                    knowledge_ai::generate_note_ai,
                    knowledge_commands::set_knowledge_editing,
                    knowledge_commands::finish_knowledge_close,
                    knowledge_commands::search_notes,
                    knowledge_commands::reorder_notes,
                    knowledge_commands::start_note_screenshot,
                    knowledge_commands::export_note_package,
                    knowledge_commands::export_note_draft,
                    knowledge_commands::read_note_markdown,
                    knowledge_commands::import_note_markdown,
                    knowledge_commands::export_note_markdown,
                    knowledge_commands::export_note_document,
                    knowledge_commands::get_note_export_html,
                    knowledge_commands::export_note_map,
                    knowledge_commands::import_note_package,
                    knowledge_commands::search_note_reference_targets,
                    knowledge_commands::get_journal_note,
                    knowledge_commands::purge_note,
                    knowledge_commands::get_note_references,
                    knowledge_commands::get_note_backlinks,
                    knowledge_commands::delete_empty_notebook,
                    knowledge_commands::import_note_attachment,
                    knowledge_commands::paste_note_clipboard_image,
                    knowledge_commands::open_note_attachment,
                    knowledge_commands::get_note,
                    knowledge_commands::save_note,
                    knowledge_commands::set_note_color,
                    knowledge_commands::get_note_structure_svg,
                    knowledge_commands::duplicate_note,
                    knowledge_commands::retry_note_remote_image,
                    knowledge_commands::write_note_clipboard_text,
                    knowledge_commands::get_note_appearance,
                    knowledge_commands::move_note_item,
                    knowledge_commands::undo_note_move,
                    knowledge_commands::save_note_appearance,
                    knowledge_commands::write_note_clipboard_fragment,
                    knowledge_commands::read_note_clipboard_fragment,
                    knowledge_commands::set_note_deleted,
                    knowledge_commands::list_note_revisions,
                    knowledge_commands::get_note_revision,
                    knowledge_commands::pin_note_revision,
                    knowledge_commands::restore_note_revision,
                    knowledge_commands::upsert_notebook,
                    knowledge_commands::list_notebooks,
                    ai::prepare_ai_request,
                    ai::get_ai_session,
                    ai::list_ai_sessions,
                    ai::rename_ai_session,
                    ai::delete_ai_session,
                    ai::generate_ai_session,
                    ai::cancel_ai_session,
                    ai::apply_ai_session,
                    ai::undo_ai_application,
                    ai::search_ai_candidates,
                    ai::run_ai_search,
                    get_library_meta,
                    tasks::list_library_tasks,
                    reliability::stash_note_recovery,
                    reliability::get_note_recovery,
                    reliability::clear_note_recovery,
                    reliability::restore_note_recovery_copy,
                    reliability::set_note_review,
                    reliability::list_note_reviews,
                    reliability::get_note_review,
                    reliability::get_interrupted_restore,
                    reliability::recover_interrupted_restore,
                    reliability::list_recovery_notes,
                    reliability::export_diagnostics,
                    reliability::add_project_checklist,
                    reliability::list_workflow_templates,
                    reliability::save_workflow_template,
                    reliability::delete_workflow_template,
                    automatic_backup::get_automatic_backup_policy,
                    automatic_backup::save_automatic_backup_policy,
                    upgrade_backup::get_upgrade_backup_status,
                    upgrade_backup::save_upgrade_backup_keep,
                    automatic_backup::get_automatic_backup_due,
                    automatic_backup::finish_automatic_backup,
                    tasks::get_task_warning,
                    tasks::get_library_task,
                    tasks::start_library_task,
                    tasks::cancel_library_task,
                    tasks::retry_library_task,
                    tasks::exit_when_tasks_idle,
                    tasks::queue_model_thumbnail_tasks,
                    tasks::get_model_thumbnail_task,
                    tasks::finish_model_thumbnail_task,
                    search_assets,
                    select_asset_ids,
                    get_asset,
                    upsert_asset,
                    delete_asset,
                    set_favorite,
                    upsert_category,
                    delete_category,
                    get_delete_impact,
                    delete_library_items,
                    list_trash,
                    restore_trash_batch,
                    purge_trash_batch,
                    empty_trash,
                    prepare_reference_cover_ids,
                    get_image_data,
                    import_asset_media,
                    delete_asset_media,
                    retry_asset_media,
                    reorder_asset_media,
                    set_asset_cover_media,
                    search_media_entries,
                    get_selection_labels,
                    select_media_entry_ids,
                    get_media_entry,
                    open_model_location,
                    export_model_package,
                    pending_model_thumbnails,
                    save_model_thumbnail,
                    report_model_thumbnail_failure,
                    import_media_entries,
                    collect_asset_previews_to_media,
                    add_media_images_to_board,
                    update_media_entry,
                    delete_media_entries,
                    batch_update_media_entries,
                    list_media_folders,
                    upsert_media_folder,
                    move_media_folder,
                    delete_media_folder,
                    link_media_assets,
                    unlink_media_assets,
                    link_project_media,
                    unlink_project_media,
                    copy_extraction_code,
                    open_share_link,
                    open_external_url,
                    preview_import,
                    import_assets,
                    batch_translate_missing_assets,
                    export_import_template,
                    export_backup,
                    restore_backup,
                    get_library_locations,
                    change_library,
                    forget_recent_library,
                    read_clipboard_text,
                    parse_share_text,
                    fetch_fab_metadata,
                    check_fab_url,
                    check_share_url,
                    batch_update_assets,
                    move_assets_to_category,
                    move_category,
                    undo_library_move,
                    get_library_health,
                    list_health_issues,
                    check_all_share_links,
                    cancel_share_link_check,
                    check_asset_share_link,
                    get_translation_settings,
                    save_deepseek_credentials,
                    delete_deepseek_credentials,
                    save_translation_preferences,
                    clear_translation_cache,
                    estimate_translation,
                    estimate_translation_cost,
                    estimate_missing_translations,
                    save_translation_credentials,
                    delete_translation_credentials,
                    set_fab_auto_translate,
                    set_content_language,
                    test_translation_service,
                    translate_asset_fields,
                    list_translation_terms,
                    upsert_translation_term,
                    delete_translation_term,
                    set_translation_term_enabled,
                    reset_translation_term_overrides,
                    import_translation_terms,
                    export_translation_terms,
                    list_reference_boards,
                    create_reference_board,
                    rename_reference_board,
                    duplicate_reference_board,
                    delete_reference_board,
                    get_reference_board,
                    add_asset_images_to_board,
                    import_reference_images,
                    paste_reference_clipboard_image,
                    duplicate_reference_items,
                    save_reference_board_changes,
                    get_reference_image_data,
                    export_reference_board,
                    open_reference_window,
                    get_reference_window_board,
                    reference_window_ready,
                    attach_reference_window,
                    set_reference_window_always_on_top,
                    is_reference_window_open,
                    get_personalization_settings,
                    save_global_preferences,
                    save_library_preferences,
                    get_workspace_context,
                    save_workspace_context,
                    reset_personalization_settings,
                    list_smart_collections,
                    create_smart_collection,
                    update_smart_collection,
                    duplicate_smart_collection,
                    delete_smart_collection,
                    reorder_smart_collections,
                    list_tags,
                    rename_tag,
                    merge_tags,
                    delete_tags,
                    delete_unused_tags,
                    list_projects,
                    get_project,
                    list_project_assets,
                    list_project_tasks,
                    get_project_unit_impact,
                    upsert_project,
                    archive_project,
                    delete_project,
                    add_project_assets,
                    update_project_assets,
                    remove_project_assets,
                    upsert_project_unit,
                    set_project_shot_video,
                    delete_project_unit,
                    reorder_project_units,
                    upsert_project_task,
                    delete_project_task,
                    move_project_task,
                    set_project_task_assets,
                    link_project_board,
                    unlink_project_board,
                    set_main_project_board,
                    upsert_project_path,
                    delete_project_path,
                    check_project_path,
                    open_project_path
                ]);
            handler(invoke)
        })
}

#[cfg(test)]
mod tests {
    use super::{main_window_action, maintenance_command_allowed, MainWindowAction};
    #[test]
    fn maintenance_preserves_read_only_browsing_and_rejects_changes() {
        assert!(maintenance_command_allowed("search_assets", true));
        assert!(maintenance_command_allowed("get_media_entry", true));
        assert!(!maintenance_command_allowed("delete_media_entries", true));
        assert!(!maintenance_command_allowed("upsert_asset", true));
        assert!(!maintenance_command_allowed("search_assets", false));
        assert!(maintenance_command_allowed("list_library_tasks", false));
        assert!(!maintenance_command_allowed("list_reference_boards", true));
        assert!(maintenance_command_allowed(
            "get_upgrade_backup_status",
            false
        ));
        assert!(maintenance_command_allowed(
            "get_upgrade_backup_status",
            true
        ));
        assert!(!maintenance_command_allowed(
            "save_upgrade_backup_keep",
            false
        ));
        assert!(!maintenance_command_allowed(
            "save_upgrade_backup_keep",
            true
        ));
    }

    #[test]
    fn destroyed_main_window_does_not_request_exit_again() {
        assert_eq!(main_window_action("main", false), MainWindowAction::Ignore);
        assert_eq!(
            main_window_action("main", true),
            MainWindowAction::ExitApplication
        );
        assert_eq!(
            main_window_action("reference-board", true),
            MainWindowAction::Ignore
        );
    }
}
