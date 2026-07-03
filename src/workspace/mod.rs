#![allow(dead_code)]

mod model;
mod service;
mod windows;

#[allow(unused_imports)]
pub use self::{
    model::{ActiveWorkspace, MonitorId, MonitorInfo, WorkspaceSnapshot},
    service::WorkspaceService,
};
