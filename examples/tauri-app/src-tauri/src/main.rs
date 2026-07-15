// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::WebviewWindow;
use tauri_plugin_decoration::WebviewWindowExt;

#[tauri::command]
fn activate_custom_titlebar(window: WebviewWindow) -> Result<(), String> {
    window
        .create_overlay_titlebar()
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "macos")]
    window
        .set_traffic_lights_inset(16.0, 20.0)
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[tauri::command]
fn show_native_fallback(window: WebviewWindow) -> Result<(), String> {
    window
        .restore_native_titlebar()
        .map_err(|error| format!("failed to restore native decorations: {error}"))?;
    window
        .show()
        .map_err(|error| format!("failed to show native fallback: {error}"))
}

fn main() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_decoration::init())
        .invoke_handler(tauri::generate_handler![
            activate_custom_titlebar,
            show_native_fallback
        ])
        .run(tauri::generate_context!())
}
