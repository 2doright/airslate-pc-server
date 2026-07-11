#![allow(dead_code)]

#[cfg(target_os = "macos")]
mod macos;
mod model;
mod service;
#[cfg(windows)]
mod windows;

#[allow(unused_imports)]
pub use self::{
    model::{ActiveWorkspace, MonitorId, MonitorInfo, WorkspaceSnapshot},
    service::WorkspaceService,
};
