#![allow(dead_code)]

mod model;
#[cfg(not(windows))]
mod portable;
mod service;
#[cfg(windows)]
mod windows;

#[allow(unused_imports)]
pub use self::{
    model::{ActiveWorkspace, MonitorId, MonitorInfo, WorkspaceSnapshot},
    service::WorkspaceService,
};
