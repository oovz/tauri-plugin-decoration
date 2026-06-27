// The show_snap_overlay function has been removed.
// On Windows 11, the native Snap Layout flyout is now handled
// automatically by the Rust snap module's child HWND overlay,
// which returns HTMAXBUTTON from WM_NCHITTEST.
// No JavaScript invocation is needed.
