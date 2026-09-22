use crate::{
    error::AppError,
    shortcut::{KeyCode, MouseButton, ShortcutCommand, ShortcutExecutor},
};

use super::core_graphics::{
    CGEventCreateKeyboardEvent, CGEventCreateMouseEvent, CGEventCreateScrollWheelEvent, CGKeyCode,
    CGPoint, K_CG_EVENT_LEFT_MOUSE_DOWN, K_CG_EVENT_LEFT_MOUSE_UP, K_CG_EVENT_MOUSE_MOVED,
    K_CG_EVENT_RIGHT_MOUSE_DOWN, K_CG_EVENT_RIGHT_MOUSE_UP, K_CG_HID_EVENT_TAP,
    K_CG_MOUSE_BUTTON_LEFT, K_CG_MOUSE_BUTTON_RIGHT, K_CG_SCROLL_EVENT_UNIT_LINE,
    current_mouse_location, ensure_post_event_access, post_event,
};

pub(super) struct MacosShortcutExecutor;

impl MacosShortcutExecutor {
    pub(super) fn new() -> Self {
        Self
    }
}

impl ShortcutExecutor for MacosShortcutExecutor {
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

fn post_key_event(key: KeyCode, key_up: bool) -> Result<(), AppError> {
    let virtual_key = macos_key_code(key)?;
    // SAFETY: A null event source requests the default CoreGraphics source. `virtual_key` is a
    // platform key code produced by `macos_key_code`.
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
    // SAFETY: A null source is supported, line units and a wheel count of one are valid, and
    // `line_delta` is the same normalized value used before this structural refactor.
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
    // SAFETY: A null source requests the default CoreGraphics source. `event_type`, `button`,
    // and `point` are constructed from the supported shortcut command variants above.
    let event = unsafe { CGEventCreateMouseEvent(std::ptr::null_mut(), event_type, point, button) };
    post_event(event)
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
