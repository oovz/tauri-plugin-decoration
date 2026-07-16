use crate::{
    dispatcher::{dispatch_sync, DispatchError},
    frontend::{FrontendOptions, Platform},
    lifecycle::{ActivationDecision, DecorationState, FrontendTarget, ReadinessDecision},
};
use anyhow::anyhow;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Error, Manager, RunEvent, Runtime, WebviewWindow, WindowEvent};

mod dispatcher;
mod frontend;
mod lifecycle;
mod protocol;

#[cfg(target_os = "macos")]
mod traffic;

#[cfg(any(target_os = "linux", test))]
mod linux;

#[cfg(target_os = "windows")]
mod snap;

#[cfg(any(target_os = "windows", test))]
mod snap_state;

#[cfg(any(target_os = "macos", test))]
mod traffic_state;

/// Native decoration extensions for a Tauri [`WebviewWindow`].
///
/// A decorated native window must use its primary, same-label webview for the
/// embedded decoration runtime. Sibling webviews and remote content are not
/// part of the supported command boundary.
pub trait WebviewWindowExt {
    fn create_overlay_titlebar(&self) -> Result<&WebviewWindow, Error>;
    fn restore_native_titlebar(&self) -> Result<&WebviewWindow, Error>;
    #[cfg(target_os = "macos")]
    fn set_traffic_lights_inset(&self, x: f32, y: f32) -> Result<&WebviewWindow, Error>;
    #[cfg(target_os = "macos")]
    fn make_transparent(&self) -> Result<&WebviewWindow, Error>;
    #[cfg(target_os = "macos")]
    fn set_window_level(&self, level: u32) -> Result<&WebviewWindow, Error>;
}

impl WebviewWindowExt for WebviewWindow {
    /// Create a custom titlebar overlay.
    /// This will remove the default titlebar and create a draggable area for the titlebar.
    /// On Windows, it will also create custom window controls.
    fn create_overlay_titlebar(&self) -> Result<&WebviewWindow, Error> {
        let window = self.clone();
        dispatch_webview(self, move || begin_activation(&window))?;
        Ok(self)
    }

    /// Cancel any pending or active custom decoration for this native window
    /// and restore the operating system titlebar.
    fn restore_native_titlebar(&self) -> Result<&WebviewWindow, Error> {
        let window = self.clone();
        dispatch_webview(self, move || cancel_activation(&window))?;
        Ok(self)
    }

    /// Position the macOS traffic-light buttons (close/minimize/zoom).
    ///
    /// The two parameters control different things because the buttons are
    /// native OS controls positioned within an AppKit titlebar container:
    ///
    /// - `x` — **horizontal position**, in points from the left edge of the
    ///   window's content. The first button is placed at `x`, and the other
    ///   buttons preserve AppKit's measured native spacing relative to it.
    ///   This is a direct offset.
    ///
    /// - `y` — **titlebar container height**, in points, added on top of the
    ///   button height (`container_height = button_height + y`). The buttons
    ///   are then **vertically centered** within that container. This is an
    ///   *indirect* control: increasing `y` makes the container taller and
    ///   pushes the centered buttons down from the window's top edge;
    ///   decreasing it pulls them up. It does *not* offset the buttons off
    ///   center within the container.
    ///
    /// As a side effect, this method exposes the right edge of the last
    /// button to the webview as both the legacy
    /// `--decoration-traffic-light-left` property and the platform-neutral
    /// `--tauri-plugin-decoration-left-clearance` property so app content can
    /// avoid overlapping the cluster.
    ///
    /// This is only available on macOS.
    #[cfg(target_os = "macos")]
    fn set_traffic_lights_inset(&self, x: f32, y: f32) -> Result<&WebviewWindow, Error> {
        let window = self.clone();
        dispatch_webview(self, move || {
            traffic::set_inset(&window, x.into(), y.into())
        })?;
        Ok(self)
    }

    /// Make the live WKWebView and NSWindow transparent.
    ///
    /// Unlike Tauri's creation-time `transparent` option, this changes an
    /// existing webview. Enable the `macos-transparency` Cargo feature to opt
    /// into Wry's private macOS transparency API.
    #[cfg(target_os = "macos")]
    fn make_transparent(&self) -> Result<&WebviewWindow, Error> {
        #[cfg(feature = "macos-transparency")]
        {
            let window = self.clone();
            dispatch_webview(self, move || traffic::make_transparent(&window))?;
            Ok(self)
        }

        #[cfg(not(feature = "macos-transparency"))]
        {
            Err(anyhow!(
                "make_transparent requires the tauri-plugin-decoration macos-transparency feature"
            )
            .into())
        }
    }

    /// Set the window level.
    /// This will set the window level to the specified value.
    /// NSWindowLevel values can be found [here](https://developer.apple.com/documentation/appkit/nswindowlevel?language=objc).
    /// This is only available on macOS.
    #[cfg(target_os = "macos")]
    fn set_window_level(&self, level: u32) -> Result<&WebviewWindow, Error> {
        let window = self.clone();
        dispatch_webview(self, move || traffic::set_window_level(&window, level))?;
        Ok(self)
    }
}

fn decoration_state<R: Runtime, M: Manager<R>>(
    manager: &M,
) -> Result<tauri::State<'_, DecorationState>, Error> {
    manager.try_state::<DecorationState>().ok_or_else(|| {
        anyhow!(
            "tauri-plugin-decoration is not initialized; register init() statically before webviews are created"
        )
        .into()
    })
}

fn dispatch_webview<R, T, F>(window: &WebviewWindow<R>, action: F) -> Result<T, Error>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce() -> Result<T, Error> + Send + 'static,
{
    let main_thread = decoration_state(window)?.main_thread();
    let app = window.app_handle().clone();
    dispatch_sync(main_thread, move |job| app.run_on_main_thread(job), action).map_err(|error| {
        match error {
            DispatchError::Schedule(error) | DispatchError::Action(error) => error,
            DispatchError::CompletionDropped => {
                anyhow!("decoration main-thread action was dropped before completion").into()
            }
        }
    })
}

#[cfg(target_os = "macos")]
fn platform() -> Platform {
    Platform::Macos
}

#[cfg(target_os = "windows")]
fn platform() -> Platform {
    Platform::Windows
}

#[cfg(target_os = "linux")]
fn platform() -> Platform {
    Platform::Linux
}

#[cfg(not(target_os = "linux"))]
fn frontend_options<R: Runtime>(window: &WebviewWindow<R>) -> Result<FrontendOptions, Error> {
    let mut options = FrontendOptions::default();
    if window.is_minimizable()? {
        options.controls.push("minimize");
    }
    if window.is_maximizable()? && window.is_resizable()? {
        options.controls.push("maximize");
    }
    if window.is_closable()? {
        options.controls.push("close");
    }
    Ok(options)
}

#[cfg(target_os = "linux")]
fn frontend_options<R: Runtime>(window: &WebviewWindow<R>) -> Result<FrontendOptions, Error> {
    linux::frontend_options(window)
}

fn evaluate_preparation<R: Runtime>(
    window: &WebviewWindow<R>,
    target: FrontendTarget,
) -> Result<(), Error> {
    let script = frontend::prepare_script(target, platform(), &frontend_options(window)?)?;
    window.eval(&script)
}

#[cfg(any(target_os = "linux", test))]
fn run_linux_restoration(
    visible: bool,
    hide: impl FnOnce() -> Result<(), Error>,
    restore: impl FnOnce() -> Result<(), Error>,
    show: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    if !visible {
        return restore();
    }

    hide()?;
    let restore_result = restore();
    let show_result = show();
    match (restore_result, show_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(restore), Ok(())) => Err(restore),
        (Ok(()), Err(show)) => Err(show),
        (Err(restore), Err(show)) => Err(anyhow!(
            "restoring Linux native decorations failed: {restore}; showing the window again also failed: {show}"
        )
        .into()),
    }
}

fn restore_native_decorations<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), Error> {
    #[cfg(target_os = "windows")]
    {
        let uninstall = snap::uninstall(window);
        let restore = window.set_decorations(true);
        return match (uninstall, restore) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(uninstall), Ok(())) => Err(uninstall),
            (Ok(()), Err(restore)) => Err(restore),
            (Err(uninstall), Err(restore)) => Err(anyhow!(
                "snap overlay removal failed: {uninstall}; restoring native decorations also failed: {restore}"
            )
            .into()),
        };
    }
    #[cfg(target_os = "macos")]
    {
        let deactivate = traffic::deactivate(window);
        let restore = window.set_decorations(true);
        return match (deactivate, restore) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(deactivate), Ok(())) => Err(deactivate),
            (Ok(()), Err(restore)) => Err(restore),
            (Err(deactivate), Err(restore)) => Err(anyhow!(
                "traffic-light deactivation failed: {deactivate}; restoring native decorations also failed: {restore}"
            )
            .into()),
        };
    }
    #[cfg(target_os = "linux")]
    {
        let visible = window.is_visible()?;
        return run_linux_restoration(
            visible,
            || window.hide(),
            || window.set_decorations(true),
            || window.show(),
        );
    }
    #[allow(unreachable_code)]
    Ok(())
}

fn cancel_activation<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), Error> {
    restore_native_decorations(window)?;
    let state = decoration_state(window)?;
    if let Some(target) = state.cancel_current(window.label()) {
        let script = frontend::cancel_script(target)?;
        window.eval(&script)?;
    }
    Ok(())
}

fn apply_native_decorations<R: Runtime>(
    window: &WebviewWindow<R>,
    _target: FrontendTarget,
) -> Result<(), Error> {
    #[cfg(target_os = "macos")]
    {
        window.set_decorations(true)?;
        return traffic::activate(window, _target);
    }
    #[cfg(target_os = "windows")]
    {
        let options = frontend_options(window)?;
        if options.controls.contains(&"maximize") {
            let right_index = u32::from(options.controls.contains(&"close"));
            snap::install(window, _target, 32, 58, right_index)?;
        }
        return window.set_decorations(false);
    }
    #[cfg(target_os = "linux")]
    return window.set_decorations(false);
    #[allow(unreachable_code)]
    Ok(())
}

fn begin_activation<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), Error> {
    let state = decoration_state(window)?;
    let target = match state
        .begin_activation(window.label())
        .map_err(|error| Error::from(anyhow!(error)))?
    {
        ActivationDecision::AlreadyActive(_) => return Ok(()),
        ActivationDecision::Reserved(target) => target,
    };
    if let Err(error) = restore_native_decorations(window) {
        state.fail_preparation(window.label(), target);
        return Err(error);
    }
    if let Err(error) = evaluate_preparation(window, target) {
        state.fail_preparation(window.label(), target);
        return Err(error);
    }
    Ok(())
}

fn handle_page_load<R: Runtime>(
    window: &WebviewWindow<R>,
    event: tauri::webview::PageLoadEvent,
) -> Result<(), Error> {
    let state = decoration_state(window)?;
    match event {
        tauri::webview::PageLoadEvent::Started => {
            let Some(target) = state.invalidate_document(window.label()) else {
                return Ok(());
            };
            if let Err(error) = restore_native_decorations(window) {
                state.fail_preparation(window.label(), target);
                return Err(error);
            }
        }
        tauri::webview::PageLoadEvent::Finished => {
            let Some(target) = state.prepare_document(window.label()) else {
                return Ok(());
            };
            if let Err(error) = evaluate_preparation(window, target) {
                state.fail_preparation(window.label(), target);
                let _ = restore_native_decorations(window);
                return Err(error);
            }
        }
    }
    Ok(())
}

fn parse_frontend_token(value: &str) -> Result<u64, String> {
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("decoration acknowledgement contains a non-canonical token".to_owned());
    }
    value
        .parse()
        .map_err(|_| "decoration acknowledgement token is out of range".to_owned())
}

fn run_native_activation<Apply, Rollback>(
    state: &DecorationState,
    label: &str,
    target: FrontendTarget,
    apply: Apply,
    rollback: Rollback,
) -> Result<(), Error>
where
    Apply: FnOnce() -> Result<(), Error>,
    Rollback: FnOnce() -> Result<(), Error>,
{
    match state.begin_native_apply(label, target) {
        ReadinessDecision::AlreadyActive => return Ok(()),
        ReadinessDecision::AlreadyApplying => {
            return Err(anyhow!("decoration native activation is already in progress").into());
        }
        ReadinessDecision::Stale => {
            return Err(anyhow!("stale decoration readiness acknowledgement").into());
        }
        ReadinessDecision::Apply => {}
    }

    if let Err(apply_error) = apply() {
        let rollback_error = rollback().err();
        state.fail_native_apply(label, target);
        return match rollback_error {
            None => Err(apply_error),
            Some(rollback_error) => Err(anyhow!(
                "native decoration activation failed: {apply_error}; rollback failed: {rollback_error}"
            )
            .into()),
        };
    }

    if state.commit_native_apply(label, target) {
        return Ok(());
    }

    let rollback_error = rollback().err();
    state.fail_native_apply(label, target);
    match rollback_error {
        None => Err(anyhow!("decoration activation was superseded before commit").into()),
        Some(error) => {
            Err(anyhow!("decoration activation was superseded and rollback failed: {error}").into())
        }
    }
}

fn acknowledge_on_main<R: Runtime>(
    window: &WebviewWindow<R>,
    target: FrontendTarget,
    ok: bool,
) -> Result<(), Error> {
    let state = decoration_state(window)?;
    if !ok {
        return state
            .fail_preparation(window.label(), target)
            .then_some(())
            .ok_or_else(|| anyhow!("stale decoration failure acknowledgement").into());
    }

    run_native_activation(
        &state,
        window.label(),
        target,
        || apply_native_decorations(window, target),
        || restore_native_decorations(window),
    )
}

fn primary_webview_labels_match(webview_label: &str, native_window_label: &str) -> bool {
    webview_label == native_window_label
}

fn require_primary_webview<R: Runtime>(webview: &WebviewWindow<R>) -> Result<(), String> {
    let native_window = webview.as_ref().window();
    if primary_webview_labels_match(webview.label(), native_window.label()) {
        Ok(())
    } else {
        Err("decoration commands are limited to the native window's primary webview".to_owned())
    }
}

#[tauri::command(rename_all = "camelCase")]
fn frontend_ack<R: Runtime>(
    webview: WebviewWindow<R>,
    _state: tauri::State<'_, DecorationState>,
    window_generation: String,
    document_token: String,
    ok: bool,
) -> Result<(), String> {
    require_primary_webview(&webview)?;
    let window_generation = parse_frontend_token(&window_generation)?;
    let document_token = parse_frontend_token(&document_token)?;
    let target = FrontendTarget::from_values(window_generation, document_token)
        .ok_or_else(|| "decoration acknowledgement tokens must be nonzero".to_owned())?;
    let window = webview.clone();
    dispatch_webview(&webview, move || acknowledge_on_main(&window, target, ok))
        .map_err(|error| error.to_string())
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("decoration")
        .register_uri_scheme_protocol(protocol::SCHEME, |_context, request| {
            protocol::handle(request)
        })
        .invoke_handler(tauri::generate_handler![frontend_ack])
        .setup(|app, _api| {
            if app.manage(DecorationState::new(std::thread::current().id())) {
                Ok(())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "tauri-plugin-decoration state was already initialized",
                )
                .into())
            }
        })
        .on_page_load(|webview, payload| {
            let Some(window) = webview.app_handle().get_webview_window(webview.label()) else {
                return;
            };
            let reconcile = window.clone();
            let event = payload.event();
            if let Err(error) =
                dispatch_webview(&window, move || handle_page_load(&reconcile, event))
            {
                eprintln!(
                    "decoration: failed to reconcile page for window {:?}: {error}",
                    window.label()
                );
            }
        })
        .on_event(|app, event| {
            let RunEvent::WindowEvent { label, event, .. } = event else {
                return;
            };
            #[cfg(target_os = "windows")]
            if matches!(event, WindowEvent::Resized(_)) {
                if let Some(window) = app.get_webview_window(label) {
                    if let Err(error) = snap::refresh_fullscreen(&window) {
                        eprintln!(
                            "decoration: failed to synchronize Windows fullscreen state for window {label:?}: {error}"
                        );
                    }
                }
            }
            if matches!(event, WindowEvent::Destroyed)
                && app.get_webview_window(label).is_none()
            {
                if let Some(state) = app.try_state::<DecorationState>() {
                    if let Some(generation) = state.begin_destroy_current(label) {
                        let finish_app = app.clone();
                        let finish_label = label.clone();
                        if let Err(error) = app.run_on_main_thread(move || {
                            if let Some(state) = finish_app.try_state::<DecorationState>() {
                                state.finish_destroy(&finish_label, generation);
                            }
                        }) {
                            eprintln!(
                                "decoration: failed to schedule terminal cleanup for window {label:?}: {error}"
                            );
                        }
                    }
                }
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_frontend_token, primary_webview_labels_match, run_linux_restoration,
        run_native_activation, ActivationDecision, DecorationState,
    };
    use anyhow::anyhow;
    use std::{
        cell::RefCell,
        sync::atomic::{AtomicUsize, Ordering},
        thread,
    };

    fn reserve(state: &DecorationState) -> super::FrontendTarget {
        match state.begin_activation("main").unwrap() {
            ActivationDecision::Reserved(target) => target,
            other => panic!("expected reservation, got {other:?}"),
        }
    }

    #[test]
    fn frontend_tokens_are_canonical_nonzero_u64_values() {
        assert_eq!(parse_frontend_token("1"), Ok(1));
        assert_eq!(parse_frontend_token(&u64::MAX.to_string()), Ok(u64::MAX));
        for invalid in ["", "0", "01", "+1", "1.0", "18446744073709551616"] {
            assert!(parse_frontend_token(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn only_the_native_windows_primary_webview_can_drive_its_decoration() {
        assert!(primary_webview_labels_match("main", "main"));
        assert!(!primary_webview_labels_match("sidebar", "main"));
        assert!(!primary_webview_labels_match("main", "other-window"));
    }

    #[test]
    fn visible_linux_restoration_unmaps_before_restoring_and_remaps_afterward() {
        let operations = RefCell::new(Vec::new());
        run_linux_restoration(
            true,
            || {
                operations.borrow_mut().push("hide");
                Ok(())
            },
            || {
                operations.borrow_mut().push("restore");
                Ok(())
            },
            || {
                operations.borrow_mut().push("show");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*operations.borrow(), ["hide", "restore", "show"]);
    }

    #[test]
    fn failed_visible_linux_restoration_still_attempts_to_remap() {
        let shown = AtomicUsize::new(0);
        let error = run_linux_restoration(
            true,
            || Ok(()),
            || Err(anyhow!("restore failed").into()),
            || {
                shown.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("restore failed"));
        assert_eq!(shown.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn native_apply_error_rolls_back_and_leaves_a_retryable_state() {
        let state = DecorationState::new(thread::current().id());
        let target = reserve(&state);
        let rollbacks = AtomicUsize::new(0);

        let error = run_native_activation(
            &state,
            "main",
            target,
            || Err(anyhow!("apply failed").into()),
            || {
                rollbacks.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("apply failed"));
        assert_eq!(rollbacks.load(Ordering::SeqCst), 1);
        let retry = reserve(&state);
        assert_eq!(retry.window, target.window);
        assert_ne!(retry.document, target.document);
    }

    #[test]
    fn superseded_native_apply_rolls_back_instead_of_committing_old_state() {
        let state = DecorationState::new(thread::current().id());
        let target = reserve(&state);
        let rollbacks = AtomicUsize::new(0);

        let error = run_native_activation(
            &state,
            "main",
            target,
            || {
                assert!(state.begin_destroy("main", target.window));
                Ok(())
            },
            || {
                rollbacks.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("superseded"));
        assert_eq!(rollbacks.load(Ordering::SeqCst), 1);
    }
}
