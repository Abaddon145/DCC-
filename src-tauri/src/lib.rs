mod backup;
mod commands;
mod db;
mod fab;
mod images;
mod importer;
mod link_checker;
mod models;
mod organization;
mod reference_boards;
mod state;
mod translation;

use commands::*;
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::initialize().unwrap_or_else(|error| panic!("无法启动素材库：{error}"));
    tauri::Builder::default()
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
            get_asset,
            upsert_asset,
            delete_asset,
            set_favorite,
            upsert_category,
            delete_category,
            get_image_data,
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
            delete_unused_tags
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
