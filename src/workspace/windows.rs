use std::mem::size_of;

use windows::{
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{
            DEVMODEW, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_FLAGS, EnumDisplayMonitors,
            EnumDisplaySettingsExW, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
        },
        UI::WindowsAndMessaging::MONITORINFOF_PRIMARY,
    },
    core::{BOOL, PCWSTR},
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

    let (pixel_width, pixel_height) = load_current_display_mode(&monitor_info.szDevice)?;
    let rect = monitor_info.monitorInfo.rcMonitor;

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

fn load_current_display_mode(device_name: &[u16]) -> Result<(u32, u32), AppError> {
    let mut dev_mode = DEVMODEW::default();
    dev_mode.dmSize = size_of::<DEVMODEW>() as u16;

    // SAFETY: `device_name` points to the null-terminated monitor device name provided by
    // GetMonitorInfoW, and `dev_mode` is initialized with the required structure size.
    let result = unsafe {
        EnumDisplaySettingsExW(
            PCWSTR(device_name.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut dev_mode,
            ENUM_DISPLAY_SETTINGS_FLAGS(0),
        )
    };

    if !result.as_bool() {
        return Err(AppError::Workspace(format!(
            "EnumDisplaySettingsExW failed for monitor {}",
            utf16_to_string(device_name)
        )));
    }

    let pixel_width = dev_mode.dmPelsWidth;
    let pixel_height = dev_mode.dmPelsHeight;
    if pixel_width == 0 || pixel_height == 0 {
        return Err(AppError::Workspace(format!(
            "Windows returned an invalid display mode for monitor {}",
            utf16_to_string(device_name)
        )));
    }

    Ok((pixel_width, pixel_height))
}

fn utf16_to_string(value: &[u16]) -> String {
    let end = value.iter().position(|ch| *ch == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}
