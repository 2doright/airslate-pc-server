use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
};

use crate::app::state::{PRESSURE_LUT_SIZE, SharedPressureSettings};

use tracing::{info, warn};

use crate::{
    error::AppError,
    protocol::{GestureFrame, StylusEventType, StylusFlags, StylusFrame},
    shortcut::{
        ScreenPoint, SharedRadialMenuOverlay, SharedShortcutProfile, ShortcutExecutor,
        ShortcutRuntime,
    },
    udp_ingest::{IncomingEvent, IncomingEventSink},
    workspace::{MonitorInfo, WorkspaceService},
};

const LOGICAL_COORD_MAX: u16 = 32_767;
const WINDOWS_PRESSURE_MAX: u32 = 1_024;
const WINDOWS_TILT_MIN: i32 = -90;
const WINDOWS_TILT_MAX: i32 = 90;

pub trait PenInjector: Send + Sync {
    fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenInjectionCommandKind {
    Down,
    Update,
    Up,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PenInjectionCommand {
    pub x: i32,
    pub y: i32,
    pub kind: PenInjectionCommandKind,
    pub in_range: bool,
    pub is_contact: bool,
    pub pressure: u32,
    pub tilt_x: i32,
    pub tilt_y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivePenState {
    session_id: String,
    x: i32,
    y: i32,
    in_range: bool,
    is_contact: bool,
}

enum StylusWorkerEvent {
    Stylus {
        session_id: String,
        frame: StylusFrame,
    },
    SessionEnded {
        session_id: String,
    },
    Stop,
}

enum ShortcutWorkerEvent {
    PointerContextUpdated {
        session_id: String,
        point: ScreenPoint,
        in_range: bool,
        is_contact: bool,
    },
    StylusFlags {
        session_id: String,
        seq: u32,
        flags: StylusFlags,
    },
    Gesture {
        session_id: String,
        frame: GestureFrame,
    },
    SessionEnded {
        session_id: String,
    },
    Stop,
}

pub struct StylusInputPipeline {
    stylus_sender: Sender<StylusWorkerEvent>,
    shortcut_sender: Sender<ShortcutWorkerEvent>,
    stylus_worker: Mutex<Option<JoinHandle<()>>>,
    shortcut_worker: Mutex<Option<JoinHandle<()>>>,
    workspace: WorkspaceService,
    pressure_settings: SharedPressureSettings,
}

impl StylusInputPipeline {
    pub fn new(
        workspace: WorkspaceService,
        injector: Arc<dyn PenInjector>,
        shortcut_executor: Arc<dyn ShortcutExecutor>,
        pressure_settings: SharedPressureSettings,
        shortcut_profile: SharedShortcutProfile,
        radial_overlay: SharedRadialMenuOverlay,
    ) -> Self {
        let (stylus_sender, stylus_receiver) = mpsc::channel();
        let (shortcut_sender, shortcut_receiver) = mpsc::channel();

        let stylus_workspace = workspace.clone();
        let stylus_pressure_settings = pressure_settings.clone();
        let stylus_worker = thread::spawn(move || {
            StylusWorker::new(stylus_workspace, injector, stylus_pressure_settings)
                .run(stylus_receiver);
        });
        let shortcut_workspace = workspace.clone();
        let shortcut_worker = thread::spawn(move || {
            ShortcutWorker::new(
                shortcut_executor,
                shortcut_profile,
                radial_overlay,
                shortcut_workspace,
            )
            .run(shortcut_receiver);
        });

        Self {
            stylus_sender,
            shortcut_sender,
            stylus_worker: Mutex::new(Some(stylus_worker)),
            shortcut_worker: Mutex::new(Some(shortcut_worker)),
            workspace,
            pressure_settings,
        }
    }

    fn send_stylus(&self, event: StylusWorkerEvent) {
        if self.stylus_sender.send(event).is_err() {
            warn!("stylus worker queue is unavailable");
        }
    }

    fn send_shortcut(&self, event: ShortcutWorkerEvent) {
        if self.shortcut_sender.send(event).is_err() {
            warn!("shortcut worker queue is unavailable");
        }
    }

    fn current_monitor(&self) -> Result<MonitorInfo, AppError> {
        self.workspace
            .current_workspace()
            .map(|workspace| workspace.monitor)
    }
}

impl IncomingEventSink for StylusInputPipeline {
    fn emit(&self, event: IncomingEvent) {
        match event {
            IncomingEvent::Stylus {
                session_id, frame, ..
            } => {
                let shortcut_session_id = session_id.clone();
                let seq = frame.seq;
                let flags = frame.flags;

                if let Ok(monitor) = self.current_monitor() {
                    let pressure = self
                        .pressure_settings
                        .read()
                        .map(|settings| settings.map_pressure(frame.pressure))
                        .unwrap_or_else(|_| default_pressure(frame.pressure));
                    let pointer = map_stylus_frame(&monitor, &frame, pressure);
                    self.send_shortcut(ShortcutWorkerEvent::PointerContextUpdated {
                        session_id: shortcut_session_id.clone(),
                        point: ScreenPoint {
                            x: pointer.x,
                            y: pointer.y,
                        },
                        in_range: pointer.in_range,
                        is_contact: pointer.is_contact,
                    });
                } else {
                    warn!(session_id = %shortcut_session_id, "failed to resolve active workspace for shortcut pointer context");
                }

                self.send_stylus(StylusWorkerEvent::Stylus { session_id, frame });

                if has_shortcut_pulse(flags) {
                    self.send_shortcut(ShortcutWorkerEvent::StylusFlags {
                        session_id: shortcut_session_id,
                        seq,
                        flags,
                    });
                }
            }
            IncomingEvent::Gesture {
                session_id,
                source_ip,
                frame,
            } => {
                info!(
                    session_id = %session_id,
                    source_ip = %source_ip,
                    seq = frame.seq,
                    "stage 7 gesture frame received"
                );
                self.send_shortcut(ShortcutWorkerEvent::Gesture { session_id, frame });
            }
            IncomingEvent::SessionEnded { session_id, .. } => {
                self.send_stylus(StylusWorkerEvent::SessionEnded {
                    session_id: session_id.clone(),
                });
                self.send_shortcut(ShortcutWorkerEvent::SessionEnded { session_id });
            }
        }
    }
}

impl Drop for StylusInputPipeline {
    fn drop(&mut self) {
        let _ = self.stylus_sender.send(StylusWorkerEvent::Stop);
        let _ = self.shortcut_sender.send(ShortcutWorkerEvent::Stop);

        join_worker(&self.stylus_worker, "stylus queue worker");
        join_worker(&self.shortcut_worker, "shortcut queue worker");
    }
}

struct StylusWorker {
    workspace: WorkspaceService,
    injector: Arc<dyn PenInjector>,
    pressure_settings: SharedPressureSettings,
    state: Option<ActivePenState>,
    ended_sessions: HashSet<String>,
}

impl StylusWorker {
    fn new(
        workspace: WorkspaceService,
        injector: Arc<dyn PenInjector>,
        pressure_settings: SharedPressureSettings,
    ) -> Self {
        Self {
            workspace,
            injector,
            pressure_settings,
            state: None,
            ended_sessions: HashSet::new(),
        }
    }

    fn run(mut self, receiver: Receiver<StylusWorkerEvent>) {
        while let Ok(event) = receiver.recv() {
            match event {
                StylusWorkerEvent::Stylus { session_id, frame } => {
                    self.handle_stylus(session_id, frame)
                }
                StylusWorkerEvent::SessionEnded { session_id } => {
                    self.handle_session_end(session_id)
                }
                StylusWorkerEvent::Stop => break,
            }
        }
    }

    fn handle_stylus(&mut self, session_id: String, frame: StylusFrame) {
        if self.ended_sessions.contains(&session_id) {
            info!(session_id = %session_id, seq = frame.seq, "ignored stale stylus frame after session end");
            return;
        }

        let monitor = match self.workspace.current_workspace() {
            Ok(workspace) => workspace.monitor,
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "failed to resolve active workspace for stylus frame");
                return;
            }
        };

        let pressure = self
            .pressure_settings
            .read()
            .map(|settings| settings.map_pressure(frame.pressure))
            .unwrap_or_else(|_| default_pressure(frame.pressure));
        let command = map_stylus_frame(&monitor, &frame, pressure);
        self.record_command_state(&session_id, &command);

        if let Err(error) = self.injector.inject(command.clone()) {
            warn!(
                session_id = %session_id,
                seq = frame.seq,
                error = %error,
                "failed to inject stylus frame"
            );
            return;
        }

        info!(
            session_id = %session_id,
            seq = frame.seq,
            x = command.x,
            y = command.y,
            kind = ?command.kind,
            in_range = command.in_range,
            is_contact = command.is_contact,
            "stage 6 stylus frame injected"
        );
    }

    fn handle_session_end(&mut self, session_id: String) {
        self.ended_sessions.insert(session_id.clone());

        let Some(active) = self.state.take() else {
            return;
        };

        if active.session_id != session_id {
            self.state = Some(active);
            return;
        }

        if !active.in_range && !active.is_contact {
            return;
        }

        let command = PenInjectionCommand {
            x: active.x,
            y: active.y,
            kind: PenInjectionCommandKind::Cancel,
            in_range: false,
            is_contact: false,
            pressure: 0,
            tilt_x: 0,
            tilt_y: 0,
        };

        if let Err(error) = self.injector.inject(command) {
            warn!(session_id = %session_id, error = %error, "failed to inject session-end cleanup");
            return;
        }

        info!(session_id = %session_id, "stage 6 pen stream cleaned up on session end");
    }

    fn record_command_state(&mut self, session_id: &str, command: &PenInjectionCommand) {
        if matches!(command.kind, PenInjectionCommandKind::Cancel)
            || (!command.in_range && !command.is_contact)
        {
            self.state = None;
            return;
        }

        self.state = Some(ActivePenState {
            session_id: session_id.to_string(),
            x: command.x,
            y: command.y,
            in_range: command.in_range,
            is_contact: command.is_contact,
        });
    }
}

struct ShortcutWorker {
    runtime: ShortcutRuntime,
    ended_sessions: HashSet<String>,
}

impl ShortcutWorker {
    fn new(
        executor: Arc<dyn ShortcutExecutor>,
        profile: SharedShortcutProfile,
        overlay: SharedRadialMenuOverlay,
        workspace: WorkspaceService,
    ) -> Self {
        Self {
            runtime: ShortcutRuntime::new(executor, profile, overlay, workspace),
            ended_sessions: HashSet::new(),
        }
    }

    fn run(mut self, receiver: Receiver<ShortcutWorkerEvent>) {
        while let Ok(event) = receiver.recv() {
            match event {
                ShortcutWorkerEvent::PointerContextUpdated {
                    session_id,
                    point,
                    in_range,
                    is_contact,
                } => self.handle_pointer_context(session_id, point, in_range, is_contact),
                ShortcutWorkerEvent::StylusFlags {
                    session_id,
                    seq,
                    flags,
                } => self.handle_stylus_flags(session_id, seq, flags),
                ShortcutWorkerEvent::Gesture { session_id, frame } => {
                    self.handle_gesture(session_id, frame)
                }
                ShortcutWorkerEvent::SessionEnded { session_id } => {
                    self.handle_session_end(session_id)
                }
                ShortcutWorkerEvent::Stop => break,
            }
        }
    }

    fn handle_pointer_context(
        &mut self,
        session_id: String,
        point: ScreenPoint,
        in_range: bool,
        is_contact: bool,
    ) {
        if self.ended_sessions.contains(&session_id) {
            info!(session_id = %session_id, x = point.x, y = point.y, "ignored stale pointer context after session end");
            return;
        }

        self.runtime
            .handle_pointer_context(&session_id, point, in_range, is_contact);
    }

    fn handle_stylus_flags(&mut self, session_id: String, seq: u32, flags: StylusFlags) {
        if self.ended_sessions.contains(&session_id) {
            info!(session_id = %session_id, seq, "ignored stale stylus shortcut pulse after session end");
            return;
        }

        self.runtime.handle_stylus_flags(&session_id, seq, flags);
    }

    fn handle_gesture(&mut self, session_id: String, frame: GestureFrame) {
        if self.ended_sessions.contains(&session_id) {
            info!(session_id = %session_id, seq = frame.seq, "ignored stale gesture frame after session end");
            return;
        }

        self.runtime.handle_gesture_frame(&session_id, &frame);
    }

    fn handle_session_end(&mut self, session_id: String) {
        self.ended_sessions.insert(session_id.clone());
        self.runtime.handle_session_end(&session_id);
    }
}

fn join_worker(worker: &Mutex<Option<JoinHandle<()>>>, name: &'static str) {
    let handle = worker
        .lock()
        .map_err(|_| AppError::StatePoisoned(name))
        .ok()
        .and_then(|mut worker| worker.take());

    if let Some(handle) = handle
        && handle.join().is_err()
    {
        warn!(worker = name, "worker panicked during shutdown");
    }
}

fn has_shortcut_pulse(flags: StylusFlags) -> bool {
    flags.squeeze()
        || flags.double_tap()
        || flags.two_tap()
        || flags.three_tap()
        || flags.four_tap()
}

fn map_stylus_frame(
    monitor: &MonitorInfo,
    frame: &StylusFrame,
    pressure: u16,
) -> PenInjectionCommand {
    let in_range = frame.flags.in_range();
    let is_contact = frame.flags.is_contact();

    PenInjectionCommand {
        x: map_axis(frame.x, monitor.pixel_width, monitor.virtual_left),
        y: map_axis(frame.y, monitor.pixel_height, monitor.virtual_top),
        kind: map_command_kind(frame.event_type),
        in_range,
        is_contact,
        pressure: map_pressure(pressure, is_contact),
        tilt_x: map_tilt(frame.tilt_x),
        tilt_y: map_tilt(frame.tilt_y),
    }
}

fn map_command_kind(event_type: StylusEventType) -> PenInjectionCommandKind {
    match event_type {
        StylusEventType::Down => PenInjectionCommandKind::Down,
        StylusEventType::Move => PenInjectionCommandKind::Update,
        StylusEventType::Up => PenInjectionCommandKind::Up,
        StylusEventType::Cancel => PenInjectionCommandKind::Cancel,
    }
}

fn map_axis(value: u16, pixel_extent: u32, virtual_origin: i32) -> i32 {
    if pixel_extent <= 1 {
        return virtual_origin;
    }

    let span = pixel_extent - 1;
    let scaled = ((u64::from(value) * u64::from(span)) + u64::from(LOGICAL_COORD_MAX / 2))
        / u64::from(LOGICAL_COORD_MAX);

    virtual_origin + i32::try_from(scaled).expect("mapped coordinate should fit in i32")
}

fn map_pressure(value: u16, is_contact: bool) -> u32 {
    let mapped = u32::from(value).min(WINDOWS_PRESSURE_MAX);

    if is_contact { mapped.max(1) } else { mapped }
}

fn default_pressure(value: f32) -> u16 {
    let index = (value.clamp(0.0, 1.0) * (PRESSURE_LUT_SIZE as f32 - 1.0)).round() as usize;
    let normalized = index.min(PRESSURE_LUT_SIZE - 1) as f32 / (PRESSURE_LUT_SIZE as f32 - 1.0);
    (normalized * WINDOWS_PRESSURE_MAX as f32).round() as u16
}

fn map_tilt(value: i8) -> i32 {
    i32::from(value).clamp(WINDOWS_TILT_MIN, WINDOWS_TILT_MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        net::Ipv4Addr,
        sync::{Mutex, mpsc},
        thread,
        time::{Duration, Instant},
    };

    use crate::{
        protocol::{GestureFrame, GestureState, GestureType, StylusEventType, StylusFlags},
        shortcut::{KeyCode, MouseButton, ShortcutCommand, ShortcutExecutor},
        udp_ingest::IncomingEvent,
        workspace::{MonitorId, WorkspaceSnapshot},
    };

    use super::*;

    #[derive(Default)]
    struct RecordingInjector {
        commands: Mutex<Vec<PenInjectionCommand>>,
    }

    #[derive(Default)]
    struct RecordingShortcutExecutor {
        commands: Mutex<Vec<ShortcutCommand>>,
    }

    struct BlockingInjector {
        commands: Mutex<Vec<PenInjectionCommand>>,
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        should_block: Mutex<bool>,
    }

    impl BlockingInjector {
        fn new(entered: mpsc::Sender<()>, release: mpsc::Receiver<()>) -> Self {
            Self {
                commands: Mutex::new(Vec::new()),
                entered,
                release: Mutex::new(release),
                should_block: Mutex::new(true),
            }
        }
    }

    impl PenInjector for RecordingInjector {
        fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
            self.commands
                .lock()
                .expect("injector should lock")
                .push(command);
            Ok(())
        }
    }

    impl PenInjector for BlockingInjector {
        fn inject(&self, command: PenInjectionCommand) -> Result<(), AppError> {
            self.commands
                .lock()
                .expect("injector should lock")
                .push(command);

            let mut should_block = self.should_block.lock().expect("block flag should lock");
            if *should_block {
                *should_block = false;
                self.entered.send(()).expect("entered signal should send");
                self.release
                    .lock()
                    .expect("release receiver should lock")
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|_| AppError::Startup("blocking injector gate timed out"))?;
            }

            Ok(())
        }
    }

    impl ShortcutExecutor for RecordingShortcutExecutor {
        fn execute(&self, command: ShortcutCommand) -> Result<(), AppError> {
            self.commands
                .lock()
                .expect("shortcut executor should lock")
                .push(command);
            Ok(())
        }
    }

    fn test_workspace() -> WorkspaceService {
        WorkspaceService::from_snapshot(WorkspaceSnapshot {
            monitors: vec![MonitorInfo {
                id: MonitorId::new("monitor-1".to_string()),
                device_name: "DISPLAY1".to_string(),
                is_primary: true,
                pixel_width: 1920,
                pixel_height: 1080,
                virtual_left: 100,
                virtual_top: 200,
                virtual_right: 2020,
                virtual_bottom: 1280,
            }],
            active_monitor_id: Some(MonitorId::new("monitor-1".to_string())),
            active_workspace: Some(crate::workspace::ActiveWorkspace {
                monitor: MonitorInfo {
                    id: MonitorId::new("monitor-1".to_string()),
                    device_name: "DISPLAY1".to_string(),
                    is_primary: true,
                    pixel_width: 1920,
                    pixel_height: 1080,
                    virtual_left: 100,
                    virtual_top: 200,
                    virtual_right: 2020,
                    virtual_bottom: 1280,
                },
            }),
        })
    }

    fn stylus_frame(event_type: StylusEventType, flags: u8, x: u16, y: u16) -> StylusFrame {
        StylusFrame {
            seq: 1,
            timestamp: 10,
            x,
            y,
            pressure: 0.5,
            tilt_x: 10,
            tilt_y: -15,
            event_type,
            flags: StylusFlags(flags),
            reserved_ext: 0,
        }
    }

    fn test_pressure_settings() -> SharedPressureSettings {
        Arc::new(std::sync::RwLock::new(
            crate::app::state::PressureSettings::from_curve(crate::config::PressureCurve::default()),
        ))
    }

    fn test_shortcut_profile() -> SharedShortcutProfile {
        crate::shortcut::ShortcutProfile::default().shared()
    }

    fn gesture_frame(
        gesture_type: GestureType,
        state: GestureState,
        seq: u32,
        val1: f32,
    ) -> GestureFrame {
        GestureFrame {
            gesture_type,
            state,
            seq,
            timestamp: 10,
            val1,
            val2: 0.0,
            val3: 0.0,
            val4: 0.0,
        }
    }

    fn gesture_frame_xy(
        gesture_type: GestureType,
        state: GestureState,
        seq: u32,
        val1: f32,
        val2: f32,
    ) -> GestureFrame {
        GestureFrame {
            gesture_type,
            state,
            seq,
            timestamp: 10,
            val1,
            val2,
            val3: 0.0,
            val4: 0.0,
        }
    }

    fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if condition() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(condition(), "condition was not met before timeout");
    }

    #[test]
    fn maps_stylus_coordinates_to_virtual_screen_space() {
        let monitor = test_workspace()
            .current_workspace()
            .expect("workspace should exist")
            .monitor;

        let origin = map_stylus_frame(
            &monitor,
            &stylus_frame(StylusEventType::Move, 0b0000_0001, 0, 0),
            0,
        );
        let corner = map_stylus_frame(
            &monitor,
            &stylus_frame(StylusEventType::Move, 0b0000_0001, 32_767, 32_767),
            0,
        );
        let center = map_stylus_frame(
            &monitor,
            &stylus_frame(StylusEventType::Move, 0b0000_0001, 16_384, 16_384),
            0,
        );

        assert_eq!((origin.x, origin.y), (100, 200));
        assert_eq!((corner.x, corner.y), (2019, 1279));
        assert_eq!((center.x, center.y), (1060, 740));
    }

    #[test]
    fn maps_pressure_and_tilt_into_windows_ranges() {
        let monitor = test_workspace()
            .current_workspace()
            .expect("workspace should exist")
            .monitor;
        let mut frame = stylus_frame(StylusEventType::Move, 0b0000_0011, 100, 200);
        frame.pressure = 1.5;
        frame.tilt_x = 120_i8.saturating_sub(0);
        frame.tilt_y = -120_i8.saturating_add(0);

        let command = map_stylus_frame(&monitor, &frame, 1024);

        assert_eq!(command.pressure, 1024);
        assert_eq!(command.tilt_x, 90);
        assert_eq!(command.tilt_y, -90);
    }

    #[test]
    fn cancel_frame_clears_local_stream_state() {
        let injector = Arc::new(RecordingInjector::default());
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector.clone(),
            shortcut,
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: stylus_frame(StylusEventType::Down, 0b0000_0011, 100, 200),
        });
        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: stylus_frame(StylusEventType::Cancel, 0, 100, 200),
        });
        pipeline.emit(IncomingEvent::SessionEnded {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
        });

        wait_until(Duration::from_secs(1), || {
            injector
                .commands
                .lock()
                .expect("injector should lock")
                .len()
                == 2
        });

        let commands = injector.commands.lock().expect("injector should lock");
        assert_eq!(commands[0].kind, PenInjectionCommandKind::Down);
        assert_eq!(commands[1].kind, PenInjectionCommandKind::Cancel);
    }

    #[test]
    fn session_end_injects_cleanup_for_active_pen_stream() {
        let injector = Arc::new(RecordingInjector::default());
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector.clone(),
            shortcut,
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: stylus_frame(StylusEventType::Down, 0b0000_0011, 100, 200),
        });
        pipeline.emit(IncomingEvent::SessionEnded {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
        });

        wait_until(Duration::from_secs(1), || {
            injector
                .commands
                .lock()
                .expect("injector should lock")
                .len()
                == 2
        });

        let commands = injector.commands.lock().expect("injector should lock");
        assert_eq!(commands[0].kind, PenInjectionCommandKind::Down);
        assert_eq!(commands[1].kind, PenInjectionCommandKind::Cancel);
        assert_eq!(commands[1].is_contact, false);
        assert_eq!(commands[1].in_range, false);
    }

    #[test]
    fn gesture_events_dispatch_shortcut_commands_without_pen_injection() {
        let injector = Arc::new(RecordingInjector::default());
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector.clone(),
            shortcut.clone(),
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame(GestureType::OneSwipe, GestureState::Begin, 1, 0.0),
        });

        wait_until(Duration::from_secs(1), || {
            shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock")
                .len()
                == 1
        });

        assert!(
            injector
                .commands
                .lock()
                .expect("injector should lock")
                .is_empty()
        );
        assert_eq!(
            *shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock"),
            vec![ShortcutCommand::PressChord(vec![KeyCode::E])]
        );
    }

    #[test]
    fn session_end_releases_active_shortcut_holds() {
        let injector = Arc::new(RecordingInjector::default());
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector,
            shortcut.clone(),
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame(GestureType::OneLongPress, GestureState::Begin, 1, 0.0),
        });
        pipeline.emit(IncomingEvent::SessionEnded {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
        });

        wait_until(Duration::from_secs(1), || {
            shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock")
                .len()
                == 2
        });

        assert_eq!(
            *shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock"),
            vec![
                ShortcutCommand::KeyDown(KeyCode::Alt),
                ShortcutCommand::KeyUp(KeyCode::Alt),
            ]
        );
    }

    #[test]
    fn double_tap_uses_hover_then_last_in_range_pointer_context() {
        let injector = Arc::new(RecordingInjector::default());
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector,
            shortcut.clone(),
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        let mut hover_frame = stylus_frame(StylusEventType::Move, 0b0000_0001, 2000, 4000);
        hover_frame.seq = 1;
        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: hover_frame,
        });

        let mut hover_double_tap = stylus_frame(StylusEventType::Move, 0b0000_1001, 2000, 4000);
        hover_double_tap.seq = 2;
        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: hover_double_tap,
        });

        let mut contact_frame = stylus_frame(StylusEventType::Move, 0b0000_0011, 3000, 5000);
        contact_frame.seq = 3;
        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: contact_frame,
        });

        let mut contact_double_tap = stylus_frame(StylusEventType::Move, 0b0000_1011, 3000, 5000);
        contact_double_tap.seq = 4;
        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: contact_double_tap,
        });

        wait_until(Duration::from_secs(1), || {
            shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock")
                .len()
                == 2
        });

        let commands = shortcut
            .commands
            .lock()
            .expect("shortcut executor should lock");
        assert_eq!(
            *commands,
            vec![
                ShortcutCommand::RightClickAt { x: 217, y: 332 },
                ShortcutCommand::RightClickAt { x: 276, y: 365 },
            ]
        );
    }

    #[test]
    fn three_pan_emits_drag_commands_in_pipeline() {
        let injector = Arc::new(RecordingInjector::default());
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector,
            shortcut.clone(),
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame_xy(GestureType::ThreePan, GestureState::Begin, 1, 0.0, 0.0),
        });
        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame_xy(GestureType::ThreePan, GestureState::Update, 2, 10.0, -6.0),
        });
        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame_xy(GestureType::ThreePan, GestureState::End, 3, 0.0, 0.0),
        });

        wait_until(Duration::from_secs(1), || {
            shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock")
                .len()
                == 5
        });

        assert_eq!(
            *shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock"),
            vec![
                ShortcutCommand::KeyDown(KeyCode::Alt),
                ShortcutCommand::MouseButtonDown(MouseButton::Right),
                ShortcutCommand::MouseMoveRelative { dx: 10, dy: -6 },
                ShortcutCommand::MouseButtonUp(MouseButton::Right),
                ShortcutCommand::KeyUp(KeyCode::Alt),
            ]
        );
    }

    #[test]
    fn gesture_shortcuts_execute_while_stylus_worker_is_blocked() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let injector = Arc::new(BlockingInjector::new(entered_tx, release_rx));
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector.clone(),
            shortcut.clone(),
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: stylus_frame(StylusEventType::Down, 0b0000_0011, 100, 200),
        });
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("stylus worker should block in injector");

        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame(GestureType::OneSwipe, GestureState::Begin, 1, 0.0),
        });

        wait_until(Duration::from_secs(1), || {
            shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock")
                .len()
                == 1
        });

        assert_eq!(
            *shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock"),
            vec![ShortcutCommand::PressChord(vec![KeyCode::E])]
        );

        release_tx.send(()).expect("release signal should send");
        wait_until(Duration::from_secs(1), || {
            !injector
                .commands
                .lock()
                .expect("injector should lock")
                .is_empty()
        });
    }

    #[test]
    fn stylus_flag_shortcuts_execute_while_stylus_worker_is_blocked() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let injector = Arc::new(BlockingInjector::new(entered_tx, release_rx));
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector.clone(),
            shortcut.clone(),
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: stylus_frame(StylusEventType::Down, 0b0000_0011, 100, 200),
        });
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("stylus worker should block in injector");

        let mut flagged_frame = stylus_frame(StylusEventType::Move, 0b0001_0001, 120, 220);
        flagged_frame.seq = 2;
        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: flagged_frame,
        });

        wait_until(Duration::from_secs(1), || {
            shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock")
                .len()
                == 1
        });

        assert_eq!(
            *shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock"),
            vec![ShortcutCommand::PressChord(vec![
                KeyCode::Control,
                KeyCode::Z
            ])]
        );

        release_tx.send(()).expect("release signal should send");
        wait_until(Duration::from_secs(1), || {
            !injector
                .commands
                .lock()
                .expect("injector should lock")
                .is_empty()
        });
    }

    #[test]
    fn session_end_fans_out_to_pen_and_shortcut_workers() {
        let injector = Arc::new(RecordingInjector::default());
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector.clone(),
            shortcut.clone(),
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: stylus_frame(StylusEventType::Down, 0b0000_0011, 100, 200),
        });
        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame(GestureType::OneLongPress, GestureState::Begin, 1, 0.0),
        });
        pipeline.emit(IncomingEvent::SessionEnded {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
        });

        wait_until(Duration::from_secs(1), || {
            injector
                .commands
                .lock()
                .expect("injector should lock")
                .len()
                == 2
                && shortcut
                    .commands
                    .lock()
                    .expect("shortcut executor should lock")
                    .len()
                    == 2
        });

        let injector_commands = injector.commands.lock().expect("injector should lock");
        let shortcut_commands = shortcut
            .commands
            .lock()
            .expect("shortcut executor should lock");
        assert_eq!(injector_commands[0].kind, PenInjectionCommandKind::Down);
        assert_eq!(injector_commands[1].kind, PenInjectionCommandKind::Cancel);
        assert_eq!(
            *shortcut_commands,
            vec![
                ShortcutCommand::KeyDown(KeyCode::Alt),
                ShortcutCommand::KeyUp(KeyCode::Alt),
            ]
        );
    }

    #[test]
    fn stale_events_after_session_end_do_not_reactivate_pen_or_shortcuts() {
        let injector = Arc::new(RecordingInjector::default());
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector.clone(),
            shortcut.clone(),
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: stylus_frame(StylusEventType::Down, 0b0000_0011, 100, 200),
        });
        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame(GestureType::OneLongPress, GestureState::Begin, 1, 0.0),
        });
        pipeline.emit(IncomingEvent::SessionEnded {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
        });
        pipeline.emit(IncomingEvent::Stylus {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: stylus_frame(StylusEventType::Down, 0b0000_0011, 150, 250),
        });
        pipeline.emit(IncomingEvent::Gesture {
            session_id: "session-a".to_string(),
            source_ip: Ipv4Addr::new(127, 0, 0, 1),
            frame: gesture_frame(GestureType::OneLongPress, GestureState::Begin, 2, 0.0),
        });

        wait_until(Duration::from_secs(1), || {
            injector
                .commands
                .lock()
                .expect("injector should lock")
                .len()
                == 2
                && shortcut
                    .commands
                    .lock()
                    .expect("shortcut executor should lock")
                    .len()
                    == 2
        });
        thread::sleep(Duration::from_millis(50));

        assert_eq!(
            *injector.commands.lock().expect("injector should lock"),
            vec![
                PenInjectionCommand {
                    x: 106,
                    y: 207,
                    kind: PenInjectionCommandKind::Down,
                    in_range: true,
                    is_contact: true,
                    pressure: 512,
                    tilt_x: 10,
                    tilt_y: -15,
                },
                PenInjectionCommand {
                    x: 106,
                    y: 207,
                    kind: PenInjectionCommandKind::Cancel,
                    in_range: false,
                    is_contact: false,
                    pressure: 0,
                    tilt_x: 0,
                    tilt_y: 0,
                },
            ]
        );
        assert_eq!(
            *shortcut
                .commands
                .lock()
                .expect("shortcut executor should lock"),
            vec![
                ShortcutCommand::KeyDown(KeyCode::Alt),
                ShortcutCommand::KeyUp(KeyCode::Alt),
            ]
        );
    }
}
