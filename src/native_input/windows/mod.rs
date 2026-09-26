use std::sync::Arc;

use crate::{
    error::AppError, input_pipeline::PenInjector, shortcut::ShortcutExecutor,
    workspace::WorkspaceService,
};

mod pen;
mod shortcut;

pub(super) fn create_pen_injector(
    _workspace: &WorkspaceService,
) -> Result<Arc<dyn PenInjector>, AppError> {
    Ok(Arc::new(pen::WindowsPenInjector::new()?))
}

pub(super) fn create_shortcut_executor(_workspace: &WorkspaceService) -> Arc<dyn ShortcutExecutor> {
    Arc::new(shortcut::WindowsShortcutExecutor::new())
}
