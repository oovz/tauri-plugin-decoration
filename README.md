# tauri-plugin-decoration

[![crates.io](https://img.shields.io/crates/v/tauri-plugin-decoration.svg)](https://crates.io/crates/tauri-plugin-decoration)

Native window controls for custom Tauri v2 titlebars.

| Platform | Controls |
|---|---|
| Windows 10 | HTML window controls |
| Windows 11 | HTML window controls with native Snap Layout |
| macOS | Native AppKit traffic lights |
| Linux | GTK-themed HTML controls for supported Wayland sessions |

The plugin requires Tauri 2.9.0 or a later compatible Tauri v2 release and Rust 1.77.2. JavaScript and CSS are embedded in the Rust crate, so applications do not install a companion npm package. Applications building with the minimum Rust version should retain an MSRV-compatible `Cargo.lock`.

<video src="https://raw.githubusercontent.com/oovz/tauri-plugin-decoration/main/assets/windows.mp4" controls muted width="100%"></video>

<video src="https://raw.githubusercontent.com/oovz/tauri-plugin-decoration/main/assets/mac.mp4" controls muted width="100%"></video>

## Quick Start

Add the dependency to `Cargo.toml`:

```toml
[dependencies]
tauri = "2.9.0"
tauri-plugin-decoration = "2.1.4"
```

Register the plugin in `src-tauri/src/main.rs` (or `lib.rs`) and add a command to activate the custom titlebar:

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

Call the command from the frontend once it mounts:

```ts
import { invoke } from "@tauri-apps/api/core";

await invoke("activate_custom_titlebar");
```

> [!IMPORTANT]
> Windows should start hidden with native decorations enabled. Show the window only after `data-tauri-plugin-decoration-active` is set on the document element. Use a timeout (e.g. 5 seconds) to call `restore_native_titlebar()` if activation fails.

## Tauri Configuration

Configure CSP rules and window behavior in `tauri.conf.json`:

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

Add permissions to your capability file:

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

## Titlebar Layout and CSS

Mark non-interactive titlebar areas with `data-tauri-drag-region`. Interactive elements like buttons and inputs must sit outside this region.

Use the provided CSS variables to clear window controls:

```css
.titlebar-content {
  padding-left: max(8px, var(--tauri-plugin-decoration-left-clearance, 0px));
  padding-right: max(8px, var(--tauri-plugin-decoration-right-clearance, 0px));
  cursor: default;
  user-select: none;
}
```

Clearances collapse to `0px` in fullscreen mode.

Since the plugin overlays controls without altering the document scrolling behavior, a fixed titlebar will not automatically bound the scrolling model. To keep the scrollbar below the titlebar, lock the main document viewport and scroll the body container:

```css
:root {
  --app-titlebar-height: 32px;
}

html,
body,
#root {
  height: 100%;
  margin: 0;
}

body,
.app-shell {
  overflow: hidden;
}

.app-shell {
  height: 100%;
}

.app-content {
  height: calc(100% - var(--app-titlebar-height));
  margin-top: var(--app-titlebar-height);
  overflow-y: auto;
}

.app-shell[data-titlebar-mode="native"] {
  --app-titlebar-height: 0px;
}
```

The example uses `32px` for `--app-titlebar-height`, matching Windows and Linux HTML strips. macOS uses AppKit's native geometry, control insets are configured independently via `set_traffic_lights_inset`.

## Linux Support

HTML controls are supported on Wayland sessions in:
- Ubuntu 24.04 LTS with GNOME/Mutter or KDE/KWin
- Fedora 44 with GNOME/Mutter or KDE/KWin

The runtime requires GTK 3.24 or newer, WebKitGTK 4.1 version 2.40 or newer, and a live `GdkWaylandDisplay`. System packages must be installed as described in the [Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux).

## macOS APIs

- `set_traffic_lights_inset(x, y)`: Positions the traffic lights. AppKit vertically centers buttons within the height adjusted by `y`.
- `set_window_level(level)`: Sets the AppKit `NSWindowLevel`.
- `make_transparent()`: Requires the `macos-transparency` feature:

```toml
tauri-plugin-decoration = { version = "2.1.4", features = ["macos-transparency"] }
```

> [!WARNING]
> The `macos-transparency` feature uses private AppKit APIs and will lead to App Store rejection. Other positioning and level APIs are safe to use.

### macOS WebKit focus behavior

macOS WebKit fires `blur` with `relatedTarget: null` on `mousedown` (Windows/Linux Chromium focus the new target synchronously). Dropdowns that close on `onBlur` by checking `relatedTarget` will unmount before `click` fires. Guard against `null`:

```tsx
onBlur={(e) => {
  const related = e.relatedTarget as Node | null;
  if (related && !e.currentTarget.contains(related)) setOpen(false);
}}
```

## Example

Refer to the [example app](examples/tauri-app) for a complete setup demonstrating custom titlebar activation and fallback.
