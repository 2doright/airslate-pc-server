use std::sync::Arc;

use crate::{
    config::PointerOutput, error::AppError, input_pipeline::PenInjector,
    shortcut::ShortcutExecutor, workspace::WorkspaceService,
};

mod mouse;
mod pen;
mod shortcut;

pub(super) fn create_pen_injector(
    _workspace: &WorkspaceService,
    output: PointerOutput,
) -> Result<Arc<dyn PenInjector>, AppError> {
    match output {
        PointerOutput::Pen => Ok(Arc::new(pen::WindowsPenInjector::new()?)),
        PointerOutput::Mouse => Ok(Arc::new(mouse::WindowsMouseInjector::new())),
    }
}

pub(super) fn create_shortcut_executor(_workspace: &WorkspaceService) -> Arc<dyn ShortcutExecutor> {
    Arc::new(shortcut::WindowsShortcutExecutor::new())
}
