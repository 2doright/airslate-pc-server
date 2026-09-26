use std::fs::File;

use input_linux::{
    AbsoluteAxis, AbsoluteInfo, AbsoluteInfoSetup, EventKind, InputId, Key, sys,
    uinput::UInputHandle,
};

use super::super::{AXIS_MAX, io_error};
use crate::error::AppError;

/// Wacom USB identity. libwacom's udev hwdb matches vendor 0x056A / product 0x0084 and
/// tags the device `ID_INPUT_TABLET=1`; a generic identity makes libinput ignore the
/// tablet and applications never observe pressure.
const TABLET_VENDOR: u16 = 0x056A;
const TABLET_PRODUCT: u16 = 0x0084;
const TABLET_VERSION: u16 = 1;
const TABLET_NAME: &str = "AirSlate Virtual Tablet";
const PRESSURE_MAX: i32 = 1_024;
/// Physical resolution of the position axes in units per millimeter. libinput's
/// `tablet_reject_device` ignores any tablet whose ABS_X/ABS_Y report no
/// resolution (`evdev_device_get_size` fails on the faked resolution), so the
/// position axes must declare one; 100 units/mm is the value libinput's own
/// test suite uses for virtual tablets.
const AXIS_RESOLUTION: i32 = 100;
/// Tilt range in degrees, matching the Wacom convention libinput maps onto its
/// ±64° tool-tilt range. The injector clamps into it because Linux drawing
/// applications never observe tilt beyond that range.
pub(super) const TILT_MIN: i32 = -64;
pub(super) const TILT_MAX: i32 = 64;

pub(super) fn create_tablet() -> Result<UInputHandle<File>, AppError> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
        .map_err(|error| io_error("failed to open /dev/uinput", error))?;
    let device = UInputHandle::new(file);

    device
        .set_evbit(EventKind::Key)
        .map_err(|error| io_error("failed to enable EV_KEY on the virtual tablet", error))?;
    for key in [
        Key::ButtonToolPen,
        Key::ButtonTouch,
        Key::ButtonStylus,
        Key::ButtonStylus2,
    ] {
        device
            .set_keybit(key)
            .map_err(|error| io_error("failed to declare a virtual tablet key", error))?;
    }

    device
        .set_evbit(EventKind::Absolute)
        .map_err(|error| io_error("failed to enable EV_ABS on the virtual tablet", error))?;
    for axis in [
        AbsoluteAxis::X,
        AbsoluteAxis::Y,
        AbsoluteAxis::Pressure,
        AbsoluteAxis::TiltX,
        AbsoluteAxis::TiltY,
    ] {
        device
            .set_absbit(axis)
            .map_err(|error| io_error("failed to declare a virtual tablet axis", error))?;
    }

    let id = InputId {
        bustype: sys::BUS_USB,
        vendor: TABLET_VENDOR,
        product: TABLET_PRODUCT,
        version: TABLET_VERSION,
    };
    device
        .create(&id, TABLET_NAME.as_bytes(), 0, &tablet_axes())
        .map_err(|error| io_error("failed to create the virtual tablet device", error))?;

    Ok(device)
}

/// Axis setup table for the virtual tablet. Split out from `create_tablet` so
/// the resolution invariants libinput depends on stay unit-testable.
fn tablet_axes() -> [AbsoluteInfoSetup; 5] {
    [
        abs_setup(AbsoluteAxis::X, 0, AXIS_MAX, AXIS_RESOLUTION),
        abs_setup(AbsoluteAxis::Y, 0, AXIS_MAX, AXIS_RESOLUTION),
        // Pressure is normalized by libinput against the axis range alone.
        abs_setup(AbsoluteAxis::Pressure, 0, PRESSURE_MAX, 0),
        // Tilt must report resolution 0: a non-zero value tells libinput the raw
        // value is radians, while 0 selects the degree convention.
        abs_setup(AbsoluteAxis::TiltX, TILT_MIN, TILT_MAX, 0),
        abs_setup(AbsoluteAxis::TiltY, TILT_MIN, TILT_MAX, 0),
    ]
}

fn abs_setup(axis: AbsoluteAxis, minimum: i32, maximum: i32, resolution: i32) -> AbsoluteInfoSetup {
    AbsoluteInfoSetup {
        axis,
        info: AbsoluteInfo {
            value: 0,
            minimum,
            maximum,
            fuzz: 0,
            flat: 0,
            resolution,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_axes_declare_resolution_for_libinput() {
        let axes = tablet_axes();
        for axis in [AbsoluteAxis::X, AbsoluteAxis::Y] {
            let setup = axes
                .iter()
                .find(|setup| setup.axis.code() == axis.code())
                .expect("position axis");
            assert!(
                setup.info.resolution > 0,
                "libinput ignores tablets whose ABS_X/ABS_Y report no resolution"
            );
        }
    }

    #[test]
    fn tilt_axes_declare_zero_resolution_for_degrees() {
        let axes = tablet_axes();
        for axis in [AbsoluteAxis::TiltX, AbsoluteAxis::TiltY] {
            let setup = axes
                .iter()
                .find(|setup| setup.axis.code() == axis.code())
                .expect("tilt axis");
            assert_eq!(setup.info.resolution, 0);
            assert_eq!(setup.info.minimum, TILT_MIN);
            assert_eq!(setup.info.maximum, TILT_MAX);
        }
    }
}
