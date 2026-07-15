# Decoration example

This example demonstrates the normal integration path with one hidden
`main` window:

1. React mounts the document.
2. The frontend invokes `activate_custom_titlebar`.
3. The plugin prepares the exact document and commits native decoration after
   its stylesheet is ready.
4. The frontend sees `data-tauri-plugin-decoration-active` and shows the
   window.
5. If activation rejects or exceeds five seconds, Rust restores the native
   titlebar before showing the fallback.

The application titlebar is the actual Tauri drag region, uses a normal arrow
cursor, and disables text selection. Its left/right padding consumes plugin
clearance variables, so title text moves to the edge in fullscreen and returns
to its normal position afterward.

External applications apply the same `WebviewWindowExt` methods to any Tauri
`WebviewWindow`.

Unlike the published plugin's caret requirements, this test application pins
its Tauri Rust, JavaScript API, CLI, and lockfile graph to the supported 2.9.0
floor. It is a minimum-version test fixture, not dependency guidance for users.

From the repository root:

```sh
pnpm install --frozen-lockfile
pnpm test:frontend
pnpm --filter tauri-app build
pnpm example:dev
```

The workspace expects Node 24 and pnpm 11.9.0.
