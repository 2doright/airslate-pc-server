use std::mem::size_of;

use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK,
        MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VIRTUAL_KEY,
    },
    WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    },
};

use crate::{
    error::AppError,
    shortcut::{KeyCode, MouseButton, ShortcutCommand, ShortcutExecutor},
};

pub(super) struct WindowsShortcutExecutor;

impl WindowsShortcutExecutor {
    pub(super) fn new() -> Self {
        Self
    }
}

impl ShortcutExecutor for WindowsShortcutExecutor {
    fn execute(&self, command: ShortcutCommand) -> Result<(), AppError> {
        let inputs = match command {
            ShortcutCommand::KeyDown(key) => vec![keyboard_input(key, false)],
            ShortcutCommand::KeyUp(key) => vec![keyboard_input(key, true)],
            ShortcutCommand::PressChord(keys) => build_chord_inputs(&keys),
            ShortcutCommand::MouseMoveRelative { dx, dy } => {
                vec![mouse_move_relative_input(dx, dy)]
            }
            ShortcutCommand::MouseWheel { delta } => vec![mouse_wheel_input(delta)],
            ShortcutCommand::MouseButtonDown(button) => vec![mouse_button_input(button, true)],
            ShortcutCommand::MouseButtonUp(button) => vec![mouse_button_input(button, false)],
            ShortcutCommand::ClickAt { button, x, y } => build_click_at_inputs(button, x, y),
        };

        // SAFETY: `inputs` is an initialized contiguous slice and the element size matches INPUT.
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err(AppError::Io(std::io::Error::last_os_error()));
        }

        Ok(())
    }
}

fn build_chord_inputs(keys: &[KeyCode]) -> Vec<INPUT> {
    let mut inputs = Vec::with_capacity(keys.len() * 2);
    for &key in keys {
        inputs.push(keyboard_input(key, false));
    }
    for &key in keys.iter().rev() {
        inputs.push(keyboard_input(key, true));
    }
    inputs
}

fn keyboard_input(key: KeyCode, key_up: bool) -> INPUT {
    let mut flags = Default::default();
    if key.is_extended() {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if key_up {
        flags |= KEYEVENTF_KEYUP;
    }

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key(key),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse_move_relative_input(dx: i32, dy: i32) -> INPUT {
    mouse_input(dx, dy, 0, MOUSEEVENTF_MOVE)
}

fn mouse_wheel_input(delta: i32) -> INPUT {
    mouse_input(0, 0, delta, MOUSEEVENTF_WHEEL)
}

fn mouse_button_input(button: MouseButton, down: bool) -> INPUT {
    let flags = match (button, down) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
    };
    mouse_input(0, 0, 0, flags)
}

fn build_click_at_inputs(button: MouseButton, x: i32, y: i32) -> Vec<INPUT> {
    let (dx, dy) = absolute_mouse_coords(x, y);
    vec![
        mouse_input(
            dx,
            dy,
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        ),
        mouse_button_input(button, true),
        mouse_button_input(button, false),
    ]
}

fn mouse_input(
    dx: i32,
    dy: i32,
    mouse_data: i32,
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn absolute_mouse_coords(x: i32, y: i32) -> (i32, i32) {
    // SAFETY: GetSystemMetrics only reads process-wide system metrics and receives valid constants.
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    // SAFETY: GetSystemMetrics only reads process-wide system metrics and receives valid constants.
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    // SAFETY: GetSystemMetrics only reads process-wide system metrics and receives valid constants.
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1);
    // SAFETY: GetSystemMetrics only reads process-wide system metrics and receives valid constants.
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(1);

    let normalized_x = (((x - left) as i64) * 65_535 / i64::from(width - 1).max(1)) as i32;
    let normalized_y = (((y - top) as i64) * 65_535 / i64::from(height - 1).max(1)) as i32;
    (normalized_x, normalized_y)
}

fn virtual_key(key: KeyCode) -> VIRTUAL_KEY {
    VIRTUAL_KEY(key.virtual_key())
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    };

    use super::*;

    #[test]
    fn mouse_move_input_uses_move_flag() {
        let input = mouse_move_relative_input(12, -6);
        // SAFETY: `input` was constructed with the mouse union variant by this module.
        unsafe {
            assert_eq!(input.Anonymous.mi.dwFlags, MOUSEEVENTF_MOVE);
            assert_eq!(input.Anonymous.mi.dx, 12);
            assert_eq!(input.Anonymous.mi.dy, -6);
        }
    }

    #[test]
    fn mouse_wheel_input_uses_wheel_flag() {
        let input = mouse_wheel_input(120);
        // SAFETY: `input` was constructed with the mouse union variant by this module.
        unsafe {
            assert_eq!(input.Anonymous.mi.dwFlags, MOUSEEVENTF_WHEEL);
            assert_eq!(input.Anonymous.mi.mouseData, 120);
        }
    }

    #[test]
    fn mouse_button_inputs_use_right_button_flags() {
        let down = mouse_button_input(MouseButton::Right, true);
        let up = mouse_button_input(MouseButton::Right, false);
        // SAFETY: both values were constructed with the mouse union variant by this module.
        unsafe {
            assert_eq!(down.Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTDOWN);
            assert_eq!(up.Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTUP);
        }
    }

    #[test]
    fn mouse_button_inputs_use_left_button_flags() {
        let down = mouse_button_input(MouseButton::Left, true);
        let up = mouse_button_input(MouseButton::Left, false);
        // SAFETY: both values were constructed with the mouse union variant by this module.
        unsafe {
            assert_eq!(down.Anonymous.mi.dwFlags, MOUSEEVENTF_LEFTDOWN);
            assert_eq!(up.Anonymous.mi.dwFlags, MOUSEEVENTF_LEFTUP);
        }
    }

    #[test]
    fn right_click_at_builds_move_down_up_sequence() {
        let inputs = build_click_at_inputs(MouseButton::Right, 300, 400);
        assert_eq!(inputs.len(), 3);
        // SAFETY: all values were constructed with the mouse union variant by this module.
        unsafe {
            assert!(inputs[0].Anonymous.mi.dwFlags.contains(MOUSEEVENTF_MOVE));
            assert_eq!(inputs[1].Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTDOWN);
            assert_eq!(inputs[2].Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTUP);
        }
    }
}
