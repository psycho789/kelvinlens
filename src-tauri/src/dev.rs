//! Debug builds only: lets automated UI checks drive the running app. If `KELVINLENS_DEV_EVAL`
//! names a file, its JavaScript runs in the main window each time the file changes, and the page
//! reports back through the `dev_log` command (printed to stdout). Release builds contain none of
//! this.

use std::time::Duration;

use tauri::{AppHandle, Manager};

pub fn start(app: &AppHandle) {
    let Ok(path) = std::env::var("KELVINLENS_DEV_EVAL") else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(300));
            let Ok(script) = std::fs::read_to_string(&path) else { continue };
            if script == last {
                continue;
            }
            last.clone_from(&script);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval(&script);
            }
        }
    });
}

#[tauri::command]
pub fn dev_log(message: String) {
    println!("[dev] {message}");
}
