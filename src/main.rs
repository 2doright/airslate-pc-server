#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod config;
mod desktop_bridge;
mod error;
mod handshake;
mod input_pipeline;
mod native_input;
mod protocol;
mod radial_overlay;
mod session;
mod shortcut;
mod udp_ingest;
mod usb_accessory;
mod workspace;

use std::process::ExitCode;

use tracing::Level;
#[cfg(windows)]
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

fn main() -> ExitCode {
    if let Some(exit_code) = usb_accessory::run_driver_helper_if_requested() {
        return exit_code;
    }
    init_tracing();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fatal: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), error::AppError> {
    init_dpi_awareness()?;
    #[cfg(target_os = "linux")]
    apply_wayland_webview_workaround();
    let context = app::initialize()?;
    app::start_services(&context)?;
    desktop_bridge::shell::run(context)
}

#[cfg(windows)]
fn init_dpi_awareness() -> Result<(), error::AppError> {
    // SAFETY: This configures process-wide DPI awareness before any application windows are
    // created and uses a documented Windows DPI awareness context.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }?;
    Ok(())
}

#[cfg(not(windows))]
fn init_dpi_awareness() -> Result<(), error::AppError> {
    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();
}

#[cfg(target_os = "linux")]
fn apply_wayland_webview_workaround() {
    // The NVIDIA proprietary driver cannot complete the explicit-sync negotiation
    // webkit's dmabuf renderer performs with the Wayland compositor, so the webview
    // dies with "Error 71 (Protocol error) dispatching to Wayland display" before any
    // window appears. Disabling the dmabuf renderer selects the shared-memory painting
    // path, which works on every driver; the workaround is therefore limited to the
    // affected configuration so X11 sessions and other drivers keep the accelerated
    // path.
    if !(is_wayland_session() && has_nvidia_proprietary_driver()) {
        return;
    }

    // SAFETY: runs before the Tauri application and webkit are built, while the process
    // is still single threaded, so no concurrent reader observes a torn environment.
    unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    tracing::info!("disabled the webkit dmabuf renderer for the NVIDIA Wayland session");
}

#[cfg(target_os = "linux")]
fn is_wayland_session() -> bool {
    is_wayland_environment(
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("XDG_SESSION_TYPE").as_deref(),
    )
}

#[cfg(target_os = "linux")]
fn is_wayland_environment(
    has_wayland_display: bool,
    session_type: Option<&std::ffi::OsStr>,
) -> bool {
    has_wayland_display || session_type == Some(std::ffi::OsStr::new("wayland"))
}

#[cfg(target_os = "linux")]
fn has_nvidia_proprietary_driver() -> bool {
    std::path::Path::new("/proc/driver/nvidia/version").exists()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn wayland_display_alone_marks_a_wayland_session() {
        assert!(is_wayland_environment(true, None));
    }

    #[test]
    fn session_type_alone_marks_a_wayland_session() {
        assert!(is_wayland_environment(
            false,
            Some(std::ffi::OsStr::new("wayland"))
        ));
    }

    #[test]
    fn a_session_without_any_wayland_signal_is_not_wayland() {
        assert!(!is_wayland_environment(
            false,
            Some(std::ffi::OsStr::new("x11"))
        ));
        assert!(!is_wayland_environment(false, None));
    }
}
