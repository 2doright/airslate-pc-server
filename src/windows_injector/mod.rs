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
                INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
                MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
                MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VIRTUAL_KEY,
                VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9, VK_A, VK_B, VK_BACK,
                VK_C, VK_CONTROL, VK_D, VK_DELETE, VK_E, VK_ESCAPE, VK_F, VK_G, VK_H, VK_I, VK_J,
                VK_K, VK_L, VK_M, VK_MENU, VK_N, VK_O, VK_OEM_4, VK_OEM_6, VK_P, VK_Q, VK_R,
                VK_RETURN, VK_S, VK_SHIFT, VK_SPACE, VK_T, VK_TAB, VK_U, VK_V, VK_W, VK_X, VK_Y,
                VK_Z,
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

#[cfg(not(target_os = "macos"))]
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

#[cfg(all(not(windows), not(target_os = "macos")))]
use tracing::{debug, warn};

#[cfg(windows)]
const POINTER_ID: u32 = 1;

#[cfg(windows)]
pub struct WindowsPenInjector {
    device: usize,
    frame_id: Mutex<u32>,
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub struct WindowsPenInjector {}

#[cfg(not(target_os = "macos"))]
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

#[cfg(all(not(windows), not(target_os = "macos")))]
impl WindowsPenInjector {
    pub fn new() -> Result<Self, AppError> {
        warn!("native pen injection is not implemented on this platform");
        Ok(Self {})
    }
}

#[cfg(not(target_os = "macos"))]
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

#[cfg(all(not(windows), not(target_os = "macos")))]
impl PenInjector for WindowsPenInjector {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
        debug!(?command, "skipping pen injection on this platform");
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
            ShortcutCommand::RightClickAt { x, y } => build_right_click_at_inputs(x, y),
        };

        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err(AppError::Io(std::io::Error::last_os_error()));
        }

        Ok(())
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
impl ShortcutExecutor for WindowsShortcutExecutor {
    fn execute(&self, command: ShortcutCommand) -> Result<(), AppError> {
        debug!(?command, "skipping shortcut execution on this platform");
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
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
    };
    mouse_input(0, 0, 0, flags)
}

#[cfg(windows)]
fn build_right_click_at_inputs(x: i32, y: i32) -> Vec<INPUT> {
    let (dx, dy) = absolute_mouse_coords(x, y);
    vec![
        mouse_input(
            dx,
            dy,
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        ),
        mouse_button_input(MouseButton::Right, true),
        mouse_button_input(MouseButton::Right, false),
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
    match key {
        KeyCode::Alt => VK_MENU,
        KeyCode::Space => VK_SPACE,
        KeyCode::Shift => VK_SHIFT,
        KeyCode::Control => VK_CONTROL,
        KeyCode::Enter => VK_RETURN,
        KeyCode::Tab => VK_TAB,
        KeyCode::Escape => VK_ESCAPE,
        KeyCode::Backspace => VK_BACK,
        KeyCode::Delete => VK_DELETE,
        KeyCode::A => VK_A,
        KeyCode::B => VK_B,
        KeyCode::C => VK_C,
        KeyCode::D => VK_D,
        KeyCode::E => VK_E,
        KeyCode::F => VK_F,
        KeyCode::G => VK_G,
        KeyCode::H => VK_H,
        KeyCode::I => VK_I,
        KeyCode::J => VK_J,
        KeyCode::K => VK_K,
        KeyCode::L => VK_L,
        KeyCode::M => VK_M,
        KeyCode::N => VK_N,
        KeyCode::O => VK_O,
        KeyCode::P => VK_P,
        KeyCode::Q => VK_Q,
        KeyCode::R => VK_R,
        KeyCode::S => VK_S,
        KeyCode::T => VK_T,
        KeyCode::U => VK_U,
        KeyCode::V => VK_V,
        KeyCode::W => VK_W,
        KeyCode::X => VK_X,
        KeyCode::Y => VK_Y,
        KeyCode::Z => VK_Z,
        KeyCode::Digit0 => VK_0,
        KeyCode::Digit1 => VK_1,
        KeyCode::Digit2 => VK_2,
        KeyCode::Digit3 => VK_3,
        KeyCode::Digit4 => VK_4,
        KeyCode::Digit5 => VK_5,
        KeyCode::Digit6 => VK_6,
        KeyCode::Digit7 => VK_7,
        KeyCode::Digit8 => VK_8,
        KeyCode::Digit9 => VK_9,
        KeyCode::BracketLeft => VK_OEM_4,
        KeyCode::BracketRight => VK_OEM_6,
    }
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
    fn right_click_at_builds_move_down_up_sequence() {
        let inputs = build_right_click_at_inputs(300, 400);
        assert_eq!(inputs.len(), 3);
        unsafe {
            assert!(inputs[0].Anonymous.mi.dwFlags.contains(MOUSEEVENTF_MOVE));
            assert_eq!(inputs[1].Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTDOWN);
            assert_eq!(inputs[2].Anonymous.mi.dwFlags, MOUSEEVENTF_RIGHTUP);
        }
    }
}
