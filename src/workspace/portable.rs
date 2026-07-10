use crate::{
    error::AppError,
    workspace::model::{MonitorId, MonitorInfo},
};

pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>, AppError> {
    Ok(vec![MonitorInfo {
        id: MonitorId::new("primary".to_string()),
        device_name: format!("{} primary display", std::env::consts::OS),
        is_primary: true,
        pixel_width: 1920,
        pixel_height: 1080,
        virtual_left: 0,
        virtual_top: 0,
        virtual_right: 1920,
        virtual_bottom: 1080,
    }])
}
