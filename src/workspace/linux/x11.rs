use std::{
    ffi::CStr,
    os::raw::c_int,
    ptr,
    sync::{Mutex, MutexGuard},
};

use x11_dl::{xlib, xrandr};

use super::{exclusive_edge, positive_extent};
use crate::{
    error::AppError,
    workspace::model::{MonitorId, MonitorInfo},
};

/// libX11 and libXrandr corrupt the heap when several threads talk to them at the same time, so
/// every interaction is serialized. Enumeration runs at startup and on explicit refreshes only,
/// so a process wide lock costs nothing that matters.
static X11_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    X11_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Enumerates the monitors reported by the X server.
pub(super) fn enumerate_monitors() -> Result<Vec<MonitorInfo>, AppError> {
    randr_monitors()?.into_iter().map(monitor_info).collect()
}

/// Returns the connector name of the primary output, if the X server reports one.
///
/// Under a Wayland session the X server is `XWayland` and mirrors the compositor layout,
/// including the primary output, under the same connector names. The call is best effort: a
/// session without an X server simply leaves the primary output undecided.
pub(super) fn primary_output_name() -> Option<String> {
    // XWayland always listens on a local display. A remote one - a stale `ssh -X` forwarding
    // target - would block `XOpenDisplay` on a connect timeout for a value the compositor
    // layout cannot depend on anyway.
    if !is_local_display() {
        return None;
    }

    randr_monitors()
        .ok()?
        .into_iter()
        .find(|monitor| monitor.primary)
        .and_then(|monitor| monitor.name)
        .filter(|name| !name.is_empty())
}

fn is_local_display() -> bool {
    std::env::var_os("DISPLAY")
        .as_deref()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with(':') || value.starts_with("unix:"))
}

struct X11Monitor {
    name: Option<String>,
    primary: bool,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn monitor_info(monitor: X11Monitor) -> Result<MonitorInfo, AppError> {
    let pixel_width = positive_extent(monitor.width, "width")?;
    let pixel_height = positive_extent(monitor.height, "height")?;

    // RandR 1.5 always names its monitors; the position is only a last resort because it does
    // not identify the same monitor across hot-plug events.
    let id = monitor
        .name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("monitor-{}-{}", monitor.x, monitor.y));
    let device_name = id.clone();

    Ok(MonitorInfo {
        id: MonitorId::new(id),
        device_name,
        is_primary: monitor.primary,
        pixel_width,
        pixel_height,
        virtual_left: monitor.x,
        virtual_top: monitor.y,
        virtual_right: exclusive_edge(monitor.x, pixel_width)?,
        virtual_bottom: exclusive_edge(monitor.y, pixel_height)?,
    })
}

/// Reads the RandR 1.5 monitor list of the X server named by `DISPLAY`.
fn randr_monitors() -> Result<Vec<X11Monitor>, AppError> {
    let _x11_guard = lock();
    let xlib = xlib::Xlib::open()
        .map_err(|error| AppError::Workspace(format!("failed to load libX11: {error}")))?;
    let xrandr = xrandr::Xrandr::open()
        .map_err(|error| AppError::Workspace(format!("failed to load libXrandr: {error}")))?;

    // SAFETY: `XOpenDisplay` accepts a null name to use `DISPLAY`; the connection is closed
    // exactly once on every path below.
    let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
    if display.is_null() {
        return Err(AppError::Workspace(
            "XOpenDisplay failed; no X server is reachable".to_string(),
        ));
    }

    let root = unsafe { (xlib.XDefaultRootWindow)(display) };
    let mut monitor_count: c_int = 0;
    // SAFETY: `XRRGetMonitors` returns an array of `monitor_count` initialized entries or a
    // null pointer; the array is released with `XRRFreeMonitors` once copied out.
    let raw_monitors = unsafe { (xrandr.XRRGetMonitors)(display, root, 1, &mut monitor_count) };
    if raw_monitors.is_null() || monitor_count <= 0 {
        // SAFETY: the connection was opened above and is no longer used after this call.
        unsafe { (xlib.XCloseDisplay)(display) };
        return Err(AppError::Workspace(
            "RandR reported no monitors".to_string(),
        ));
    }

    let mut monitors = Vec::with_capacity(monitor_count as usize);
    for index in 0..monitor_count as usize {
        // SAFETY: `index` is below the count returned by `XRRGetMonitors`, so the entry lies
        // inside the array it returned.
        let entry = unsafe { &*raw_monitors.add(index) };
        monitors.push(X11Monitor {
            name: atom_name(&xlib, display, entry.name),
            primary: entry.primary != 0,
            x: entry.x,
            y: entry.y,
            width: entry.width,
            height: entry.height,
        });
    }

    // SAFETY: the array came from `XRRGetMonitors` and has not been released yet.
    unsafe { (xrandr.XRRFreeMonitors)(raw_monitors) };
    // SAFETY: the connection was opened above and is no longer used after this call.
    unsafe { (xlib.XCloseDisplay)(display) };

    Ok(monitors)
}

fn atom_name(xlib: &xlib::Xlib, display: *mut xlib::Display, atom: xlib::Atom) -> Option<String> {
    // SAFETY: `XGetAtomName` returns a NUL-terminated string owned by Xlib; it is copied before
    // being released with `XFree`.
    let raw = unsafe { (xlib.XGetAtomName)(display, atom) };
    if raw.is_null() {
        return None;
    }

    // SAFETY: `raw` is the NUL-terminated string returned by `XGetAtomName`.
    let name = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: `raw` still points at the string returned by `XGetAtomName`.
    unsafe { (xlib.XFree)(raw.cast()) };

    Some(name)
}
