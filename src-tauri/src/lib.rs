mod backup;
mod collections;
mod commands;
mod db;
mod fab;
mod glossary;
mod images;
mod importer;
mod link_checker;
mod media;
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
                media::protocol_response(connection, base_dir, id, variant, range)
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
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
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
            list_manual_collections,
            upsert_manual_collection,
            move_manual_collection,
            delete_manual_collection,
            add_collection_assets,
            remove_collection_assets,
            reorder_collection_assets,
            batch_set_asset_rating,
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
