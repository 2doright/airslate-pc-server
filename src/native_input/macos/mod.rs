use std::sync::Arc;

use crate::{
    error::AppError,
    input_pipeline::PenInjector,
    shortcut::ShortcutExecutor,
};

mod core_graphics;
mod pen;
mod shortcut;

pub(super) fn create_pen_injector() -> Result<Arc<dyn PenInjector>, AppError> {
    Ok(Arc::new(pen::MacosPenInjector::new()?))
}

pub(super) fn create_shortcut_executor() -> Arc<dyn ShortcutExecutor> {
    Arc::new(shortcut::MacosShortcutExecutor::new())
}
