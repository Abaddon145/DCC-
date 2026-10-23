#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(all(windows, debug_assertions))]
    if std::env::args().nth(1).as_deref() == Some("--learning-workspace-qa") {
        if let Err(error) = dcc_asset_library_lib::run_learning_workspace_qa(
            std::env::args().nth(2).unwrap_or_default().into(),
        ) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(all(windows, debug_assertions))]
    if std::env::args().nth(1).as_deref() == Some("--learning-native-qa-verify") {
        if let Err(error) = dcc_asset_library_lib::verify_learning_native_qa(
            std::env::args().nth(2).unwrap_or_default().into(),
        ) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(all(windows, debug_assertions))]
    if std::env::args().nth(1).as_deref() == Some("--learning-native-qa") {
        if let Err(error) = dcc_asset_library_lib::run_learning_native_qa(
            std::env::args().nth(2).unwrap_or_default().into(),
        ) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
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
