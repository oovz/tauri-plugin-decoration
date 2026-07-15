use tauri::Manager;
use tauri_plugin_decoration::WebviewWindowExt;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_decoration::init())
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| std::io::Error::other("missing compatibility window"))?;
            window.create_overlay_titlebar()?;
            window.restore_native_titlebar()?;

            #[cfg(target_os = "macos")]
            window.set_traffic_lights_inset(16.0, 20.0)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Tauri 2.9 compatibility app failed");
}
