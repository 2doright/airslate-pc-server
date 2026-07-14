use std::{ffi::c_void, sync::Mutex};

use crate::{
    error::AppError,
    input_pipeline::{PenInjectionCommand, PenInjectionCommandKind, PenInjector},
    shortcut::{KeyCode, MouseButton, ShortcutCommand, ShortcutExecutor},
};

pub struct WindowsPenInjector {
    tablet: Mutex<MacosTabletState>,
}

pub struct WindowsShortcutExecutor;

impl WindowsPenInjector {
    pub fn new() -> Result<Self, AppError> {
        Ok(Self {
            tablet: Mutex::new(MacosTabletState::new()?),
        })
    }
}

impl WindowsShortcutExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl PenInjector for WindowsPenInjector {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
        ensure_post_event_access()?;
        self.tablet
            .lock()
            .map_err(|_| AppError::StatePoisoned("macos_tablet_injector"))?
            .inject(command)
    }
}

impl ShortcutExecutor for WindowsShortcutExecutor {
    fn execute(&self, command: ShortcutCommand) -> Result<(), AppError> {
        ensure_post_event_access()?;
        match command {
            ShortcutCommand::KeyDown(key) => post_key_event(key, false),
            ShortcutCommand::KeyUp(key) => post_key_event(key, true),
            ShortcutCommand::PressChord(keys) => {
                for &key in &keys {
                    post_key_event(key, false)?;
                }
                for &key in keys.iter().rev() {
                    post_key_event(key, true)?;
                }
                Ok(())
            }
            ShortcutCommand::MouseMoveRelative { dx, dy } => post_mouse_move_relative(dx, dy),
            ShortcutCommand::MouseWheel { delta } => post_mouse_wheel(delta),
            ShortcutCommand::MouseButtonDown(button) => post_mouse_button(button, true),
            ShortcutCommand::MouseButtonUp(button) => post_mouse_button(button, false),
            ShortcutCommand::ClickAt { button, x, y } => {
                let point = CGPoint {
                    x: f64::from(x),
                    y: f64::from(y),
                };
                post_mouse_event(K_CG_EVENT_MOUSE_MOVED, K_CG_MOUSE_BUTTON_LEFT, point)?;
                post_mouse_event(mouse_event_type(button, true), mouse_button(button), point)?;
                post_mouse_event(mouse_event_type(button, false), mouse_button(button), point)
            }
        }
    }
}

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;
type CGKeyCode = u16;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

const K_CG_HID_EVENT_TAP: u32 = 0;
const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
const K_CG_EVENT_TABLET_PROXIMITY: u32 = 24;
const K_CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;
const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
const K_CG_MOUSE_BUTTON_RIGHT: u32 = 1;
const K_CG_EVENT_SOURCE_STATE_PRIVATE: i32 = -1;
const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: i32 = 1;
const K_CG_EVENT_FIELD_MOUSE_EVENT_PRESSURE: u32 = 2;
const K_CG_EVENT_FIELD_MOUSE_EVENT_BUTTON_NUMBER: u32 = 3;
const K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_X: u32 = 4;
const K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_Y: u32 = 5;
const K_CG_EVENT_FIELD_MOUSE_EVENT_SUBTYPE: u32 = 7;
const K_CG_EVENT_FIELD_TABLET_POINT_BUTTONS: u32 = 18;
const K_CG_EVENT_FIELD_TABLET_POINT_PRESSURE: u32 = 19;
const K_CG_EVENT_FIELD_TABLET_TILT_X: u32 = 20;
const K_CG_EVENT_FIELD_TABLET_TILT_Y: u32 = 21;
const K_CG_EVENT_FIELD_TABLET_DEVICE_ID: u32 = 24;
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_DEVICE_ID: u32 = 31;
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_VENDOR_POINTER_TYPE: u32 = 33;
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_CAPABILITY_MASK: u32 = 36;
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_POINTER_TYPE: u32 = 37;
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_ENTER_PROXIMITY: u32 = 38;
const K_CG_MOUSE_EVENT_SUBTYPE_TABLET_POINT: i64 = 1;
const K_CG_MOUSE_EVENT_SUBTYPE_TABLET_PROXIMITY: i64 = 2;
const NS_POINTING_DEVICE_TYPE_PEN: i64 = 1;
// CoreGraphics tablet event fields follow the Wacom-compatible path used by
// OpenTabletDriver's macOS backend: https://github.com/OpenTabletDriver/OpenTabletDriver
const WACOM_CAPABILITY_MASK: i64 = 0x001 | 0x002 | 0x004 | 0x040 | 0x080 | 0x100 | 0x400;
const WACOM_VENDOR_POINTER_TYPE_GENERAL_STYLUS: i64 = 0x802;
const MACOS_TABLET_DEVICE_ID: i64 = 5_303_613_955_435_230_461;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightPostEventAccess() -> bool;
    fn CGRequestPostEventAccess() -> bool;
    fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    fn CGEventSourceCreate(state_id: i32) -> CGEventSourceRef;
    fn CGEventSourceFlagsState(state_id: i32) -> u64;
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: CGKeyCode,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> CGEventRef;
    fn CGEventCreateScrollWheelEvent(
        source: CGEventSourceRef,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
    ) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventSetType(event: CGEventRef, event_type: u32);
    fn CGEventSetLocation(event: CGEventRef, location: CGPoint);
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventSetDoubleValueField(event: CGEventRef, field: u32, value: f64);
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

fn ensure_post_event_access() -> Result<(), AppError> {
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

fn post_key_event(key: KeyCode, key_up: bool) -> Result<(), AppError> {
    let virtual_key = macos_key_code(key)?;
    let event = unsafe { CGEventCreateKeyboardEvent(std::ptr::null_mut(), virtual_key, !key_up) };
    post_event(event)
}

fn post_mouse_move_relative(dx: i32, dy: i32) -> Result<(), AppError> {
    let current = current_mouse_location()?;
    post_mouse_event(
        K_CG_EVENT_MOUSE_MOVED,
        K_CG_MOUSE_BUTTON_LEFT,
        CGPoint {
            x: current.x + f64::from(dx),
            y: current.y + f64::from(dy),
        },
    )
}

fn post_mouse_wheel(delta: i32) -> Result<(), AppError> {
    let line_delta = match delta / 120 {
        0 => delta.signum(),
        lines => lines,
    };
    let event = unsafe {
        CGEventCreateScrollWheelEvent(
            std::ptr::null_mut(),
            K_CG_SCROLL_EVENT_UNIT_LINE,
            1,
            line_delta,
        )
    };
    post_event(event)
}

fn post_mouse_button(button: MouseButton, down: bool) -> Result<(), AppError> {
    let point = current_mouse_location()?;
    post_mouse_event(mouse_event_type(button, down), mouse_button(button), point)
}

fn mouse_event_type(button: MouseButton, down: bool) -> u32 {
    match (button, down) {
        (MouseButton::Left, true) => K_CG_EVENT_LEFT_MOUSE_DOWN,
        (MouseButton::Left, false) => K_CG_EVENT_LEFT_MOUSE_UP,
        (MouseButton::Right, true) => K_CG_EVENT_RIGHT_MOUSE_DOWN,
        (MouseButton::Right, false) => K_CG_EVENT_RIGHT_MOUSE_UP,
    }
}

fn mouse_button(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => K_CG_MOUSE_BUTTON_LEFT,
        MouseButton::Right => K_CG_MOUSE_BUTTON_RIGHT,
    }
}

fn post_mouse_event(event_type: u32, button: u32, point: CGPoint) -> Result<(), AppError> {
    let event = unsafe { CGEventCreateMouseEvent(std::ptr::null_mut(), event_type, point, button) };
    post_event(event)
}

fn current_mouse_location() -> Result<CGPoint, AppError> {
    let event = unsafe { CGEventCreate(std::ptr::null_mut()) };
    if event.is_null() {
        return Err(AppError::DesktopShell(
            "failed to create CoreGraphics event".to_string(),
        ));
    }

    let point = unsafe { CGEventGetLocation(event) };
    unsafe { CFRelease(event) };
    Ok(point)
}

fn post_event(event: CGEventRef) -> Result<(), AppError> {
    if event.is_null() {
        return Err(AppError::DesktopShell(
            "failed to create CoreGraphics event".to_string(),
        ));
    }

    unsafe {
        CGEventPost(K_CG_HID_EVENT_TAP, event);
        CFRelease(event);
    }
    Ok(())
}

struct MacosTabletState {
    event_source: CGEventSourceRef,
    is_contact: bool,
    last_point: Option<CGPoint>,
    is_in_proximity: bool,
}

// SAFETY: `MacosTabletState` exclusively owns its retained `CGEventSourceRef`. CoreGraphics event
// sources have no thread affinity, and every access is serialized through `WindowsPenInjector`'s
// mutex, so moving the state to another thread cannot create concurrent access or double release.
unsafe impl Send for MacosTabletState {}

impl MacosTabletState {
    fn new() -> Result<Self, AppError> {
        // SAFETY: The returned retained CoreGraphics object is exclusively owned by this state and
        // released in `Drop`.
        let event_source = unsafe { CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_PRIVATE) };
        if event_source.is_null() {
            return Err(AppError::DesktopShell(
                "failed to create macOS CoreGraphics event source".to_string(),
            ));
        }

        Ok(Self {
            event_source,
            is_contact: false,
            last_point: None,
            is_in_proximity: false,
        })
    }

    fn inject(&mut self, command: PenInjectionCommand) -> Result<(), AppError> {
        let point = CGPoint {
            x: f64::from(command.x),
            y: f64::from(command.y),
        };
        if command.in_range && !self.is_in_proximity {
            self.post_proximity_event(true)?;
            self.is_in_proximity = true;
        }

        let event_type = self.event_type(&command);
        let event = unsafe {
            CGEventCreateMouseEvent(self.event_source, event_type, point, K_CG_MOUSE_BUTTON_LEFT)
        };
        if event.is_null() {
            return Err(AppError::DesktopShell(
                "failed to create macOS tablet event".to_string(),
            ));
        }

        unsafe {
            CGEventSetLocation(event, point);
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_MOUSE_EVENT_BUTTON_NUMBER,
                i64::from(K_CG_MOUSE_BUTTON_LEFT),
            );
        }

        if let Some(last_point) = self.last_point {
            unsafe {
                CGEventSetDoubleValueField(
                    event,
                    K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_X,
                    point.x - last_point.x,
                );
                CGEventSetDoubleValueField(
                    event,
                    K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_Y,
                    point.y - last_point.y,
                );
            }
        }

        self.apply_tablet_values(event, &command);
        unsafe {
            CGEventPost(K_CG_HID_EVENT_TAP, event);
            CFRelease(event);
        }

        self.is_contact = command.is_contact
            && !matches!(
                command.kind,
                PenInjectionCommandKind::Up | PenInjectionCommandKind::Cancel
            );
        self.last_point = Some(point);
        if !command.in_range && self.is_in_proximity {
            self.post_proximity_event(false)?;
            self.is_in_proximity = false;
        }

        if matches!(command.kind, PenInjectionCommandKind::Cancel) || !command.in_range {
            self.is_contact = false;
            self.last_point = None;
        }

        Ok(())
    }

    fn event_type(&self, command: &PenInjectionCommand) -> u32 {
        match command.kind {
            PenInjectionCommandKind::Down => K_CG_EVENT_LEFT_MOUSE_DOWN,
            PenInjectionCommandKind::Up | PenInjectionCommandKind::Cancel => {
                K_CG_EVENT_LEFT_MOUSE_UP
            }
            PenInjectionCommandKind::Update if self.is_contact || command.is_contact => {
                K_CG_EVENT_LEFT_MOUSE_DRAGGED
            }
            PenInjectionCommandKind::Update => K_CG_EVENT_MOUSE_MOVED,
        }
    }

    fn post_proximity_event(&self, entering: bool) -> Result<(), AppError> {
        let event = unsafe { CGEventCreate(self.event_source) };
        if event.is_null() {
            return Err(AppError::DesktopShell(
                "failed to create macOS tablet proximity event".to_string(),
            ));
        }

        unsafe {
            CGEventSetType(event, K_CG_EVENT_TABLET_PROXIMITY);
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_MOUSE_EVENT_SUBTYPE,
                K_CG_MOUSE_EVENT_SUBTYPE_TABLET_PROXIMITY,
            );
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_PROXIMITY_ENTER_PROXIMITY,
                i64::from(entering),
            );
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_PROXIMITY_POINTER_TYPE,
                NS_POINTING_DEVICE_TYPE_PEN,
            );
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_PROXIMITY_CAPABILITY_MASK,
                WACOM_CAPABILITY_MASK,
            );
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_PROXIMITY_DEVICE_ID,
                MACOS_TABLET_DEVICE_ID,
            );
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_PROXIMITY_VENDOR_POINTER_TYPE,
                WACOM_VENDOR_POINTER_TYPE_GENERAL_STYLUS,
            );
            CGEventPost(K_CG_HID_EVENT_TAP, event);
            CFRelease(event);
        }

        Ok(())
    }

    fn apply_tablet_values(&self, event: CGEventRef, command: &PenInjectionCommand) {
        let pressure = if command.is_contact {
            (f64::from(command.pressure) / 1024.0).clamp(0.001, 1.0)
        } else {
            0.0
        };
        let buttons = if command.is_contact { 1 } else { 0 };

        unsafe {
            CGEventSetDoubleValueField(event, K_CG_EVENT_FIELD_MOUSE_EVENT_PRESSURE, pressure);
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_MOUSE_EVENT_SUBTYPE,
                K_CG_MOUSE_EVENT_SUBTYPE_TABLET_POINT,
            );
            CGEventSetIntegerValueField(event, K_CG_EVENT_FIELD_TABLET_POINT_BUTTONS, buttons);
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_DEVICE_ID,
                MACOS_TABLET_DEVICE_ID,
            );
            CGEventSetDoubleValueField(event, K_CG_EVENT_FIELD_TABLET_POINT_PRESSURE, pressure);
            CGEventSetDoubleValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_TILT_X,
                f64::from(command.tilt_x) / 90.0,
            );
            CGEventSetDoubleValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_TILT_Y,
                -f64::from(command.tilt_y) / 90.0,
            );
            CGEventSetFlags(
                event,
                CGEventSourceFlagsState(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE),
            );
        }
    }
}

impl Drop for MacosTabletState {
    fn drop(&mut self) {
        if self.is_in_proximity {
            let _ = self.post_proximity_event(false);
            self.is_in_proximity = false;
        }
        if !self.event_source.is_null() {
            unsafe { CFRelease(self.event_source) };
        }
    }
}

fn macos_key_code(key: KeyCode) -> Result<CGKeyCode, AppError> {
    let key_code = match key {
        KeyCode::Alt | KeyCode::AltLeft => 58,
        KeyCode::AltRight => 61,
        KeyCode::Space => 49,
        KeyCode::Shift | KeyCode::ShiftLeft => 56,
        KeyCode::ShiftRight => 60,
        KeyCode::Control | KeyCode::ControlLeft => 59,
        KeyCode::ControlRight => 62,
        KeyCode::MetaLeft => 55,
        KeyCode::MetaRight => 54,
        KeyCode::Enter => 36,
        KeyCode::Tab => 48,
        KeyCode::Escape => 53,
        KeyCode::Backspace => 51,
        KeyCode::Delete => 117,
        KeyCode::Insert => 114,
        KeyCode::Home => 115,
        KeyCode::End => 119,
        KeyCode::PageUp => 116,
        KeyCode::PageDown => 121,
        KeyCode::ArrowUp => 126,
        KeyCode::ArrowDown => 125,
        KeyCode::ArrowLeft => 123,
        KeyCode::ArrowRight => 124,
        KeyCode::CapsLock => 57,
        KeyCode::NumLock => 71,
        KeyCode::A => 0,
        KeyCode::B => 11,
        KeyCode::C => 8,
        KeyCode::D => 2,
        KeyCode::E => 14,
        KeyCode::F => 3,
        KeyCode::G => 5,
        KeyCode::H => 4,
        KeyCode::I => 34,
        KeyCode::J => 38,
        KeyCode::K => 40,
        KeyCode::L => 37,
        KeyCode::M => 46,
        KeyCode::N => 45,
        KeyCode::O => 31,
        KeyCode::P => 35,
        KeyCode::Q => 12,
        KeyCode::R => 15,
        KeyCode::S => 1,
        KeyCode::T => 17,
        KeyCode::U => 32,
        KeyCode::V => 9,
        KeyCode::W => 13,
        KeyCode::X => 7,
        KeyCode::Y => 16,
        KeyCode::Z => 6,
        KeyCode::Digit0 => 29,
        KeyCode::Digit1 => 18,
        KeyCode::Digit2 => 19,
        KeyCode::Digit3 => 20,
        KeyCode::Digit4 => 21,
        KeyCode::Digit5 => 23,
        KeyCode::Digit6 => 22,
        KeyCode::Digit7 => 26,
        KeyCode::Digit8 => 28,
        KeyCode::Digit9 => 25,
        KeyCode::BracketLeft => 33,
        KeyCode::BracketRight => 30,
        KeyCode::Backquote => 50,
        KeyCode::Minus => 27,
        KeyCode::Equal => 24,
        KeyCode::Backslash => 42,
        KeyCode::Semicolon => 41,
        KeyCode::Quote => 39,
        KeyCode::Comma => 43,
        KeyCode::Period => 47,
        KeyCode::Slash => 44,
        KeyCode::F1 => 122,
        KeyCode::F2 => 120,
        KeyCode::F3 => 99,
        KeyCode::F4 => 118,
        KeyCode::F5 => 96,
        KeyCode::F6 => 97,
        KeyCode::F7 => 98,
        KeyCode::F8 => 100,
        KeyCode::F9 => 101,
        KeyCode::F10 => 109,
        KeyCode::F11 => 103,
        KeyCode::F12 => 111,
        KeyCode::F13 => 105,
        KeyCode::F14 => 107,
        KeyCode::F15 => 113,
        KeyCode::F16 => 106,
        KeyCode::F17 => 64,
        KeyCode::F18 => 79,
        KeyCode::F19 => 80,
        KeyCode::F20 => 90,
        KeyCode::Numpad0 => 82,
        KeyCode::Numpad1 => 83,
        KeyCode::Numpad2 => 84,
        KeyCode::Numpad3 => 85,
        KeyCode::Numpad4 => 86,
        KeyCode::Numpad5 => 87,
        KeyCode::Numpad6 => 88,
        KeyCode::Numpad7 => 89,
        KeyCode::Numpad8 => 91,
        KeyCode::Numpad9 => 92,
        KeyCode::NumpadAdd => 69,
        KeyCode::NumpadSubtract => 78,
        KeyCode::NumpadMultiply => 67,
        KeyCode::NumpadDivide => 75,
        KeyCode::NumpadDecimal => 65,
        KeyCode::NumpadEnter => 76,
        KeyCode::VolumeMute => 74,
        KeyCode::VolumeDown => 73,
        KeyCode::VolumeUp => 72,
        KeyCode::ScrollLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::ContextMenu
        | KeyCode::F21
        | KeyCode::F22
        | KeyCode::F23
        | KeyCode::F24
        | KeyCode::MediaPreviousTrack
        | KeyCode::MediaNextTrack
        | KeyCode::MediaPlayPause
        | KeyCode::MediaStop
        | KeyCode::BrowserBack
        | KeyCode::BrowserForward
        | KeyCode::BrowserRefresh
        | KeyCode::BrowserStop
        | KeyCode::BrowserSearch
        | KeyCode::BrowserFavorites
        | KeyCode::BrowserHome => {
            return Err(AppError::UnsupportedShortcutKey {
                platform: "macOS",
                key: key.label(),
            });
        }
    };
    Ok(key_code)
}
