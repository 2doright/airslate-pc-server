use std::ffi::c_void;

use crate::error::AppError;

pub(super) type CGEventRef = *mut c_void;
pub(super) type CGEventSourceRef = *mut c_void;
pub(super) type CGKeyCode = u16;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(super) struct CGPoint {
    pub(super) x: f64,
    pub(super) y: f64,
}

pub(super) const K_CG_HID_EVENT_TAP: u32 = 0;
pub(super) const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
pub(super) const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
pub(super) const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
pub(super) const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
pub(super) const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
pub(super) const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
pub(super) const K_CG_EVENT_TABLET_PROXIMITY: u32 = 24;
pub(super) const K_CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;
pub(super) const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
pub(super) const K_CG_MOUSE_BUTTON_RIGHT: u32 = 1;
pub(super) const K_CG_EVENT_SOURCE_STATE_PRIVATE: i32 = -1;
pub(super) const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: i32 = 1;
pub(super) const K_CG_EVENT_FIELD_MOUSE_EVENT_PRESSURE: u32 = 2;
pub(super) const K_CG_EVENT_FIELD_MOUSE_EVENT_BUTTON_NUMBER: u32 = 3;
pub(super) const K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_X: u32 = 4;
pub(super) const K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_Y: u32 = 5;
pub(super) const K_CG_EVENT_FIELD_MOUSE_EVENT_SUBTYPE: u32 = 7;
pub(super) const K_CG_EVENT_FIELD_TABLET_POINT_X: u32 = 15;
pub(super) const K_CG_EVENT_FIELD_TABLET_POINT_Y: u32 = 16;
pub(super) const K_CG_EVENT_FIELD_TABLET_POINT_BUTTONS: u32 = 18;
pub(super) const K_CG_EVENT_FIELD_TABLET_POINT_PRESSURE: u32 = 19;
pub(super) const K_CG_EVENT_FIELD_TABLET_TILT_X: u32 = 20;
pub(super) const K_CG_EVENT_FIELD_TABLET_TILT_Y: u32 = 21;
pub(super) const K_CG_EVENT_FIELD_TABLET_DEVICE_ID: u32 = 24;
pub(super) const K_CG_EVENT_FIELD_TABLET_PROXIMITY_DEVICE_ID: u32 = 31;
pub(super) const K_CG_EVENT_FIELD_TABLET_PROXIMITY_VENDOR_POINTER_TYPE: u32 = 33;
pub(super) const K_CG_EVENT_FIELD_TABLET_PROXIMITY_CAPABILITY_MASK: u32 = 36;
pub(super) const K_CG_EVENT_FIELD_TABLET_PROXIMITY_POINTER_TYPE: u32 = 37;
pub(super) const K_CG_EVENT_FIELD_TABLET_PROXIMITY_ENTER_PROXIMITY: u32 = 38;
pub(super) const K_CG_MOUSE_EVENT_SUBTYPE_TABLET_POINT: i64 = 1;
pub(super) const K_CG_MOUSE_EVENT_SUBTYPE_TABLET_PROXIMITY: i64 = 2;
pub(super) const NS_POINTING_DEVICE_TYPE_PEN: i64 = 1;

// CoreGraphics tablet event fields follow the Wacom-compatible path used by
// OpenTabletDriver's macOS backend: https://github.com/OpenTabletDriver/OpenTabletDriver
pub(super) const WACOM_CAPABILITY_MASK: i64 = 0x001 | 0x002 | 0x004 | 0x040 | 0x080 | 0x100 | 0x400;
pub(super) const WACOM_VENDOR_POINTER_TYPE_GENERAL_STYLUS: i64 = 0x802;
pub(super) const MACOS_TABLET_DEVICE_ID: i64 = 5_303_613_955_435_230_461;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub(super) fn CGPreflightPostEventAccess() -> bool;
    pub(super) fn CGRequestPostEventAccess() -> bool;
    pub(super) fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    pub(super) fn CGEventSourceCreate(state_id: i32) -> CGEventSourceRef;
    pub(super) fn CGEventSourceFlagsState(state_id: i32) -> u64;
    pub(super) fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: CGKeyCode,
        key_down: bool,
    ) -> CGEventRef;
    pub(super) fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> CGEventRef;
    pub(super) fn CGEventCreateScrollWheelEvent(
        source: CGEventSourceRef,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
    ) -> CGEventRef;
    pub(super) fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    pub(super) fn CGEventSetType(event: CGEventRef, event_type: u32);
    pub(super) fn CGEventSetLocation(event: CGEventRef, location: CGPoint);
    pub(super) fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub(super) fn CGEventSetDoubleValueField(event: CGEventRef, field: u32, value: f64);
    pub(super) fn CGEventSetFlags(event: CGEventRef, flags: u64);
    pub(super) fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub(super) fn CFRelease(cf: *const c_void);
}

pub(super) fn ensure_post_event_access() -> Result<(), AppError> {
    // SAFETY: Both CoreGraphics functions take no arguments, retain no Rust-owned data, and are
    // safe to call from the input worker before it creates or posts synthetic events.
    let granted = unsafe {
        CGPreflightPostEventAccess() || CGRequestPostEventAccess() || CGPreflightPostEventAccess()
    };
    if granted {
        Ok(())
    } else {
        Err(AppError::MacosInputPermissionDenied)
    }
}

pub(super) fn current_mouse_location() -> Result<CGPoint, AppError> {
    // SAFETY: Creating an event with a null source is supported by CoreGraphics.
    let event = unsafe { CGEventCreate(std::ptr::null_mut()) };
    if event.is_null() {
        return Err(AppError::DesktopShell(
            "failed to create CoreGraphics event".to_string(),
        ));
    }

    // SAFETY: `event` is a valid CoreGraphics event until it is released below.
    let point = unsafe { CGEventGetLocation(event) };
    // SAFETY: `event` is a retained CoreFoundation object created above and released once.
    unsafe { CFRelease(event) };
    Ok(point)
}

pub(super) fn post_event(event: CGEventRef) -> Result<(), AppError> {
    if event.is_null() {
        return Err(AppError::DesktopShell(
            "failed to create CoreGraphics event".to_string(),
        ));
    }

    // SAFETY: `event` is a valid retained CoreGraphics event. Posting does not consume it, so
    // it is released exactly once afterward.
    unsafe {
        CGEventPost(K_CG_HID_EVENT_TAP, event);
        CFRelease(event);
    }
    Ok(())
}
