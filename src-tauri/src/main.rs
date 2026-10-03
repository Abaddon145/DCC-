#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--register-browser-capture-host") {
        match dcc_asset_library_lib::connect_browser_capture_extension(
            std::env::args().nth(2).unwrap_or_default(),
        ) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--browser-capture-host") {
        if let Err(error) = dcc_asset_library_lib::run_browser_capture_host() {
            eprintln!("{error}");
        }
        return;
    }
    dcc_asset_library_lib::run();
}
