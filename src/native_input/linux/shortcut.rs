use std::{fs::File, sync::Mutex};

use input_linux::{AbsoluteAxis, EventKind, InputId, Key, RelativeAxis, sys, uinput::UInputHandle};

use super::{AXIS_MAX, ScreenGeometry, event, io_error, syn_event, write_events};
use crate::{
    error::AppError,
    shortcut::{ShortcutCommand, ShortcutExecutor},
    workspace::WorkspaceService,
};

mod keys;

/// Generic USB identity: udev tags the device `ID_INPUT=1`, `ID_INPUT_KEY=1` and
/// `ID_INPUT_MOUSE=1`. BTN_TOUCH is deliberately not declared so libinput classifies
/// the device as a mouse instead of a tablet or touchscreen.
const KEYBOARD_VENDOR: u16 = 0x1234;
const KEYBOARD_PRODUCT: u16 = 0x5678;
const KEYBOARD_VERSION: u16 = 1;
const KEYBOARD_NAME: &str = "AirSlate Virtual Keyboard";
/// One wheel notch in the Windows `WM_MOUSEWHEEL` units the shortcut engine emits.
const WHEEL_NOTCH: i32 = 120;

pub(super) struct LinuxShortcutExecutor {
    workspace: WorkspaceService,
    device: Mutex<ShortcutDevice>,
}

impl LinuxShortcutExecutor {
    pub(super) fn new(workspace: WorkspaceService) -> Self {
        Self {
            workspace,
            device: Mutex::new(ShortcutDevice::new()),
        }
    }
}

impl ShortcutExecutor for LinuxShortcutExecutor {
    fn execute(&self, command: ShortcutCommand) -> Result<(), AppError> {
        let geometry = ScreenGeometry::from_workspace(&self.workspace)?;
        let device = self
            .device
            .lock()
            .map_err(|_| AppError::StatePoisoned("linux_shortcut_executor"))?;
        let events = build_events(&command, &geometry)?;
        device.write(&events, "failed to write virtual keyboard events")
    }
}

/// The virtual keyboard device. `create_shortcut_executor` cannot fail, so a device
/// creation failure is retained and replayed on every `execute` call instead of
/// pretending the device exists.
enum ShortcutDevice {
    Ready(UInputHandle<File>),
    Failed(String),
}

impl ShortcutDevice {
    fn new() -> Self {
        match create_keyboard_device() {
            Ok(device) => Self::Ready(device),
            Err(error) => Self::Failed(error.to_string()),
        }
    }

    fn write(&self, events: &[sys::input_event], context: &str) -> Result<(), AppError> {
        match self {
            Self::Ready(device) => write_events(device, events, context),
            Self::Failed(message) => Err(AppError::Io(std::io::Error::other(message.clone()))),
        }
    }
}

impl Drop for ShortcutDevice {
    fn drop(&mut self) {
        if let Self::Ready(device) = self {
            let _ = device.dev_destroy();
        }
    }
}

fn create_keyboard_device() -> Result<UInputHandle<File>, AppError> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
        .map_err(|error| io_error("failed to open /dev/uinput", error))?;
    let device = UInputHandle::new(file);

    device
        .set_evbit(EventKind::Key)
        .map_err(|error| io_error("failed to enable EV_KEY on the virtual keyboard", error))?;
    for key in key_capabilities() {
        device
            .set_keybit(key)
            .map_err(|error| io_error("failed to declare a virtual keyboard key", error))?;
    }

    device
        .set_evbit(EventKind::Relative)
        .map_err(|error| io_error("failed to enable EV_REL on the virtual keyboard", error))?;
    for axis in [RelativeAxis::X, RelativeAxis::Y, RelativeAxis::Wheel] {
        device
            .set_relbit(axis)
            .map_err(|error| io_error("failed to declare a virtual keyboard axis", error))?;
    }

    device
        .set_evbit(EventKind::Absolute)
        .map_err(|error| io_error("failed to enable EV_ABS on the virtual keyboard", error))?;
    for axis in [AbsoluteAxis::X, AbsoluteAxis::Y] {
        device
            .set_absbit(axis)
            .map_err(|error| io_error("failed to declare a virtual keyboard axis", error))?;
    }

    let id = InputId {
        bustype: sys::BUS_USB,
        vendor: KEYBOARD_VENDOR,
        product: KEYBOARD_PRODUCT,
        version: KEYBOARD_VERSION,
    };
    let axes = [
        abs_setup(AbsoluteAxis::X, 0, AXIS_MAX),
        abs_setup(AbsoluteAxis::Y, 0, AXIS_MAX),
    ];
    device
        .create(&id, KEYBOARD_NAME.as_bytes(), 0, &axes)
        .map_err(|error| io_error("failed to create the virtual keyboard device", error))?;

    Ok(device)
}

fn abs_setup(axis: AbsoluteAxis, minimum: i32, maximum: i32) -> input_linux::AbsoluteInfoSetup {
    input_linux::AbsoluteInfoSetup {
        axis,
        info: input_linux::AbsoluteInfo {
            value: 0,
            minimum,
            maximum,
            fuzz: 0,
            flat: 0,
            resolution: 0,
        },
    }
}

fn key_capabilities() -> Vec<Key> {
    let mut keys: Vec<Key> = keys::ALL_KEY_CODES
        .iter()
        .map(|&key| keys::linux_key(key))
        .collect();
    keys.extend([Key::ButtonLeft, Key::ButtonRight]);
    keys
}

fn build_events(
    command: &ShortcutCommand,
    geometry: &ScreenGeometry,
) -> Result<Vec<sys::input_event>, AppError> {
    let mut events = Vec::new();
    match command {
        ShortcutCommand::KeyDown(key) => {
            events.push(key_event(keys::linux_key(*key), 1));
            events.push(syn_event());
        }
        ShortcutCommand::KeyUp(key) => {
            events.push(key_event(keys::linux_key(*key), 0));
            events.push(syn_event());
        }
        ShortcutCommand::PressChord(keys) => {
            for key in keys {
                events.push(key_event(keys::linux_key(*key), 1));
            }
            events.push(syn_event());
            for key in keys.iter().rev() {
                events.push(key_event(keys::linux_key(*key), 0));
            }
            events.push(syn_event());
        }
        ShortcutCommand::MouseMoveRelative { dx, dy } => {
            events.push(rel_event(RelativeAxis::X, *dx));
            events.push(rel_event(RelativeAxis::Y, *dy));
            events.push(syn_event());
        }
        ShortcutCommand::MouseWheel { delta } => {
            events.push(rel_event(RelativeAxis::Wheel, delta / WHEEL_NOTCH));
            events.push(syn_event());
        }
        ShortcutCommand::MouseButtonDown(button) => {
            events.push(key_event(keys::button_key(*button), 1));
            events.push(syn_event());
        }
        ShortcutCommand::MouseButtonUp(button) => {
            events.push(key_event(keys::button_key(*button), 0));
            events.push(syn_event());
        }
        ShortcutCommand::ClickAt { button, x, y } => {
            events.push(abs_event(AbsoluteAxis::X, geometry.axis_x(*x)));
            events.push(abs_event(AbsoluteAxis::Y, geometry.axis_y(*y)));
            events.push(syn_event());
            events.push(key_event(keys::button_key(*button), 1));
            events.push(syn_event());
            events.push(key_event(keys::button_key(*button), 0));
            events.push(syn_event());
        }
    }
    Ok(events)
}

fn key_event(key: Key, value: i32) -> sys::input_event {
    event(EventKind::Key, key.code(), value)
}

fn abs_event(axis: AbsoluteAxis, value: i32) -> sys::input_event {
    event(EventKind::Absolute, axis.code(), value)
}

fn rel_event(axis: RelativeAxis, value: i32) -> sys::input_event {
    event(EventKind::Relative, axis.code(), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::shortcut::{KeyCode, MouseButton};
    use crate::workspace::{
        ActiveWorkspace, MonitorId, MonitorInfo, WorkspaceService, WorkspaceSnapshot,
    };

    fn geometry() -> ScreenGeometry {
        let monitor = MonitorInfo {
            id: MonitorId::new("default".to_string()),
            device_name: "DEFAULT".to_string(),
            is_primary: true,
            pixel_width: 1920,
            pixel_height: 1080,
            virtual_left: 0,
            virtual_top: 0,
            virtual_right: 1920,
            virtual_bottom: 1080,
        };
        ScreenGeometry::from_workspace(&WorkspaceService::from_snapshot(WorkspaceSnapshot {
            monitors: vec![monitor.clone()],
            active_monitor_id: Some(monitor.id.clone()),
            active_workspace: Some(ActiveWorkspace { monitor }),
        }))
        .expect("geometry")
    }

    fn build(command: ShortcutCommand) -> Vec<sys::input_event> {
        build_events(&command, &geometry()).expect("events")
    }

    fn syn_count(events: &[sys::input_event]) -> usize {
        events
            .iter()
            .filter(|event| event.type_ == EventKind::Synchronize.code())
            .count()
    }

    fn has(events: &[sys::input_event], code: u16, value: i32) -> bool {
        events
            .iter()
            .any(|event| event.code == code && event.value == value)
    }

    #[test]
    fn key_down_and_up_emit_single_frames() {
        let down = build(ShortcutCommand::KeyDown(KeyCode::A));
        assert_eq!(syn_count(&down), 1);
        assert!(has(&down, Key::A.code(), 1));
        let up = build(ShortcutCommand::KeyUp(KeyCode::A));
        assert_eq!(syn_count(&up), 1);
        assert!(has(&up, Key::A.code(), 0));
    }

    #[test]
    fn chord_presses_in_order_and_releases_reversed() {
        let events = build(ShortcutCommand::PressChord(vec![
            KeyCode::Control,
            KeyCode::Shift,
            KeyCode::Z,
        ]));
        assert_eq!(syn_count(&events), 2);
        let codes: Vec<u16> = events
            .iter()
            .filter(|event| event.type_ == EventKind::Key.code())
            .map(|event| event.code)
            .collect();
        assert_eq!(
            codes,
            vec![
                Key::LeftCtrl.code(),
                Key::LeftShift.code(),
                Key::Z.code(),
                Key::Z.code(),
                Key::LeftShift.code(),
                Key::LeftCtrl.code(),
            ]
        );
    }

    #[test]
    fn wheel_converts_windows_notches() {
        let events = build(ShortcutCommand::MouseWheel { delta: 240 });
        assert_eq!(syn_count(&events), 1);
        assert!(has(&events, RelativeAxis::Wheel.code(), 2));
        let negative = build(ShortcutCommand::MouseWheel { delta: -120 });
        assert!(has(&negative, RelativeAxis::Wheel.code(), -1));
    }

    #[test]
    fn relative_move_emits_both_axes() {
        let events = build(ShortcutCommand::MouseMoveRelative { dx: 12, dy: -6 });
        assert_eq!(syn_count(&events), 1);
        assert!(has(&events, RelativeAxis::X.code(), 12));
        assert!(has(&events, RelativeAxis::Y.code(), -6));
    }

    #[test]
    fn click_at_moves_then_clicks_in_three_frames() {
        let events = build(ShortcutCommand::ClickAt {
            button: MouseButton::Right,
            x: 960,
            y: 540,
        });
        assert_eq!(syn_count(&events), 3);
        assert!(has(&events, AbsoluteAxis::X.code(), 960 * 32_767 / 1_919));
        assert!(has(&events, AbsoluteAxis::Y.code(), 540 * 32_767 / 1_079));
        assert!(has(&events, Key::ButtonRight.code(), 1));
        assert!(has(&events, Key::ButtonRight.code(), 0));
    }

    #[test]
    fn capability_list_covers_every_key_code() {
        let capabilities = key_capabilities();
        for key in keys::ALL_KEY_CODES {
            assert!(capabilities.contains(&keys::linux_key(*key)));
        }
        assert!(capabilities.contains(&Key::ButtonLeft));
        assert!(capabilities.contains(&Key::ButtonRight));
    }
}
