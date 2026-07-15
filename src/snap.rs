use crate::{
    frontend,
    lifecycle::FrontendTarget,
    snap_state::{Callbacks, Geometry, Registry, SnapEvent},
};
use anyhow::anyhow;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};
use tauri::{Runtime, WebviewWindow};
use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, RegisterClassExW,
            SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, HTMAXBUTTON, HWND_TOP,
            SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_DPICHANGED, WM_NCDESTROY, WM_NCHITTEST,
            WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_SIZE,
            WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
        },
    },
};

const SNAP_CLASS: &[u16] = &[
    b'T' as u16,
    b'a' as u16,
    b'u' as u16,
    b'r' as u16,
    b'i' as u16,
    b'P' as u16,
    b'l' as u16,
    b'u' as u16,
    b'g' as u16,
    b'i' as u16,
    b'n' as u16,
    b'D' as u16,
    b'e' as u16,
    b'c' as u16,
    b'o' as u16,
    b'r' as u16,
    b'a' as u16,
    b't' as u16,
    b'i' as u16,
    b'o' as u16,
    b'n' as u16,
    b'S' as u16,
    b'n' as u16,
    b'a' as u16,
    b'p' as u16,
    b'V' as u16,
    b'2' as u16,
    0,
];

const SUBCLASS_ID: usize = 1;
const MAX_TITLEBAR_HEIGHT: u32 = 1024;
const MAX_BUTTON_WIDTH: u32 = 1024;
const MAX_RIGHT_INDEX: u32 = 16;

static SNAP_WINDOWS: OnceLock<Mutex<Registry>> = OnceLock::new();
static SNAP_CLASS_REGISTRATION: OnceLock<Result<isize, String>> = OnceLock::new();

fn snap_windows() -> &'static Mutex<Registry> {
    SNAP_WINDOWS.get_or_init(|| Mutex::new(Registry::default()))
}

fn lock_snap_windows() -> MutexGuard<'static, Registry> {
    snap_windows().lock().unwrap()
}

fn with_registry<T>(action: impl FnOnce(&mut Registry) -> T) -> T {
    let mut registry = lock_snap_windows();
    action(&mut registry)
}

pub(crate) fn install<R: Runtime>(
    window: &WebviewWindow<R>,
    target: FrontendTarget,
    titlebar_height: u32,
    button_width: u32,
    right_index: u32,
) -> Result<(), tauri::Error> {
    validate_geometry(titlebar_height, button_width, right_index)?;
    let hwnd = window_hwnd(window)?;

    let event_window = window.clone();
    let event_callback = Arc::new(move |event: SnapEvent| {
        if let Ok(script) = frontend::dispatch_script(target, event.as_str(), &()) {
            let _ = event_window.eval(&script);
        }
    });
    let move_window = window.clone();
    let move_callback = Arc::new(move |x: i32, y: i32| {
        if let Ok(script) = frontend::dispatch_script(target, "snap-mousemove", &(x, y)) {
            let _ = move_window.eval(&script);
        }
    });

    unsafe {
        install_hwnd(
            hwnd,
            Geometry::new(titlebar_height, button_width, right_index),
            Callbacks::new(event_callback, move_callback),
        )?;
    }
    if let Err(error) = refresh_fullscreen(window) {
        let rollback = unsafe { uninstall_hwnd(hwnd) }.err();
        return match rollback {
            Some(rollback) => Err(anyhow!(
                "initial snap fullscreen synchronization failed: {error}; rollback failed: {rollback}"
            )
            .into()),
            None => Err(error),
        };
    }
    Ok(())
}

pub(crate) fn uninstall<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), tauri::Error> {
    let hwnd = window_hwnd(window)?;
    unsafe { uninstall_hwnd(hwnd) }
}

fn validate_geometry(
    titlebar_height: u32,
    button_width: u32,
    right_index: u32,
) -> Result<(), tauri::Error> {
    if titlebar_height == 0 || titlebar_height > MAX_TITLEBAR_HEIGHT {
        return Err(
            anyhow!("snap titlebar height must be between 1 and {MAX_TITLEBAR_HEIGHT}").into(),
        );
    }
    if button_width == 0 || button_width > MAX_BUTTON_WIDTH {
        return Err(anyhow!("snap button width must be between 1 and {MAX_BUTTON_WIDTH}").into());
    }
    if right_index > MAX_RIGHT_INDEX {
        return Err(
            anyhow!("snap right-side button index must not exceed {MAX_RIGHT_INDEX}").into(),
        );
    }
    Ok(())
}

unsafe fn install_hwnd(
    hwnd: isize,
    geometry: Geometry,
    callbacks: Callbacks,
) -> Result<(), tauri::Error> {
    if with_registry(|registry| registry.position(hwnd)).is_some() {
        return Err(anyhow!("a snap overlay is already installed for this window").into());
    }
    let instance = register_class()?;
    let parent = hwnd as HWND;
    let overlay = CreateWindowExW(
        0,
        SNAP_CLASS.as_ptr(),
        SNAP_CLASS.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
        0,
        0,
        0,
        0,
        parent,
        std::ptr::null_mut(),
        instance,
        std::ptr::null_mut(),
    );
    if overlay.is_null() {
        return Err(last_os_error("CreateWindowExW"));
    }

    if SetWindowSubclass(parent, Some(parent_subclass_proc), SUBCLASS_ID, 0) == 0 {
        let subclass_error = anyhow!("SetWindowSubclass failed for the decoration snap overlay");
        let destroy_result = DestroyWindow(overlay);
        return if destroy_result == 0 {
            Err(anyhow!("{subclass_error}; cleanup DestroyWindow failed").into())
        } else {
            Err(subclass_error.into())
        };
    }

    if let Err(error) =
        with_registry(|registry| registry.insert(hwnd, overlay as isize, geometry, callbacks))
    {
        RemoveWindowSubclass(parent, Some(parent_subclass_proc), SUBCLASS_ID);
        let destroy_failed = DestroyWindow(overlay) == 0;
        return if destroy_failed {
            Err(anyhow!("{error}; cleanup DestroyWindow failed").into())
        } else {
            Err(anyhow!(error).into())
        };
    }

    if let Err(error) = update_overlay_position(parent) {
        let rollback = uninstall_hwnd(hwnd).err();
        return match rollback {
            Some(rollback) => Err(anyhow!(
                "snap overlay positioning failed: {error}; rollback failed: {rollback}"
            )
            .into()),
            None => Err(error),
        };
    }

    Ok(())
}

unsafe fn uninstall_hwnd(hwnd: isize) -> Result<(), tauri::Error> {
    let parent = hwnd as HWND;
    if with_registry(|registry| registry.position(hwnd)).is_none() {
        return Ok(());
    }

    let subclass_removed =
        RemoveWindowSubclass(parent, Some(parent_subclass_proc), SUBCLASS_ID) != 0;
    let removed = with_registry(|registry| registry.remove_parent(hwnd));
    let destroy_error = removed.and_then(|removed| {
        (DestroyWindow(removed.overlay() as HWND) == 0)
            .then(|| std::io::Error::last_os_error().to_string())
    });

    match (subclass_removed, destroy_error) {
        (true, None) => Ok(()),
        (false, None) => Err(anyhow!("RemoveWindowSubclass failed for the snap overlay").into()),
        (true, Some(destroy_error)) => {
            Err(anyhow!("DestroyWindow failed for the snap overlay: {destroy_error}").into())
        }
        (false, Some(destroy_error)) => Err(anyhow!(
            "RemoveWindowSubclass failed; DestroyWindow also failed: {destroy_error}"
        )
        .into()),
    }
}

fn register_class() -> Result<HINSTANCE, tauri::Error> {
    let registration = SNAP_CLASS_REGISTRATION.get_or_init(|| unsafe {
        let instance = GetModuleHandleW(std::ptr::null()) as HINSTANCE;
        if instance.is_null() {
            return Err(format!(
                "GetModuleHandleW failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: SNAP_CLASS.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        if RegisterClassExW(&class) == 0 {
            return Err(format!(
                "RegisterClassExW failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(instance as isize)
    });

    registration
        .as_ref()
        .copied()
        .map(|instance| instance as HINSTANCE)
        .map_err(|error| anyhow!(error.clone()).into())
}

unsafe fn update_overlay_position(hwnd: HWND) -> Result<(), tauri::Error> {
    let Some(position) = with_registry(|registry| registry.position(hwnd as isize)) else {
        return Ok(());
    };
    if position.fullscreen() {
        ShowWindow(position.overlay() as HWND, SW_HIDE);
        return Ok(());
    }

    let mut rect = std::mem::zeroed();
    if GetClientRect(hwnd, &mut rect) == 0 {
        return Err(last_os_error("GetClientRect"));
    }
    let dpi = GetDpiForWindow(hwnd) as u64;
    if dpi == 0 {
        return Err(last_os_error("GetDpiForWindow"));
    }

    let button_width = scaled(position.button_width(), dpi);
    let titlebar_height = scaled(position.titlebar_height(), dpi);
    let occupied_width = button_width.saturating_mul(
        i32::try_from(position.right_index().saturating_add(1)).unwrap_or(i32::MAX),
    );
    let x = rect.right.saturating_sub(occupied_width);
    if SetWindowPos(
        position.overlay() as HWND,
        HWND_TOP,
        x,
        0,
        button_width,
        titlebar_height,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    ) == 0
    {
        return Err(last_os_error("SetWindowPos"));
    }
    Ok(())
}

pub(crate) fn refresh_fullscreen<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<(), tauri::Error> {
    let fullscreen = window.is_fullscreen()?;
    let hwnd = window_hwnd(window)?;
    let Some((position, effects)) =
        with_registry(|registry| registry.set_fullscreen(hwnd, fullscreen))
    else {
        return Ok(());
    };

    let native_update = unsafe {
        if position.fullscreen() {
            ShowWindow(position.overlay() as HWND, SW_HIDE);
            Ok(())
        } else {
            update_overlay_position(hwnd as HWND)
        }
    };
    if let Err(error) = native_update {
        with_registry(|registry| registry.rollback_fullscreen(hwnd, fullscreen));
        return Err(error);
    }
    effects.dispatch();
    Ok(())
}

fn registered_overlay(hwnd: HWND) -> bool {
    with_registry(|registry| registry.parent_for_overlay(hwnd as isize).is_some())
}

fn scaled(value: u32, dpi: u64) -> i32 {
    let scaled = (u64::from(value).saturating_mul(dpi).saturating_add(48)) / 96;
    i32::try_from(scaled).unwrap_or(i32::MAX).max(1)
}

fn last_os_error(operation: &str) -> tauri::Error {
    anyhow!("{operation} failed: {}", std::io::Error::last_os_error()).into()
}

unsafe fn handle_parent_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE | WM_DPICHANGED => {
            let _ = update_overlay_position(hwnd);
        }
        WM_NCDESTROY => {
            // Parent destruction already destroyed the child overlay. Remove
            // only Rust bookkeeping and this callback; never call DestroyWindow
            // from the parent's terminal destruction message.
            with_registry(|registry| registry.remove_parent(hwnd as isize));
            RemoveWindowSubclass(hwnd, Some(parent_subclass_proc), SUBCLASS_ID);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn parent_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> LRESULT {
    catch_unwind(AssertUnwindSafe(|| {
        handle_parent_message(hwnd, msg, wparam, lparam)
    }))
    .unwrap_or_else(|_| DefSubclassProc(hwnd, msg, wparam, lparam))
}

unsafe fn handle_overlay_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST if registered_overlay(hwnd) => return HTMAXBUTTON as LRESULT,
        WM_NCMOUSEMOVE => {
            let Some(parent) = with_registry(|registry| registry.parent_for_overlay(hwnd as isize))
            else {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            };
            let mut point = windows_sys::Win32::Foundation::POINT {
                x: (lparam as i16) as i32,
                y: ((lparam >> 16) as i16) as i32,
            };
            if ScreenToClient(parent as HWND, &mut point) == 0 {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }

            let effects =
                with_registry(|registry| registry.mouse_move(hwnd as isize, point.x, point.y));
            let entered = effects.entered();
            effects.dispatch();
            if entered {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                if TrackMouseEvent(&mut track) == 0 {
                    let effects = with_registry(|registry| registry.mouse_leave(hwnd as isize));
                    effects.dispatch();
                }
            }
            return 0;
        }
        WM_NCMOUSELEAVE => {
            let effects = with_registry(|registry| registry.mouse_leave(hwnd as isize));
            effects.dispatch();
            return 0;
        }
        WM_NCLBUTTONDOWN => {
            let effects = with_registry(|registry| registry.mouse_down(hwnd as isize));
            effects.dispatch();
            return 0;
        }
        WM_NCLBUTTONUP => {
            let effects = with_registry(|registry| registry.mouse_up(hwnd as isize));
            effects.dispatch();
            return 0;
        }
        WM_NCDESTROY => {
            let removed = with_registry(|registry| registry.remove_overlay(hwnd as isize));
            if let Some(removed) = removed {
                RemoveWindowSubclass(
                    removed.parent() as HWND,
                    Some(parent_subclass_proc),
                    SUBCLASS_ID,
                );
            }
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    catch_unwind(AssertUnwindSafe(|| {
        handle_overlay_message(hwnd, msg, wparam, lparam)
    }))
    .unwrap_or_else(|_| DefWindowProcW(hwnd, msg, wparam, lparam))
}

fn window_hwnd<R: Runtime>(window: &WebviewWindow<R>) -> Result<isize, tauri::Error> {
    let handle = window.window_handle()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get()),
        _ => Err(anyhow!("native snap overlay requires a Win32 window handle").into()),
    }
}
