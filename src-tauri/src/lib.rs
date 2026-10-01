mod ai;
mod ai_store;
mod backup;
mod bilingual;
mod commands;
mod db;
mod fab;
mod file_digest;
mod glossary;
mod images;
mod importer;
mod knowledge;
mod knowledge_ai;
mod knowledge_commands;
mod knowledge_files;
mod link_checker;
mod media;
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
mod search_syntax;
mod state;
mod tasks;
mod translation;
mod translation_service;

use commands::*;
use state::AppState;
use tauri::Manager;

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
    ) || (backup
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
    if let Ok(entry) = keyring::Entry::new("DCCAssetLibrary.BaiduNetdisk", "default") {
        let _ = entry.delete_credential();
    }
    let state = AppState::initialize().unwrap_or_else(|error| panic!("无法启动素材库：{error}"));
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
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(|invoke| {
            let webview = invoke.message.webview();
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
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
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
