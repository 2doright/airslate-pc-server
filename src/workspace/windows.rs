use std::mem::size_of;

use windows::{
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW},
        UI::WindowsAndMessaging::MONITORINFOF_PRIMARY,
    },
    core::BOOL,
};

use crate::{
    error::AppError,
    workspace::model::{MonitorId, MonitorInfo},
};

pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>, AppError> {
    let mut monitors = Vec::new();

    // SAFETY: The callback stores results in `monitors`, whose pointer remains valid for the
    // duration of the synchronous EnumDisplayMonitors call. The callback only casts the LPARAM
    // back to the original `Vec<MonitorInfo>` pointer and does not outlive this function.
    let result = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor_proc),
            LPARAM((&mut monitors as *mut Vec<MonitorInfo>) as isize),
        )
    };

    if !result.as_bool() {
        return Err(AppError::Workspace(
            "failed to enumerate Windows monitors".to_string(),
        ));
    }

    Ok(monitors)
}

unsafe extern "system" fn enum_monitor_proc(
    monitor: HMONITOR,
    _hdc: HDC,
    _clip_rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let monitors = data.0 as *mut Vec<MonitorInfo>;
    if monitors.is_null() {
        return BOOL(0);
    }

    match load_monitor_info(monitor) {
        Ok(info) => {
            // SAFETY: `monitors` comes from a valid mutable reference created in
            // `enumerate_monitors` and EnumDisplayMonitors invokes the callback synchronously.
            unsafe {
                (*monitors).push(info);
            }
            BOOL(1)
        }
        Err(_) => BOOL(0),
    }
}

fn load_monitor_info(monitor: HMONITOR) -> Result<MonitorInfo, AppError> {
    let mut monitor_info = MONITORINFOEXW::default();
    monitor_info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;

    // SAFETY: `monitor_info` is properly initialized and lives for the duration of the call.
    let get_info_ok = unsafe { GetMonitorInfoW(monitor, &mut monitor_info as *mut _ as *mut _) };
    if !get_info_ok.as_bool() {
        return Err(AppError::Workspace(
            "GetMonitorInfoW failed while loading monitor metadata".to_string(),
        ));
    }

    let device_name = utf16_to_string(&monitor_info.szDevice);
    if device_name.is_empty() {
        return Err(AppError::Workspace(
            "Windows monitor device name was empty".to_string(),
        ));
    }

    let rect = monitor_info.monitorInfo.rcMonitor;
    let (pixel_width, pixel_height) = monitor_extent(rect)?;

    Ok(MonitorInfo {
        id: MonitorId::new(device_name.clone()),
        device_name,
        is_primary: monitor_info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        pixel_width,
        pixel_height,
        virtual_left: rect.left,
        virtual_top: rect.top,
        virtual_right: rect.right,
        virtual_bottom: rect.bottom,
    })
}

fn monitor_extent(rect: RECT) -> Result<(u32, u32), AppError> {
    let width = rect.right.checked_sub(rect.left);
    let height = rect.bottom.checked_sub(rect.top);
    match (
        width.and_then(|value| u32::try_from(value).ok()),
        height.and_then(|value| u32::try_from(value).ok()),
    ) {
        (Some(width), Some(height)) if width > 0 && height > 0 => Ok((width, height)),
        _ => Err(AppError::Workspace(format!(
            "Windows returned invalid monitor bounds ({}, {})-({}, {})",
            rect.left, rect.top, rect.right, rect.bottom
        ))),
    }
}

fn utf16_to_string(value: &[u16]) -> String {
    let end = value.iter().position(|ch| *ch == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_extent_uses_the_selected_monitor_bounds() {
        let rect = RECT {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1440,
        };

        assert_eq!(
            monitor_extent(rect).expect("valid monitor bounds"),
            (2560, 1440)
        );
    }

    #[test]
    fn monitor_extent_supports_monitors_left_of_the_primary_display() {
        let rect = RECT {
            left: -1920,
            top: 120,
            right: 0,
            bottom: 1200,
        };

        assert_eq!(
            monitor_extent(rect).expect("valid monitor bounds"),
            (1920, 1080)
        );
    }

    #[test]
    fn monitor_extent_rejects_empty_or_inverted_bounds() {
        let rect = RECT {
            left: 1920,
            top: 0,
            right: 1920,
            bottom: 1080,
        };

        assert!(monitor_extent(rect).is_err());
    }
}
