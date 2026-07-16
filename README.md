# tauri-plugin-decoration

[![crates.io](https://img.shields.io/crates/v/tauri-plugin-decoration.svg)](https://crates.io/crates/tauri-plugin-decoration)

Native window controls for custom Tauri v2 titlebars.

| Platform | Controls |
|---|---|
| Windows 10 | HTML window controls |
| Windows 11 | HTML window controls with native Snap Layout |
| macOS | Native AppKit traffic lights |
| Linux | GTK-themed HTML controls for supported Wayland sessions |

The plugin requires Tauri 2.9.0 or a later compatible Tauri v2 release and
Rust 1.77.2. Its JavaScript and CSS are embedded in the Rust crate, so
applications do not install a companion npm package. Applications building
with the minimum Rust version should retain an MSRV-compatible `Cargo.lock`.

<video src="https://raw.githubusercontent.com/oovz/tauri-plugin-decoration/main/assets/windows.mp4" controls muted width="100%"></video>

<video src="https://raw.githubusercontent.com/oovz/tauri-plugin-decoration/main/assets/mac.mp4" controls muted width="100%"></video>

## Quick start

Add the plugin and Tauri to your application:

```toml
[dependencies]
tauri = "2.9.0"
tauri-plugin-decoration = "2.1.0"
```

Register the plugin before Tauri creates any WebViews, then expose an
application command for titlebar activation:

```rust
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

fn main() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_decoration::init())
        .invoke_handler(tauri::generate_handler![activate_custom_titlebar])
        .run(tauri::generate_context!())
}
```

Invoke the command after the frontend mounts:

```ts
import { invoke } from "@tauri-apps/api/core";

await invoke("activate_custom_titlebar");
```

> [!IMPORTANT]
> Start opted-in windows hidden and keep native decorations enabled. Wait for
> `data-tauri-plugin-decoration-active` before showing the window. Bound the
> wait with an application timeout; if the marker never appears, call
> `restore_native_titlebar()` before `show()`. The example uses five seconds.

## Tauri configuration

Enable Tauri's global API for the embedded controls and allow the plugin's
stylesheet protocol in your CSP. Linux theme icons also need `data:` in
`img-src`.

```json
{
  "app": {
    "withGlobalTauri": true,
    "security": {
      "csp": {
        "default-src": "'self' customprotocol: asset:",
        "connect-src": "ipc: http://ipc.localhost",
        "img-src": "'self' asset: http://asset.localhost data:",
        "style-src": "'self' tauri-plugin-decoration: http://tauri-plugin-decoration.localhost https://tauri-plugin-decoration.localhost",
        "style-src-elem": "'self' tauri-plugin-decoration: http://tauri-plugin-decoration.localhost https://tauri-plugin-decoration.localhost"
      }
    },
    "windows": [
      {
        "label": "main",
        "decorations": true,
        "visible": false,
        "titleBarStyle": "Overlay",
        "hiddenTitle": true
      }
    ]
  }
}
```

`titleBarStyle` and `hiddenTitle` configure the macOS titlebar. Set window
flags such as `resizable`, `maximizable`, `minimizable`, and `closable` before
activation.

The following capability covers all built-in controls and the hidden-window
reveal flow:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "decoration",
  "windows": ["main"],
  "permissions": [
    "core:window:allow-close",
    "core:window:allow-is-fullscreen",
    "core:window:allow-is-maximized",
    "core:window:allow-minimize",
    "core:window:allow-show",
    "core:window:allow-start-dragging",
    "core:window:allow-internal-toggle-maximize",
    "core:window:allow-toggle-maximize",
    "decoration:default"
  ]
}
```

`decoration:default` permits the plugin's activation acknowledgement. Remove
window-action permissions only when the corresponding controls are not
available to the user. Scope the capability to each local, primary WebView by
window label.

## Titlebar content

Put `data-tauri-drag-region` on the non-interactive part of your application
titlebar. Keep buttons, links, and inputs outside the drag region.

Use the clearance variables to keep content away from native and HTML window
controls:

```css
.titlebar-content {
  padding-left: max(8px, var(--tauri-plugin-decoration-left-clearance, 0px));
  padding-right: max(8px, var(--tauri-plugin-decoration-right-clearance, 0px));
  cursor: default;
  user-select: none;
}
```

Both clearances become zero in fullscreen and return when the window leaves
fullscreen.

## Linux

Linux controls are supported on these Wayland sessions:

- Ubuntu 24.04 LTS with GNOME/Mutter or KDE/KWin
- Fedora 44 with GNOME/Mutter or KDE/KWin

The runtime requires GTK 3.24 or newer, WebKitGTK 4.1 version 2.40 or newer,
and a live `GdkWaylandDisplay`. GTK supplies the control order and icons when
the titlebar activates. See the
[Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux)
for system packages.

## macOS APIs

`set_traffic_lights_inset(x, y)` positions the native traffic lights. `x` is
the first button's horizontal position. `y` adds height to the titlebar
container, and AppKit centers the buttons vertically within it.

`set_window_level(level)` accepts an AppKit `NSWindowLevel` value.

`make_transparent()` requires the `macos-transparency` feature:

```toml
tauri-plugin-decoration = { version = "2.1.0", features = ["macos-transparency"] }
```

> [!WARNING]
> This feature enables Tauri's private macOS API and prevents App Store
> acceptance. Traffic-light positioning and window-level APIs do not require
> it.

## Example

The [example app](examples/tauri-app) contains a complete hidden-window
activation and native-titlebar fallback.
