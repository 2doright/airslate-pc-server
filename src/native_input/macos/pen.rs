use std::sync::Mutex;

use crate::{
    error::AppError,
    input_pipeline::{PenInjectionCommand, PenInjectionCommandKind, PenInjector},
};

use super::core_graphics::{
    CFRelease, CGEventCreate, CGEventCreateMouseEvent, CGEventPost, CGEventRef,
    CGEventSetDoubleValueField, CGEventSetFlags, CGEventSetIntegerValueField, CGEventSetLocation,
    CGEventSetType, CGEventSourceCreate, CGEventSourceFlagsState, CGEventSourceRef, CGPoint,
    K_CG_EVENT_FIELD_MOUSE_EVENT_BUTTON_NUMBER, K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_X,
    K_CG_EVENT_FIELD_MOUSE_EVENT_DELTA_Y, K_CG_EVENT_FIELD_MOUSE_EVENT_PRESSURE,
    K_CG_EVENT_FIELD_MOUSE_EVENT_SUBTYPE, K_CG_EVENT_FIELD_TABLET_DEVICE_ID,
    K_CG_EVENT_FIELD_TABLET_POINT_BUTTONS, K_CG_EVENT_FIELD_TABLET_POINT_PRESSURE,
    K_CG_EVENT_FIELD_TABLET_POINT_X, K_CG_EVENT_FIELD_TABLET_POINT_Y,
    K_CG_EVENT_FIELD_TABLET_PROXIMITY_CAPABILITY_MASK, K_CG_EVENT_FIELD_TABLET_PROXIMITY_DEVICE_ID,
    K_CG_EVENT_FIELD_TABLET_PROXIMITY_ENTER_PROXIMITY,
    K_CG_EVENT_FIELD_TABLET_PROXIMITY_POINTER_TYPE,
    K_CG_EVENT_FIELD_TABLET_PROXIMITY_VENDOR_POINTER_TYPE, K_CG_EVENT_FIELD_TABLET_TILT_X,
    K_CG_EVENT_FIELD_TABLET_TILT_Y, K_CG_EVENT_LEFT_MOUSE_DOWN, K_CG_EVENT_LEFT_MOUSE_DRAGGED,
    K_CG_EVENT_LEFT_MOUSE_UP, K_CG_EVENT_MOUSE_MOVED, K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
    K_CG_EVENT_SOURCE_STATE_PRIVATE, K_CG_EVENT_TABLET_PROXIMITY, K_CG_HID_EVENT_TAP,
    K_CG_MOUSE_BUTTON_LEFT, K_CG_MOUSE_EVENT_SUBTYPE_TABLET_POINT,
    K_CG_MOUSE_EVENT_SUBTYPE_TABLET_PROXIMITY, MACOS_TABLET_DEVICE_ID, NS_POINTING_DEVICE_TYPE_PEN,
    WACOM_CAPABILITY_MASK, WACOM_VENDOR_POINTER_TYPE_GENERAL_STYLUS, ensure_post_event_access,
};

pub(super) struct MacosPenInjector {
    tablet: Mutex<MacosTabletState>,
}

impl MacosPenInjector {
    pub(super) fn new() -> Result<Self, AppError> {
        Ok(Self {
            tablet: Mutex::new(MacosTabletState::new()?),
        })
    }
}

impl PenInjector for MacosPenInjector {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
        ensure_post_event_access()?;
        self.tablet
            .lock()
            .map_err(|_| AppError::StatePoisoned("macos_tablet_injector"))?
            .inject(command)
    }
}

struct MacosTabletState {
    event_source: CGEventSourceRef,
    is_contact: bool,
    last_point: Option<CGPoint>,
    is_in_proximity: bool,
}

// SAFETY: `MacosTabletState` exclusively owns its retained `CGEventSourceRef`. CoreGraphics event
// sources have no thread affinity, and every access is serialized through `MacosPenInjector`'s
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
        // SAFETY: `event_source` is valid for the lifetime of this state and `point` is a
        // correctly laid out CoreGraphics CGPoint value.
        let event = unsafe {
            CGEventCreateMouseEvent(self.event_source, event_type, point, K_CG_MOUSE_BUTTON_LEFT)
        };
        if event.is_null() {
            return Err(AppError::DesktopShell(
                "failed to create macOS tablet event".to_string(),
            ));
        }

        // SAFETY: `event` is valid and owned here. The field constants and values match the
        // tablet-point subtype selected before tablet-specific fields are written.
        unsafe {
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_MOUSE_EVENT_SUBTYPE,
                K_CG_MOUSE_EVENT_SUBTYPE_TABLET_POINT,
            );
            CGEventSetLocation(event, point);
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_MOUSE_EVENT_BUTTON_NUMBER,
                i64::from(K_CG_MOUSE_BUTTON_LEFT),
            );
        }

        if let Some(last_point) = self.last_point {
            // SAFETY: `event` remains valid and the delta fields accept finite f64 values.
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
        // SAFETY: `event` is a valid retained CoreGraphics event. Posting does not consume it;
        // this releases the sole retained reference afterward.
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
        // SAFETY: `event_source` is a valid retained CoreGraphics event source.
        let event = unsafe { CGEventCreate(self.event_source) };
        if event.is_null() {
            return Err(AppError::DesktopShell(
                "failed to create macOS tablet proximity event".to_string(),
            ));
        }

        // SAFETY: `event` is valid and exclusively owned here. These constants are the
        // CoreGraphics tablet proximity fields used by the existing Wacom-compatible path.
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

        // SAFETY: `event` is a valid tablet-point event created by this state. Each field is
        // written with the same type and normalization used before this structural refactor.
        unsafe {
            CGEventSetDoubleValueField(event, K_CG_EVENT_FIELD_MOUSE_EVENT_PRESSURE, pressure);
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_POINT_X,
                i64::from(command.tablet_x),
            );
            CGEventSetIntegerValueField(
                event,
                K_CG_EVENT_FIELD_TABLET_POINT_Y,
                i64::from(command.tablet_y),
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
            // SAFETY: `event_source` is the retained object created in `new` and is released
            // exactly once during drop.
            unsafe { CFRelease(self.event_source) };
        }
    }
}
