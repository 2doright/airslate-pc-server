#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod desktop_bridge;
mod error;
mod handshake;
mod input_pipeline;
mod protocol;
mod radial_overlay;
mod session;
mod shortcut;
mod udp_ingest;
mod usb_accessory;
mod windows_injector;
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
    let context = app::initialize()?;
    app::start_services(&context)?;
    desktop_bridge::shell::run(context)
}

#[cfg(windows)]
fn init_dpi_awareness() -> Result<(), error::AppError> {
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
