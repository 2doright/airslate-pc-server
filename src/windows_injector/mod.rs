#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::{WindowsPenInjector, WindowsShortcutExecutor};

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use std::{mem::size_of, sync::Mutex};

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HANDLE, HWND, POINT},
    UI::{
        Controls::{
            CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, HSYNTHETICPOINTERDEVICE,
            POINTER_FEEDBACK_DEFAULT, POINTER_TYPE_INFO, POINTER_TYPE_INFO_0,
        },
        Input::{
            KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
                KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
                MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
                MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VIRTUAL_KEY,
            },
            Pointer::{
                InjectSyntheticPointerInput, POINTER_FLAG_CANCELED, POINTER_FLAG_DOWN,
                POINTER_FLAG_FIRSTBUTTON, POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE,
                POINTER_FLAG_NEW, POINTER_FLAG_PRIMARY, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
                POINTER_FLAGS, POINTER_INFO, POINTER_PEN_INFO,
            },
        },
        WindowsAndMessaging::{
            GetSystemMetrics, PEN_FLAG_NONE, PEN_MASK_PRESSURE, PEN_MASK_TILT_X, PEN_MASK_TILT_Y,
            PT_PEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        },
    },
};

#[cfg(windows)]
use crate::{
    error::AppError,
    input_pipeline::{PenInjectionCommand, PenInjector},
    shortcut::{ShortcutCommand, ShortcutExecutor},
};

#[cfg(windows)]
use crate::{
    input_pipeline::PenInjectionCommandKind,
    shortcut::{KeyCode, MouseButton},
};

#[cfg(windows)]
const POINTER_ID: u32 = 1;

#[cfg(windows)]
pub struct WindowsPenInjector {
    device: usize,
    frame_id: Mutex<u32>,
}

#[cfg(windows)]
pub struct WindowsShortcutExecutor;

#[cfg(windows)]
impl WindowsPenInjector {
    pub fn new() -> Result<Self, AppError> {
        let device = unsafe { CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_DEFAULT) }?;

        Ok(Self {
            device: device.0 as usize,
            frame_id: Mutex::new(0),
        })
    }

    fn device_handle(&self) -> HSYNTHETICPOINTERDEVICE {
        HSYNTHETICPOINTERDEVICE(self.device as *mut c_void)
    }
}

#[cfg(windows)]
impl WindowsShortcutExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(windows)]
impl Drop for WindowsPenInjector {
    fn drop(&mut self) {
        unsafe {
            DestroySyntheticPointerDevice(self.device_handle());
        }
    }
}

#[cfg(windows)]
impl PenInjector for WindowsPenInjector {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
        let mut frame_id = self
            .frame_id
            .lock()
            .map_err(|_| AppError::StatePoisoned("windows_pen_injector"))?;
        *frame_id = frame_id.wrapping_add(1);

        let pointer_info = build_pointer_type_info(*frame_id, &command);
        unsafe { InjectSyntheticPointerInput(self.device_handle(), &[pointer_info]) }?;
        Ok(())
    }
}

#[cfg(windows)]
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

        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err(AppError::Io(std::io::Error::last_os_error()));
        }

        Ok(())
    }
}

#[cfg(windows)]
fn build_pointer_type_info(frame_id: u32, command: &PenInjectionCommand) -> POINTER_TYPE_INFO {
    let point = POINT {
        x: command.x,
        y: command.y,
    };

    let pointer_info = POINTER_INFO {
        pointerType: PT_PEN,
        pointerId: POINTER_ID,
        frameId: frame_id,
        pointerFlags: build_pointer_flags(command),
        sourceDevice: HANDLE::default(),
        hwndTarget: HWND::default(),
        ptPixelLocation: point,
        ptHimetricLocation: POINT::default(),
        ptPixelLocationRaw: point,
        ptHimetricLocationRaw: POINT::default(),
        dwTime: 0,
        historyCount: 0,
        InputData: 0,
        dwKeyStates: 0,
        PerformanceCount: 0,
        ButtonChangeType: Default::default(),
    };

    let pen_info = POINTER_PEN_INFO {
        pointerInfo: pointer_info,
        penFlags: PEN_FLAG_NONE,
        penMask: PEN_MASK_PRESSURE | PEN_MASK_TILT_X | PEN_MASK_TILT_Y,
        pressure: command.pressure,
        rotation: 0,
        tiltX: command.tilt_x,
        tiltY: command.tilt_y,
    };

    POINTER_TYPE_INFO {
        r#type: PT_PEN,
        Anonymous: POINTER_TYPE_INFO_0 { penInfo: pen_info },
    }
}

#[cfg(windows)]
fn build_pointer_flags(command: &PenInjectionCommand) -> POINTER_FLAGS {
    let mut bits = POINTER_FLAG_PRIMARY.0;

    if command.in_range {
        bits |= POINTER_FLAG_INRANGE.0;
    }

    if command.is_contact {
        bits |= POINTER_FLAG_INCONTACT.0 | POINTER_FLAG_FIRSTBUTTON.0;
    }

    bits |= match command.kind {
        PenInjectionCommandKind::Down => POINTER_FLAG_NEW.0 | POINTER_FLAG_DOWN.0,
        PenInjectionCommandKind::Update => POINTER_FLAG_UPDATE.0,
        PenInjectionCommandKind::Up => POINTER_FLAG_UP.0,
        PenInjectionCommandKind::Cancel => POINTER_FLAG_CANCELED.0 | POINTER_FLAG_UP.0,
    };

    POINTER_FLAGS(bits)
}

#[cfg(windows)]
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

#[cfg(windows)]
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

#[cfg(windows)]
fn mouse_move_relative_input(dx: i32, dy: i32) -> INPUT {
    mouse_input(dx, dy, 0, MOUSEEVENTF_MOVE)
}

#[cfg(windows)]
fn mouse_wheel_input(delta: i32) -> INPUT {
    mouse_input(0, 0, delta, MOUSEEVENTF_WHEEL)
}

#[cfg(windows)]
fn mouse_button_input(button: MouseButton, down: bool) -> INPUT {
    let flags = match (button, down) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
    };
    mouse_input(0, 0, 0, flags)
}

#[cfg(windows)]
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

#[cfg(windows)]
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

#[cfg(windows)]
fn absolute_mouse_coords(x: i32, y: i32) -> (i32, i32) {
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1);
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(1);

    let normalized_x = (((x - left) as i64) * 65_535 / i64::from(width - 1).max(1)) as i32;
    let normalized_y = (((y - top) as i64) * 65_535 / i64::from(height - 1).max(1)) as i32;
    (normalized_x, normalized_y)
}

#[cfg(windows)]
fn virtual_key(key: KeyCode) -> VIRTUAL_KEY {
    VIRTUAL_KEY(key.virtual_key())
}

#[cfg(all(test, windows))]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    };

    use super::*;

    #[test]
    fn down_command_sets_expected_pointer_flags() {
        let flags = build_pointer_flags(&PenInjectionCommand {
            x: 100,
            y: 200,
            kind: PenInjectionCommandKind::Down,
            in_range: true,
            is_contact: true,
            pressure: 512,
            tilt_x: 0,
            tilt_y: 0,
        });

        assert_eq!(
            flags.0,
            POINTER_FLAG_PRIMARY.0
                | POINTER_FLAG_INRANGE.0
                | POINTER_FLAG_INCONTACT.0
                | POINTER_FLAG_FIRSTBUTTON.0
                | POINTER_FLAG_NEW.0
                | POINTER_FLAG_DOWN.0
        );
    }

    #[test]
    fn cancel_command_sets_cancel_and_up_flags() {
        let flags = build_pointer_flags(&PenInjectionCommand {
            x: 100,
            y: 200,
            kind: PenInjectionCommandKind::Cancel,
            in_range: false,
            is_contact: false,
            pressure: 0,
            tilt_x: 0,
            tilt_y: 0,
        });

        assert_eq!(
            flags.0,
            POINTER_FLAG_PRIMARY.0 | POINTER_FLAG_CANCELED.0 | POINTER_FLAG_UP.0
        );
    }

    #[test]
    fn mouse_move_input_uses_move_flag() {
        let input = mouse_move_relative_input(12, -6);
        unsafe {
            assert_eq!(input.Anonymous.mi.dwFlags, MOUSEEVENTF_MOVE);
            assert_eq!(input.Anonymous.mi.dx, 12);
            assert_eq!(input.Anonymous.mi.dy, -6);
        }
    }

    #[test]
    fn mouse_wheel_input_uses_wheel_flag() {
        let input = mouse_wheel_input(120);
        unsafe {
            assert_eq!(input.Anonymous.mi.dwFlags, MOUSEEVENTF_WHEEL);
            assert_eq!(input.Anonymous.mi.mouseData, 120);
        }
    }

    #[test]
    fn mouse_button_inputs_use_right_button_flags() {
        let down = mouse_button_input(MouseButton::Right, true);
        let up = mouse_button_input(MouseButton::Right, false);
        unsafe {
            assert_eq!(down.Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTDOWN);
            assert_eq!(up.Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTUP);
        }
    }

    #[test]
    fn mouse_button_inputs_use_left_button_flags() {
        let down = mouse_button_input(MouseButton::Left, true);
        let up = mouse_button_input(MouseButton::Left, false);
        unsafe {
            assert_eq!(down.Anonymous.mi.dwFlags, MOUSEEVENTF_LEFTDOWN);
            assert_eq!(up.Anonymous.mi.dwFlags, MOUSEEVENTF_LEFTUP);
        }
    }

    #[test]
    fn right_click_at_builds_move_down_up_sequence() {
        let inputs = build_click_at_inputs(MouseButton::Right, 300, 400);
        assert_eq!(inputs.len(), 3);
        unsafe {
            assert!(inputs[0].Anonymous.mi.dwFlags.contains(MOUSEEVENTF_MOVE));
            assert_eq!(inputs[1].Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTDOWN);
            assert_eq!(inputs[2].Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTUP);
        }
    }
}
