use std::sync::Arc;

use crate::{
    config::PointerOutput, error::AppError, input_pipeline::PenInjector,
    shortcut::ShortcutExecutor, workspace::WorkspaceService,
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

pub fn create_pen_injector(
    workspace: &WorkspaceService,
    output: PointerOutput,
) -> Result<Arc<dyn PenInjector>, AppError> {
    platform::create_pen_injector(workspace, output)
}

pub fn create_shortcut_executor(workspace: &WorkspaceService) -> Arc<dyn ShortcutExecutor> {
    platform::create_shortcut_executor(workspace)
}
