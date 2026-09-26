use std::sync::Arc;

use input_linux::{EventKind, SynchronizeKind, sys, uinput::UInputHandle};

use crate::{
    error::AppError, input_pipeline::PenInjector, shortcut::ShortcutExecutor,
    workspace::WorkspaceService,
};

mod pen;
mod shortcut;

pub fn create_pen_injector(workspace: &WorkspaceService) -> Result<Arc<dyn PenInjector>, AppError> {
    Ok(Arc::new(pen::LinuxPenInjector::new(workspace.clone())?))
}

pub fn create_shortcut_executor(workspace: &WorkspaceService) -> Arc<dyn ShortcutExecutor> {
    Arc::new(shortcut::LinuxShortcutExecutor::new(workspace.clone()))
}

/// Highest absolute-axis value reported by both virtual devices. libinput and the
/// compositors normalize absolute axes to `[0, 1]` against this range and map the
/// result onto the monitor union, so proportional coordinates track monitor changes.
pub(super) const AXIS_MAX: i32 = 32_767;

/// Wraps an io error with the operation that failed, keeping the original kind.
pub(super) fn io_error(context: &str, error: std::io::Error) -> AppError {
    AppError::Io(std::io::Error::new(
        error.kind(),
        format!("{context}: {error}"),
    ))
}

/// Union of every monitor in the current workspace snapshot, in desktop pixels.
pub(super) struct ScreenGeometry {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

impl ScreenGeometry {
    /// Samples the live workspace so pen and shortcut injection follow monitor changes.
    pub(super) fn from_workspace(workspace: &WorkspaceService) -> Result<Self, AppError> {
        let snapshot = workspace.snapshot()?;
        let monitors = snapshot.monitors.as_slice();
        let first = monitors.first().ok_or_else(|| {
            AppError::Workspace("workspace snapshot contains no monitors".to_string())
        })?;
        let mut left = first.virtual_left;
        let mut top = first.virtual_top;
        let mut right = first.virtual_right;
        let mut bottom = first.virtual_bottom;
        for monitor in &monitors[1..] {
            left = left.min(monitor.virtual_left);
            top = top.min(monitor.virtual_top);
            right = right.max(monitor.virtual_right);
            bottom = bottom.max(monitor.virtual_bottom);
        }
        Ok(Self {
            left,
            top,
            width: (right - left).max(1),
            height: (bottom - top).max(1),
        })
    }

    pub(super) fn axis_x(&self, desktop_x: i32) -> i32 {
        map_axis(desktop_x, self.left, self.width)
    }

    pub(super) fn axis_y(&self, desktop_y: i32) -> i32 {
        map_axis(desktop_y, self.top, self.height)
    }
}

/// Maps a desktop pixel coordinate into the uinput axis range, clamped to it.
fn map_axis(desktop: i32, origin: i32, extent: i32) -> i32 {
    let denominator = i64::from((extent - 1).max(1));
    let scaled = (i64::from(desktop - origin) * i64::from(AXIS_MAX)) / denominator;
    scaled.clamp(0, i64::from(AXIS_MAX)) as i32
}

pub(super) fn event(kind: EventKind, code: u16, value: i32) -> sys::input_event {
    sys::input_event {
        // The kernel ignores the timestamp on uinput writes and stamps events itself.
        time: sys::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        type_: kind.code(),
        code,
        value,
    }
}

pub(super) fn syn_event() -> sys::input_event {
    event(EventKind::Synchronize, SynchronizeKind::Report.code(), 0)
}

/// Writes one frame of events and fails unless the device accepted all of them.
pub(super) fn write_events(
    device: &UInputHandle<std::fs::File>,
    events: &[sys::input_event],
    context: &str,
) -> Result<(), AppError> {
    let written = device
        .write(events)
        .map_err(|error| io_error(context, error))?;
    if written != events.len() {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            format!("{context}: accepted {written} of {} events", events.len()),
        )));
    }
    Ok(())
}
