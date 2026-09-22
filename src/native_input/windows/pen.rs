use std::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};

use windows::Win32::{
    Foundation::{HANDLE, HWND, POINT},
    UI::{
        Controls::{
            CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, HSYNTHETICPOINTERDEVICE,
            POINTER_FEEDBACK_DEFAULT, POINTER_TYPE_INFO, POINTER_TYPE_INFO_0,
        },
        Input::Pointer::{
            InjectSyntheticPointerInput, POINTER_FLAG_CANCELED, POINTER_FLAG_DOWN,
            POINTER_FLAG_FIRSTBUTTON, POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE,
            POINTER_FLAG_NEW, POINTER_FLAG_PRIMARY, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
            POINTER_FLAGS, POINTER_INFO, POINTER_PEN_INFO,
        },
        WindowsAndMessaging::{
            GetSystemMetrics, PEN_FLAG_NONE, PEN_MASK_PRESSURE, PEN_MASK_TILT_X, PEN_MASK_TILT_Y,
            PT_PEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        },
    },
};

use crate::{
    error::AppError,
    input_pipeline::{PenInjectionCommand, PenInjectionCommandKind, PenInjector},
};

const POINTER_ID: u32 = 1;

pub(super) struct WindowsPenInjector {
    device: usize,
    frame_id: AtomicU32,
}

impl WindowsPenInjector {
    pub(super) fn new() -> Result<Self, AppError> {
        // SAFETY: PT_PEN with a maximum count of one and the feedback mode are valid API values.
        let device = unsafe { CreateSyntheticPointerDevice(PT_PEN, 1, POINTER_FEEDBACK_DEFAULT) }?;

        Ok(Self {
            device: device.0 as usize,
            frame_id: AtomicU32::new(0),
        })
    }

    fn device_handle(&self) -> HSYNTHETICPOINTERDEVICE {
        HSYNTHETICPOINTERDEVICE(self.device as *mut c_void)
    }
}

impl Drop for WindowsPenInjector {
    fn drop(&mut self) {
        // SAFETY: `device` came from a successful creation call and is destroyed exactly once.
        unsafe {
            DestroySyntheticPointerDevice(self.device_handle());
        }
    }
}

impl PenInjector for WindowsPenInjector {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
        let frame_id = self
            .frame_id
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);

        let pointer_info = build_pointer_type_info(frame_id, &command);
        // SAFETY: the device handle is valid for `self` and the slice contains one initialized
        // PT_PEN value, matching the device's configured pointer type and count.
        unsafe { InjectSyntheticPointerInput(self.device_handle(), &[pointer_info]) }?;
        Ok(())
    }
}

fn build_pointer_type_info(frame_id: u32, command: &PenInjectionCommand) -> POINTER_TYPE_INFO {
    // InjectSyntheticPointerInput uses coordinates relative to the virtual screen's top-left,
    // while the input pipeline uses desktop coordinates returned by GetMonitorInfoW.
    // SAFETY: GetSystemMetrics only reads process-wide system metrics and receives valid constants.
    let virtual_left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    // SAFETY: GetSystemMetrics only reads process-wide system metrics and receives valid constants.
    let virtual_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let point = synthetic_pointer_location(command.x, command.y, virtual_left, virtual_top);

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
        historyCount: 1,
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

fn synthetic_pointer_location(
    desktop_x: i32,
    desktop_y: i32,
    virtual_left: i32,
    virtual_top: i32,
) -> POINT {
    POINT {
        x: desktop_x - virtual_left,
        y: desktop_y - virtual_top,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn down_command_sets_expected_pointer_flags() {
        let flags = build_pointer_flags(&PenInjectionCommand {
            x: 100,
            y: 200,
            tablet_x: 100,
            tablet_y: 200,
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
    fn pointer_info_reports_one_input_history_entry() {
        let info = build_pointer_type_info(
            1,
            &PenInjectionCommand {
                x: 100,
                y: 200,
                tablet_x: 100,
                tablet_y: 200,
                kind: PenInjectionCommandKind::Down,
                in_range: true,
                is_contact: true,
                pressure: 512,
                tilt_x: 0,
                tilt_y: 0,
            },
        );

        // SAFETY: `info` was built as a PT_PEN value, so reading its active penInfo union
        // variant is valid.
        unsafe {
            assert_eq!(info.Anonymous.penInfo.pointerInfo.historyCount, 1);
        }
    }

    #[test]
    fn synthetic_pointer_location_is_relative_to_negative_virtual_screen_origin() {
        let point = synthetic_pointer_location(0, 0, -2560, -360);

        assert_eq!((point.x, point.y), (2560, 360));
    }

    #[test]
    fn cancel_command_sets_cancel_and_up_flags() {
        let flags = build_pointer_flags(&PenInjectionCommand {
            x: 100,
            y: 200,
            tablet_x: 100,
            tablet_y: 200,
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
}
