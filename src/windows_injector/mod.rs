#[cfg(any(windows, target_os = "macos"))]
use std::ffi::c_void;
#[cfg(windows)]
use std::{mem::size_of, sync::Mutex};
#[cfg(target_os = "macos")]
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

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

use crate::{
    error::AppError,
    input_pipeline::{PenInjectionCommand, PenInjector},
    shortcut::{ShortcutCommand, ShortcutExecutor},
};

#[cfg(target_os = "macos")]
use crate::input_pipeline::PenInjectionCommandKind;
#[cfg(target_os = "macos")]
use crate::shortcut::{KeyCode, MouseButton};
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

#[cfg(not(windows))]
pub struct WindowsPenInjector {
    #[cfg(target_os = "macos")]
    tablet: Mutex<MacosTabletState>,
}

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

#[cfg(not(windows))]
impl WindowsPenInjector {
    pub fn new() -> Result<Self, AppError> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                tablet: Mutex::new(MacosTabletState::new()),
            })
        }
        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            warn!("native pen injection is not implemented on this platform");
            Ok(Self {})
        }
    }
}

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

#[cfg(target_os = "macos")]
impl PenInjector for WindowsPenInjector {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
        self.tablet
            .lock()
            .map_err(|_| AppError::StatePoisoned("macos_tablet_injector"))?
            .inject(command)
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

#[cfg(target_os = "macos")]
impl ShortcutExecutor for WindowsShortcutExecutor {
    fn execute(&self, command: ShortcutCommand) -> Result<(), AppError> {
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
            ShortcutCommand::RightClickAt { x, y } => {
                let point = CGPoint {
                    x: f64::from(x),
                    y: f64::from(y),
                };
                post_mouse_event(K_CG_EVENT_MOUSE_MOVED, K_CG_MOUSE_BUTTON_LEFT, point)?;
                post_mouse_event(K_CG_EVENT_RIGHT_MOUSE_DOWN, K_CG_MOUSE_BUTTON_RIGHT, point)?;
                post_mouse_event(K_CG_EVENT_RIGHT_MOUSE_UP, K_CG_MOUSE_BUTTON_RIGHT, point)
            }
        }
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

#[cfg(target_os = "macos")]
type CGEventRef = *mut c_void;
#[cfg(target_os = "macos")]
type CGEventSourceRef = *mut c_void;
#[cfg(target_os = "macos")]
type CGKeyCode = u16;

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
const K_CG_HID_EVENT_TAP: u32 = 0;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
#[cfg(target_os = "macos")]
const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
#[cfg(target_os = "macos")]
const K_CG_EVENT_TABLET_PROXIMITY: u32 = 24;
#[cfg(target_os = "macos")]
const K_CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
#[cfg(target_os = "macos")]
const K_CG_MOUSE_BUTTON_RIGHT: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_SOURCE_STATE_PRIVATE: i32 = -1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: i32 = 1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_MOUSE_EVENT_PRESSURE: u32 = 2;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_MOUSE_EVENT_BUTTON_NUMBER: u32 = 3;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_X: u32 = 4;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_Y: u32 = 5;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_MOUSE_EVENT_SUBTYPE: u32 = 7;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_POINT_BUTTONS: u32 = 18;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_POINT_PRESSURE: u32 = 19;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_TILT_X: u32 = 20;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_TILT_Y: u32 = 21;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_DEVICE_ID: u32 = 24;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_DEVICE_ID: u32 = 31;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_VENDOR_POINTER_TYPE: u32 = 33;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_CAPABILITY_MASK: u32 = 36;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_POINTER_TYPE: u32 = 37;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FIELD_TABLET_PROXIMITY_ENTER_PROXIMITY: u32 = 38;
#[cfg(target_os = "macos")]
const K_CG_MOUSE_EVENT_SUBTYPE_TABLET_POINT: i64 = 1;
#[cfg(target_os = "macos")]
const K_CG_MOUSE_EVENT_SUBTYPE_TABLET_PROXIMITY: i64 = 2;
#[cfg(target_os = "macos")]
const NS_POINTING_DEVICE_TYPE_PEN: i64 = 1;
#[cfg(target_os = "macos")]
const WACOM_CAPABILITY_MASK: i64 = 0x001 | 0x002 | 0x004 | 0x040 | 0x080 | 0x100 | 0x400;
#[cfg(target_os = "macos")]
const WACOM_VENDOR_POINTER_TYPE_GENERAL_STYLUS: i64 = 0x802;
#[cfg(target_os = "macos")]
const MACOS_TABLET_DEVICE_ID: i64 = 5_303_613_955_435_230_461;
#[cfg(target_os = "macos")]
const PROXIMITY_REFRESH_INTERVAL: Duration = Duration::from_millis(200);

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
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
    fn CGEventSetType(event: CGEventRef, event_type: u32) -> CGEventRef;
    fn CGEventSetLocation(event: CGEventRef, location: CGPoint);
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64) -> CGEventRef;
    fn CGEventSetDoubleValueField(event: CGEventRef, field: u32, value: f64);
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

#[cfg(target_os = "macos")]
fn post_key_event(key: KeyCode, key_up: bool) -> Result<(), AppError> {
    let event =
        unsafe { CGEventCreateKeyboardEvent(std::ptr::null_mut(), macos_key_code(key), !key_up) };
    post_event(event)
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn post_mouse_wheel(delta: i32) -> Result<(), AppError> {
    let event = unsafe {
        CGEventCreateScrollWheelEvent(
            std::ptr::null_mut(),
            K_CG_SCROLL_EVENT_UNIT_LINE,
            1,
            delta / 120,
        )
    };
    post_event(event)
}

#[cfg(target_os = "macos")]
fn post_mouse_button(button: MouseButton, down: bool) -> Result<(), AppError> {
    let point = current_mouse_location()?;
    match button {
        MouseButton::Right => post_mouse_event(
            if down {
                K_CG_EVENT_RIGHT_MOUSE_DOWN
            } else {
                K_CG_EVENT_RIGHT_MOUSE_UP
            },
            K_CG_MOUSE_BUTTON_RIGHT,
            point,
        ),
    }
}

#[cfg(target_os = "macos")]
fn post_mouse_event(event_type: u32, button: u32, point: CGPoint) -> Result<(), AppError> {
    let event = unsafe { CGEventCreateMouseEvent(std::ptr::null_mut(), event_type, point, button) };
    post_event(event)
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
struct MacosTabletState {
    event_source: CGEventSourceRef,
    is_contact: bool,
    last_point: Option<CGPoint>,
    last_proximity: Option<Instant>,
}

#[cfg(target_os = "macos")]
unsafe impl Send for MacosTabletState {}

#[cfg(target_os = "macos")]
impl MacosTabletState {
    fn new() -> Self {
        Self {
            event_source: unsafe { CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_PRIVATE) },
            is_contact: false,
            last_point: None,
            last_proximity: None,
        }
    }

    fn inject(&mut self, command: PenInjectionCommand) -> Result<(), AppError> {
        let point = CGPoint {
            x: f64::from(command.x),
            y: f64::from(command.y),
        };
        if command.in_range {
            self.ensure_proximity()?;
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
        if matches!(command.kind, PenInjectionCommandKind::Cancel) || !command.in_range {
            self.is_contact = false;
            self.last_point = None;
            self.last_proximity = None;
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

    fn ensure_proximity(&mut self) -> Result<(), AppError> {
        let should_post = self
            .last_proximity
            .map(|instant| instant.elapsed() > PROXIMITY_REFRESH_INTERVAL)
            .unwrap_or(true);
        if !should_post {
            return Ok(());
        }

        self.post_proximity_event()?;
        self.last_proximity = Some(Instant::now());
        Ok(())
    }

    fn post_proximity_event(&self) -> Result<(), AppError> {
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
                1,
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

#[cfg(target_os = "macos")]
impl Drop for MacosTabletState {
    fn drop(&mut self) {
        if !self.event_source.is_null() {
            unsafe { CFRelease(self.event_source) };
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_key_code(key: KeyCode) -> CGKeyCode {
    match key {
        KeyCode::Alt => 58,
        KeyCode::Space => 49,
        KeyCode::Shift => 56,
        KeyCode::Control => 59,
        KeyCode::Enter => 36,
        KeyCode::Tab => 48,
        KeyCode::Escape => 53,
        KeyCode::Backspace => 51,
        KeyCode::Delete => 117,
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
