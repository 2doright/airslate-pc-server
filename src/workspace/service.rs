use std::sync::{Arc, RwLock};

use crate::{
    config::Config,
    error::AppError,
    workspace::model::{ActiveWorkspace, MonitorInfo, WorkspaceSnapshot},
};

#[cfg(target_os = "macos")]
use crate::workspace::macos::enumerate_monitors;
#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::workspace::portable::enumerate_monitors;
#[cfg(windows)]
use crate::workspace::windows::enumerate_monitors;

#[derive(Debug, Clone)]
pub struct WorkspaceService {
    snapshot: Arc<RwLock<WorkspaceSnapshot>>,
}

impl WorkspaceService {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        let snapshot = Self::build_snapshot(config)?;
        Ok(Self {
            snapshot: Arc::new(RwLock::new(snapshot)),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_snapshot(snapshot: WorkspaceSnapshot) -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(snapshot)),
        }
    }

    pub fn refresh(&self, config: &Config) -> Result<(), AppError> {
        let next_snapshot = Self::build_snapshot(config)?;
        let mut snapshot = self
            .snapshot
            .write()
            .map_err(|_| AppError::StatePoisoned("workspace_snapshot"))?;
        *snapshot = next_snapshot;
        Ok(())
    }

    pub fn snapshot(&self) -> Result<WorkspaceSnapshot, AppError> {
        self.snapshot
            .read()
            .map_err(|_| AppError::StatePoisoned("workspace_snapshot"))
            .map(|snapshot| snapshot.clone())
    }

    pub fn current_workspace(&self) -> Result<ActiveWorkspace, AppError> {
        self.snapshot
            .read()
            .map_err(|_| AppError::StatePoisoned("workspace_snapshot"))?
            .active_workspace
            .clone()
            .ok_or_else(|| AppError::Workspace("no active workspace is available".to_string()))
    }

    fn build_snapshot(config: &Config) -> Result<WorkspaceSnapshot, AppError> {
        let monitors = enumerate_monitors()?;
        let active_monitor =
            resolve_active_monitor(&monitors, config.selected_monitor_id.as_deref());
        let active_monitor_id = active_monitor.map(|monitor| monitor.id.clone());
        let active_workspace = active_monitor
            .cloned()
            .map(|monitor| ActiveWorkspace { monitor });

        Ok(WorkspaceSnapshot {
            monitors,
            active_monitor_id,
            active_workspace,
        })
    }
}

fn resolve_active_monitor<'a>(
    monitors: &'a [MonitorInfo],
    selected_monitor_id: Option<&str>,
) -> Option<&'a MonitorInfo> {
    if let Some(selected_monitor_id) = selected_monitor_id {
        if let Some(monitor) = monitors
            .iter()
            .find(|monitor| monitor.id.as_str() == selected_monitor_id)
        {
            return Some(monitor);
        }
    }

    monitors
        .iter()
        .find(|monitor| monitor.is_primary)
        .or_else(|| monitors.first())
}
