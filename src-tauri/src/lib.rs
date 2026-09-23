mod backup;
mod commands;
mod db;
mod fab;
mod glossary;
mod images;
mod importer;
mod link_checker;
mod media;
mod media_library;
mod models;
mod organization;
mod projects;
mod reference_boards;
mod search_syntax;
mod state;
mod translation;

use commands::*;
use state::AppState;
use tauri::Manager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainWindowAction {
    Ignore,
    ExitApplication,
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
            match state.with_library(|connection, base_dir| {
                if id == "library" && variant == "file" {
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
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_library_meta,
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
            select_media_entry_ids,
            get_media_entry,
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
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}

#[cfg(test)]
mod tests {
    use super::{main_window_action, MainWindowAction};

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
