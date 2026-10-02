#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--browser-capture-host") {
        if let Err(error) = dcc_asset_library_lib::run_browser_capture_host() {
            eprintln!("{error}");
        }
        return;
    }
    dcc_asset_library_lib::run();
}
