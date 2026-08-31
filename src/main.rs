#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desktop;
mod error;
mod model;
mod service;
mod state;
mod theme_assets;

use crate::state::AppState;

fn main() {
    // WebView2 inherits these process-scoped flags. The UI uses Tauri IPC and local
    // custom protocols, so DNS, proxies and background update traffic are unnecessary.
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--disable-background-networking --disable-component-update --disable-default-apps --disable-features=msSmartScreenProtection --no-first-run --proxy-server=http://127.0.0.1:9 --proxy-bypass-list=<-loopback> --host-resolver-rules=\"MAP * 0.0.0.0\"",
    );
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            desktop::choose_workspace,
            desktop::scan_workspace,
            desktop::read_document,
            desktop::save_document,
            desktop::render_preview,
            desktop::inline_preview_images,
            desktop::batch_replace,
            desktop::choose_export_folder,
            desktop::export_documents,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Markdown PDF Desktop");
}
