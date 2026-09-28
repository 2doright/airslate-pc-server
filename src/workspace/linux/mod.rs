//! Linux monitor enumeration backing [`crate::workspace::WorkspaceService`].
//!
//! Wayland sessions enumerate the `wl_output` globals of the running compositor through
//! `wayland-client` plus the `xdg_output` extension; X11 sessions read the RandR 1.5 monitor list
//! through `x11-dl`. Neither path shells out to `xrandr` and neither needs GTK, so enumeration also
//! works before the Tauri application (and therefore `gtk::init()`) has been built, and from any
//! thread.
//!
//! Both paths report the desktop-global coordinate space of the running display server, which is
//! the space pointer and tablet events are expressed in:
//!
//! * Wayland: the compositor's logical layout, i.e. an output's mode size divided by its scale.
//!   Core `wl_output` only reports the mode in device pixels and an integer scale, so the layout
//!   size comes from `xdg_output`, which is the only source that resolves fractional scaling
//!   (1.25, 1.5, ...) correctly. The virtual tablet the uinput pen injector creates is mapped by
//!   the compositor onto exactly this space, so the union of
//!   `virtual_left`/`virtual_top`/`virtual_right`/`virtual_bottom` is the coordinate space the
//!   injector has to fill.
//! * X11: the RandR screen space, which is the coordinate space X clients and XInput tablets use.
//!
//! `virtual_right` and `virtual_bottom` are exclusive (`right == left + pixel_width`), matching the
//! Windows `rcMonitor` and macOS `CGDisplayBounds` conventions used by the other platforms.
//!
//! Monitor ids are the connector names reported by the compositor (`HDMI-1`, `eDP-1`, `DP-1`, ...)
//! because they survive reboots and are what `selected_monitor_id` is persisted against. A
//! positional id is used only when the server does not report a name at all, which no longer
//! identifies the same monitor after a hot-plug event.

use crate::{error::AppError, workspace::model::MonitorInfo};

mod wayland;
mod x11;

/// Enumerates the monitors of the current session in desktop-global coordinates.
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>, AppError> {
    let monitors = match Session::detect()? {
        // XWayland mirrors the compositor layout under the same connector names, so it is queried
        // for the primary output only; the geometry itself comes from the compositor.
        Session::Wayland => wayland::enumerate_monitors(x11::primary_output_name().as_deref())?,
        Session::X11 => x11::enumerate_monitors()?,
    };

    if monitors.is_empty() {
        return Err(AppError::Workspace("no monitors detected".to_string()));
    }

    Ok(order_monitors(monitors))
}

/// The display server that owns the current session.
enum Session {
    Wayland,
    X11,
}

impl Session {
    fn detect() -> Result<Self, AppError> {
        Self::from_env(
            std::env::var_os("XDG_SESSION_TYPE").as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
            std::env::var_os("DISPLAY").is_some(),
        )
    }

    fn from_env(
        session_type: Option<&std::ffi::OsStr>,
        has_wayland_display: bool,
        has_x11_display: bool,
    ) -> Result<Self, AppError> {
        match session_type.and_then(|value| value.to_str()) {
            Some("wayland") => return Ok(Self::Wayland),
            Some("x11") => return Ok(Self::X11),
            _ => {}
        }

        if has_wayland_display {
            Ok(Self::Wayland)
        } else if has_x11_display {
            Ok(Self::X11)
        } else {
            Err(AppError::Workspace(
                "no graphical session detected: XDG_SESSION_TYPE, WAYLAND_DISPLAY and DISPLAY are unset"
                    .to_string(),
            ))
        }
    }
}

/// Sorts monitors into a stable desktop order and guarantees a single primary monitor.
///
/// The registry order of `wl_output` globals and of RandR monitors is not meaningful, so the
/// monitors are ordered by their desktop position. [`crate::workspace::WorkspaceService`] falls
/// back to the first monitor when no `selected_monitor_id` is configured, which is then the monitor
/// covering the desktop origin.
fn order_monitors(mut monitors: Vec<MonitorInfo>) -> Vec<MonitorInfo> {
    monitors.sort_by(|left, right| {
        (left.virtual_top, left.virtual_left, left.id.as_str()).cmp(&(
            right.virtual_top,
            right.virtual_left,
            right.id.as_str(),
        ))
    });

    // Not every compositor describes a primary output. When none does, the monitor covering the
    // desktop origin - the first one after sorting - takes the role, which is the layout used in
    // practice and keeps the active monitor deterministic.
    let has_primary = monitors.iter().any(|monitor| monitor.is_primary);
    if !has_primary && let Some(primary) = monitors.first_mut() {
        primary.is_primary = true;
    }

    monitors
}

/// Validates an extent reported by the display server.
fn positive_extent(value: i32, axis: &str) -> Result<u32, AppError> {
    u32::try_from(value)
        .ok()
        .filter(|extent| *extent > 0)
        .ok_or_else(|| {
            AppError::Workspace(format!(
                "monitor {axis} extent {value} is not a positive size"
            ))
        })
}

/// Computes the exclusive right or bottom edge of a monitor.
fn exclusive_edge(origin: i32, extent: u32) -> Result<i32, AppError> {
    origin
        .checked_add(i32::try_from(extent).map_err(|_| {
            AppError::Workspace(format!("monitor extent {extent} exceeds the i32 range"))
        })?)
        .ok_or_else(|| {
            AppError::Workspace(format!(
                "monitor edge at origin {origin} with extent {extent} overflows"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::model::MonitorId;

    fn monitor(id: &str, left: i32, top: i32, width: u32, height: u32) -> MonitorInfo {
        MonitorInfo {
            id: MonitorId::new(id.to_string()),
            device_name: id.to_string(),
            is_primary: false,
            pixel_width: width,
            pixel_height: height,
            virtual_left: left,
            virtual_top: top,
            virtual_right: exclusive_edge(left, width).expect("valid monitor"),
            virtual_bottom: exclusive_edge(top, height).expect("valid monitor"),
        }
    }

    #[test]
    fn session_detection_prefers_the_session_type() {
        assert!(matches!(
            Session::from_env(Some(std::ffi::OsStr::new("wayland")), false, true),
            Ok(Session::Wayland)
        ));
        assert!(matches!(
            Session::from_env(Some(std::ffi::OsStr::new("x11")), true, false),
            Ok(Session::X11)
        ));
    }

    #[test]
    fn session_detection_falls_back_to_the_display_variables() {
        assert!(matches!(
            Session::from_env(None, true, true),
            Ok(Session::Wayland)
        ));
        assert!(matches!(
            Session::from_env(None, false, true),
            Ok(Session::X11)
        ));
    }

    #[test]
    fn session_detection_rejects_a_headless_environment() {
        assert!(Session::from_env(None, false, false).is_err());
        assert!(Session::from_env(Some(std::ffi::OsStr::new("tty")), false, false).is_err());
    }

    #[test]
    fn monitor_edges_are_exclusive_like_windows_and_macos() {
        let edge = monitor("DP-1", -1920, 120, 1920, 1080);

        assert_eq!(edge.virtual_right, 0);
        assert_eq!(edge.virtual_bottom, 1200);
    }

    #[test]
    fn ordering_puts_the_desktop_origin_first() {
        let ordered = order_monitors(vec![
            monitor("HDMI-1", 1920, 0, 1920, 1080),
            monitor("eDP-1", 0, 0, 2560, 1440),
        ]);

        assert_eq!(ordered[0].id.as_str(), "eDP-1");
        assert_eq!(ordered[1].id.as_str(), "HDMI-1");
    }

    #[test]
    fn ordering_keeps_a_reported_primary_monitor() {
        let mut primary = monitor("HDMI-1", 1920, 0, 1920, 1080);
        primary.is_primary = true;

        let ordered = order_monitors(vec![monitor("eDP-1", 0, 0, 2560, 1440), primary]);

        assert_eq!(ordered[0].id.as_str(), "eDP-1");
        assert!(!ordered[0].is_primary);
        assert!(ordered[1].is_primary);
    }

    #[test]
    fn ordering_promotes_the_origin_monitor_when_no_primary_is_reported() {
        let ordered = order_monitors(vec![
            monitor("HDMI-1", 1920, 0, 1920, 1080),
            monitor("eDP-1", 0, 0, 2560, 1440),
        ]);

        assert!(ordered[0].is_primary);
        assert!(!ordered[1].is_primary);
    }

    #[test]
    fn extents_reject_empty_or_inverted_monitors() {
        assert!(positive_extent(0, "width").is_err());
        assert!(positive_extent(-1920, "height").is_err());
        assert_eq!(positive_extent(2560, "width").expect("valid extent"), 2560);
    }
}
