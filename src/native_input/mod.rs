use std::sync::Arc;

use crate::{
    error::AppError,
    input_pipeline::PenInjector,
    shortcut::ShortcutExecutor,
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
use macos::{
    MacosPenInjector as PlatformPenInjector,
    MacosShortcutExecutor as PlatformShortcutExecutor,
};
#[cfg(windows)]
use windows::{
    WindowsPenInjector as PlatformPenInjector,
    WindowsShortcutExecutor as PlatformShortcutExecutor,
};

pub fn create_pen_injector() -> Result<Arc<dyn PenInjector>, AppError> {
    Ok(Arc::new(PlatformPenInjector::new()?))
}

pub fn create_shortcut_executor() -> Arc<dyn ShortcutExecutor> {
    Arc::new(PlatformShortcutExecutor::new())
}
