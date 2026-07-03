#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorId(String);

impl MonitorId {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub id: MonitorId,
    pub device_name: String,
    pub is_primary: bool,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub virtual_left: i32,
    pub virtual_top: i32,
    pub virtual_right: i32,
    pub virtual_bottom: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveWorkspace {
    pub monitor: MonitorInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub monitors: Vec<MonitorInfo>,
    pub active_monitor_id: Option<MonitorId>,
    pub active_workspace: Option<ActiveWorkspace>,
}
