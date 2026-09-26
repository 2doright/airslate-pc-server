use std::{fs::File, sync::Mutex};

use input_linux::{AbsoluteAxis, EventKind, Key, sys, uinput::UInputHandle};

use super::{ScreenGeometry, event, syn_event, write_events};
use crate::{
    error::AppError,
    input_pipeline::{PenInjectionCommand, PenInjectionCommandKind, PenInjector},
    workspace::WorkspaceService,
};
use device::{TILT_MAX, TILT_MIN, create_tablet};

mod device;

pub(super) struct LinuxPenInjector {
    workspace: WorkspaceService,
    state: Mutex<PenState>,
}

impl LinuxPenInjector {
    pub(super) fn new(workspace: WorkspaceService) -> Result<Self, AppError> {
        Ok(Self {
            workspace,
            state: Mutex::new(PenState::new()?),
        })
    }
}

impl PenInjector for LinuxPenInjector {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
        let geometry = ScreenGeometry::from_workspace(&self.workspace)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AppError::StatePoisoned("linux_pen_injector"))?;
        let events = state.proximity.build_events(&command, &geometry);
        write_events(
            &state.device,
            &events,
            "failed to write virtual tablet events",
        )
    }
}

/// Owns the virtual tablet device and the tool-proximity state the kernel requires.
struct PenState {
    device: UInputHandle<File>,
    proximity: ProximityState,
}

impl PenState {
    fn new() -> Result<Self, AppError> {
        Ok(Self {
            device: create_tablet()?,
            proximity: ProximityState::default(),
        })
    }
}

impl Drop for PenState {
    fn drop(&mut self) {
        // The kernel keeps the virtual device registered until UI_DEV_DESTROY; closing
        // the fd alone only releases the handle.
        let _ = self.device.dev_destroy();
    }
}

/// Tracks whether BTN_TOOL_PEN is currently asserted so proximity transitions are
/// emitted exactly once, mirroring the macOS backend's proximity state machine.
#[derive(Default)]
struct ProximityState {
    in_proximity: bool,
}

impl ProximityState {
    /// Builds the event frame for one pen command: a single SYN_REPORT, plus a second
    /// frame when the pen leaves proximity.
    fn build_events(
        &mut self,
        command: &PenInjectionCommand,
        geometry: &ScreenGeometry,
    ) -> Vec<sys::input_event> {
        let mut events = Vec::with_capacity(9);

        if command.in_range && !self.in_proximity {
            events.push(key_event(Key::ButtonToolPen, 1));
            self.in_proximity = true;
        }

        match command.kind {
            PenInjectionCommandKind::Down => {
                events.push(key_event(Key::ButtonTouch, 1));
                push_pen_axes(&mut events, command, geometry);
            }
            PenInjectionCommandKind::Update => {
                events.push(key_event(Key::ButtonTouch, i32::from(command.is_contact)));
                push_pen_axes(&mut events, command, geometry);
            }
            PenInjectionCommandKind::Up => {
                events.push(key_event(Key::ButtonTouch, 0));
            }
            PenInjectionCommandKind::Cancel => {
                events.push(key_event(Key::ButtonTouch, 0));
                events.push(key_event(Key::ButtonToolPen, 0));
                events.push(abs_event(AbsoluteAxis::Pressure, 0));
                self.in_proximity = false;
            }
        }
        events.push(syn_event());

        // Leaving proximity needs its own frame so the tool release is not merged away.
        if !command.in_range && self.in_proximity {
            events.push(key_event(Key::ButtonToolPen, 0));
            self.in_proximity = false;
            events.push(syn_event());
        }

        events
    }
}

fn push_pen_axes(
    events: &mut Vec<sys::input_event>,
    command: &PenInjectionCommand,
    geometry: &ScreenGeometry,
) {
    events.push(abs_event(AbsoluteAxis::X, geometry.axis_x(command.x)));
    events.push(abs_event(AbsoluteAxis::Y, geometry.axis_y(command.y)));
    events.push(abs_event(AbsoluteAxis::Pressure, command.pressure as i32));
    events.push(abs_event(
        AbsoluteAxis::TiltX,
        command.tilt_x.clamp(TILT_MIN, TILT_MAX),
    ));
    events.push(abs_event(
        AbsoluteAxis::TiltY,
        command.tilt_y.clamp(TILT_MIN, TILT_MAX),
    ));
}

fn key_event(key: Key, value: i32) -> sys::input_event {
    event(EventKind::Key, key.code(), value)
}

fn abs_event(axis: AbsoluteAxis, value: i32) -> sys::input_event {
    event(EventKind::Absolute, axis.code(), value)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn command(
        kind: PenInjectionCommandKind,
        in_range: bool,
        is_contact: bool,
    ) -> PenInjectionCommand {
        PenInjectionCommand {
            x: 0,
            y: 0,
            tablet_x: 0,
            tablet_y: 0,
            kind,
            in_range,
            is_contact,
            pressure: 512,
            tilt_x: 15,
            tilt_y: -15,
        }
    }

    fn build(
        kind: PenInjectionCommandKind,
        in_range: bool,
        is_contact: bool,
    ) -> (Vec<sys::input_event>, bool) {
        let mut proximity = ProximityState::default();
        let events = proximity.build_events(&command(kind, in_range, is_contact), &geometry());
        (events, proximity.in_proximity)
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
    fn down_frame_reports_proximity_tip_and_axes() {
        let (events, in_proximity) = build(PenInjectionCommandKind::Down, true, true);
        assert!(in_proximity);
        assert_eq!(syn_count(&events), 1);
        assert!(has(&events, Key::ButtonToolPen.code(), 1));
        assert!(has(&events, Key::ButtonTouch.code(), 1));
        assert!(has(&events, AbsoluteAxis::Pressure.code(), 512));
        assert!(has(&events, AbsoluteAxis::TiltX.code(), 15));
        assert!(has(&events, AbsoluteAxis::TiltY.code(), -15));
    }

    #[test]
    fn update_frame_clears_tip_when_hovering() {
        let (events, _) = build(PenInjectionCommandKind::Update, true, false);
        assert_eq!(syn_count(&events), 1);
        assert!(has(&events, Key::ButtonTouch.code(), 0));
    }

    #[test]
    fn up_frame_keeps_tool_pen_while_in_range() {
        let (events, in_proximity) = build(PenInjectionCommandKind::Up, true, false);
        assert!(in_proximity);
        assert_eq!(syn_count(&events), 1);
        assert!(!has(&events, Key::ButtonToolPen.code(), 0));
    }

    #[test]
    fn leaving_range_adds_a_tool_release_frame() {
        let mut proximity = ProximityState::default();
        let down = command(PenInjectionCommandKind::Down, true, true);
        proximity.build_events(&down, &geometry());
        let up = command(PenInjectionCommandKind::Up, false, false);
        let events = proximity.build_events(&up, &geometry());
        let in_proximity = proximity.in_proximity;
        assert!(!in_proximity);
        assert_eq!(syn_count(&events), 2);
        assert!(has(&events, Key::ButtonToolPen.code(), 0));
    }

    #[test]
    fn cancel_frame_releases_tip_tool_and_pressure() {
        let (events, in_proximity) = build(PenInjectionCommandKind::Cancel, false, false);
        assert!(!in_proximity);
        assert_eq!(syn_count(&events), 1);
        assert!(has(&events, Key::ButtonTouch.code(), 0));
        assert!(has(&events, Key::ButtonToolPen.code(), 0));
        assert!(has(&events, AbsoluteAxis::Pressure.code(), 0));
    }

    #[test]
    fn proximity_enters_only_once() {
        let mut proximity = ProximityState::default();
        let down = command(PenInjectionCommandKind::Down, true, true);
        let update = command(PenInjectionCommandKind::Update, true, true);
        let first = proximity.build_events(&down, &geometry());
        let second = proximity.build_events(&update, &geometry());
        assert_eq!(
            first
                .iter()
                .filter(|event| event.code == Key::ButtonToolPen.code())
                .count(),
            1
        );
        assert_eq!(
            second
                .iter()
                .filter(|event| event.code == Key::ButtonToolPen.code())
                .count(),
            0
        );
    }

    #[test]
    fn coordinates_map_into_the_axis_range() {
        let geometry = geometry();
        assert_eq!(geometry.axis_x(0), 0);
        assert_eq!(geometry.axis_x(1919), 32_767);
        assert_eq!(geometry.axis_x(1920), 32_767);
        assert_eq!(geometry.axis_x(-100), 0);
        assert_eq!(geometry.axis_y(960), 960 * 32_767 / 1_079);
    }

    #[test]
    fn degenerate_extent_does_not_divide_by_zero() {
        let monitor = MonitorInfo {
            id: MonitorId::new("degenerate".to_string()),
            device_name: "DEGENERATE".to_string(),
            is_primary: true,
            pixel_width: 1,
            pixel_height: 1,
            virtual_left: 0,
            virtual_top: 0,
            virtual_right: 1,
            virtual_bottom: 1,
        };
        let geometry =
            ScreenGeometry::from_workspace(&WorkspaceService::from_snapshot(WorkspaceSnapshot {
                monitors: vec![monitor.clone()],
                active_monitor_id: Some(monitor.id.clone()),
                active_workspace: Some(ActiveWorkspace { monitor }),
            }))
            .expect("geometry");
        assert_eq!(geometry.axis_x(0), 0);
        assert_eq!(geometry.axis_x(1), 32_767);
    }

    #[test]
    fn tilt_clamps_to_the_wacom_range() {
        let geometry = geometry();
        let command = PenInjectionCommand {
            tilt_x: 90,
            tilt_y: -90,
            ..command(PenInjectionCommandKind::Down, true, true)
        };
        let mut events = Vec::new();
        push_pen_axes(&mut events, &command, &geometry);
        assert!(has(&events, AbsoluteAxis::TiltX.code(), TILT_MAX));
        assert!(has(&events, AbsoluteAxis::TiltY.code(), TILT_MIN));
    }
}
