use std::sync::Arc;

use crate::{error::AppError, input_pipeline::PenInjector, shortcut::ShortcutExecutor};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

pub fn create_pen_injector() -> Result<Arc<dyn PenInjector>, AppError> {
    platform::create_pen_injector()
}

pub fn create_shortcut_executor() -> Arc<dyn ShortcutExecutor> {
    platform::create_shortcut_executor()
}
