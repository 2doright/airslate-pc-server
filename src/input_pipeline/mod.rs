use std::{
    collections::{HashSet, VecDeque},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::app::state::{PRESSURE_LUT_SIZE, SharedInputProcessingSettings, SharedPressureSettings};
use crate::config::HoverMovePolicy;

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
const PRECISE_ANCHOR_HOVER_HISTORY_MS: u64 = 300;
const PRECISE_ANCHOR_ENDPOINT_WINDOW_MS: u64 = 60;
const PRECISE_ANCHOR_STABILITY_WINDOW_MS: u64 = 120;
const PRECISE_ANCHOR_STABLE_DURATION_MS: u64 = 220;
const PRECISE_ANCHOR_WRITING_STABLE_DURATION_MS: u64 = 280;
const PRECISE_ANCHOR_TOLERANCE_DURATION_MS: u64 = 30;
const PRECISE_ANCHOR_ENDPOINT_RADIUS: i32 = 48;
// 96 logical units are about 0.29% of either normalized tablet axis.
const PRECISE_ANCHOR_DISPERSION_RADIUS: i32 = 96;
const PRECISE_ANCHOR_EXIT_RADIUS: i32 = 160;
const PRECISE_ANCHOR_WRITING_FLOW_MS: u64 = 700;
const PRECISE_ANCHOR_WRITING_STROKE_DISTANCE: i32 = 32;
const PRECISE_ANCHOR_WRITING_STROKE_COUNT: usize = 2;

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
    pub tablet_x: u16,
    pub tablet_y: u16,
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
    tablet_x: u16,
    tablet_y: u16,
    in_range: bool,
    is_contact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PenCoordinate {
    x: i32,
    y: i32,
    tablet_x: u16,
    tablet_y: u16,
}

impl From<&PenInjectionCommand> for PenCoordinate {
    fn from(command: &PenInjectionCommand) -> Self {
        Self {
            x: command.x,
            y: command.y,
            tablet_x: command.tablet_x,
            tablet_y: command.tablet_y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HoverObservation {
    timestamp_ms: u64,
    coordinate: PenCoordinate,
}

#[derive(Debug, PartialEq, Eq)]
struct HoverAnchorCandidate {
    observations: VecDeque<HoverObservation>,
    last: PenCoordinate,
    last_timestamp_ms: u64,
    stable_duration_ms: u64,
    tolerance_since_ms: Option<u64>,
    is_core_stable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoverStability {
    Core,
    Tolerance,
    Exit,
}

impl HoverAnchorCandidate {
    fn new(timestamp_ms: u64, coordinate: PenCoordinate) -> Self {
        Self {
            observations: VecDeque::from([HoverObservation {
                timestamp_ms,
                coordinate,
            }]),
            last: coordinate,
            last_timestamp_ms: timestamp_ms,
            stable_duration_ms: 0,
            tolerance_since_ms: None,
            is_core_stable: true,
        }
    }

    fn stable_duration_at(&self, timestamp_ms: u64) -> Option<u64> {
        let quiet_duration_ms = timestamp_ms.checked_sub(self.last_timestamp_ms)?;
        Some(if self.is_core_stable {
            self.stable_duration_ms.saturating_add(quiet_duration_ms)
        } else if quiet_duration_ms >= PRECISE_ANCHOR_TOLERANCE_DURATION_MS {
            quiet_duration_ms
        } else {
            self.stable_duration_ms
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContactStroke {
    origin: PenCoordinate,
    max_squared_displacement: i64,
}

impl ContactStroke {
    fn new(origin: PenCoordinate) -> Self {
        Self {
            origin,
            max_squared_displacement: 0,
        }
    }

    fn observe(&mut self, coordinate: PenCoordinate) {
        self.max_squared_displacement = self
            .max_squared_displacement
            .max(squared_tablet_distance(self.origin, coordinate));
    }

    fn is_writing_stroke(self) -> bool {
        self.max_squared_displacement >= i64::from(PRECISE_ANCHOR_WRITING_STROKE_DISTANCE).pow(2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StrokeCoordinateOffset {
    x: i32,
    y: i32,
    tablet_x: i32,
    tablet_y: i32,
}

#[derive(Default)]
struct PreciseAnchorCorrector {
    hover_candidate: Option<HoverAnchorCandidate>,
    stroke_offset: Option<StrokeCoordinateOffset>,
    contact_stroke: Option<ContactStroke>,
    writing_stroke_ends_ms: VecDeque<u64>,
}

impl PreciseAnchorCorrector {
    fn process(
        &mut self,
        frame: &StylusFrame,
        mut command: PenInjectionCommand,
        enabled: bool,
    ) -> PenInjectionCommand {
        if !enabled {
            self.reset();
            return command;
        }

        if frame.event_type == StylusEventType::Move && command.in_range && !command.is_contact {
            self.stroke_offset = None;
            self.observe_hover(frame.timestamp, &command);
            return command;
        }

        if frame.event_type == StylusEventType::Down && command.is_contact {
            self.stroke_offset = self.offset_for_down(frame.timestamp, &command);
            self.hover_candidate = None;
            self.contact_stroke = Some(ContactStroke::new(PenCoordinate::from(&command)));
        } else if frame.event_type == StylusEventType::Move && command.is_contact {
            if let Some(stroke) = &mut self.contact_stroke {
                stroke.observe(PenCoordinate::from(&command));
            }
        } else if frame.event_type == StylusEventType::Up {
            self.finish_contact_stroke(frame.timestamp);
        } else if frame.event_type == StylusEventType::Cancel {
            self.contact_stroke = None;
        }

        if let Some(offset) = self.stroke_offset
            && (command.is_contact
                || matches!(
                    frame.event_type,
                    StylusEventType::Down | StylusEventType::Up | StylusEventType::Cancel
                ))
        {
            apply_coordinate_offset(&mut command, offset);
        }

        if frame.event_type == StylusEventType::Cancel || (!command.in_range && !command.is_contact)
        {
            self.reset();
        } else if frame.event_type == StylusEventType::Up {
            self.stroke_offset = None;
        }

        command
    }

    fn observe_hover(&mut self, timestamp_ms: u64, command: &PenInjectionCommand) {
        let current = PenCoordinate::from(command);
        let Some(candidate) = &mut self.hover_candidate else {
            self.hover_candidate = Some(HoverAnchorCandidate::new(timestamp_ms, current));
            return;
        };

        let Some(elapsed_ms) = timestamp_ms.checked_sub(candidate.last_timestamp_ms) else {
            *candidate = HoverAnchorCandidate::new(timestamp_ms, current);
            return;
        };
        candidate.observations.push_back(HoverObservation {
            timestamp_ms,
            coordinate: current,
        });
        let history_start_ms = timestamp_ms.saturating_sub(PRECISE_ANCHOR_HOVER_HISTORY_MS);
        while candidate
            .observations
            .front()
            .is_some_and(|observation| observation.timestamp_ms < history_start_ms)
        {
            candidate.observations.pop_front();
        }

        match hover_stability(
            &candidate.observations,
            candidate.last,
            current,
            timestamp_ms,
        ) {
            HoverStability::Core => {
                if candidate.is_core_stable {
                    candidate.stable_duration_ms =
                        candidate.stable_duration_ms.saturating_add(elapsed_ms);
                }
                candidate.tolerance_since_ms = None;
                candidate.is_core_stable = true;
            }
            HoverStability::Tolerance => {
                let tolerance_since_ms = *candidate.tolerance_since_ms.get_or_insert(timestamp_ms);
                candidate.is_core_stable = false;
                if timestamp_ms.saturating_sub(tolerance_since_ms)
                    >= PRECISE_ANCHOR_TOLERANCE_DURATION_MS
                {
                    *candidate = HoverAnchorCandidate::new(timestamp_ms, current);
                    return;
                }
            }
            HoverStability::Exit => {
                *candidate = HoverAnchorCandidate::new(timestamp_ms, current);
                return;
            }
        }

        candidate.last = current;
        candidate.last_timestamp_ms = timestamp_ms;
    }

    fn offset_for_down(
        &mut self,
        timestamp_ms: u64,
        down: &PenInjectionCommand,
    ) -> Option<StrokeCoordinateOffset> {
        self.prune_writing_strokes(timestamp_ms);
        let candidate = self.hover_candidate.as_ref()?;
        let required_duration_ms =
            if self.writing_stroke_ends_ms.len() >= PRECISE_ANCHOR_WRITING_STROKE_COUNT {
                PRECISE_ANCHOR_WRITING_STABLE_DURATION_MS
            } else {
                PRECISE_ANCHOR_STABLE_DURATION_MS
            };
        if candidate.stable_duration_at(timestamp_ms)? < required_duration_ms {
            return None;
        }

        Some(StrokeCoordinateOffset {
            x: down.x - candidate.last.x,
            y: down.y - candidate.last.y,
            tablet_x: i32::from(down.tablet_x) - i32::from(candidate.last.tablet_x),
            tablet_y: i32::from(down.tablet_y) - i32::from(candidate.last.tablet_y),
        })
    }

    fn finish_contact_stroke(&mut self, timestamp_ms: u64) {
        let Some(stroke) = self.contact_stroke.take() else {
            return;
        };
        if stroke.is_writing_stroke() {
            self.prune_writing_strokes(timestamp_ms);
            self.writing_stroke_ends_ms.push_back(timestamp_ms);
        }
    }

    fn prune_writing_strokes(&mut self, timestamp_ms: u64) {
        let history_start_ms = timestamp_ms.saturating_sub(PRECISE_ANCHOR_WRITING_FLOW_MS);
        while self
            .writing_stroke_ends_ms
            .front()
            .is_some_and(|stroke_end_ms| *stroke_end_ms < history_start_ms)
        {
            self.writing_stroke_ends_ms.pop_front();
        }
    }

    fn reset(&mut self) {
        self.hover_candidate = None;
        self.stroke_offset = None;
        self.contact_stroke = None;
        self.writing_stroke_ends_ms.clear();
    }

    fn clear_stroke(&mut self) {
        self.stroke_offset = None;
        self.contact_stroke = None;
    }
}

fn hover_stability(
    observations: &VecDeque<HoverObservation>,
    previous: PenCoordinate,
    current: PenCoordinate,
    timestamp_ms: u64,
) -> HoverStability {
    let endpoint_start_ms = timestamp_ms.saturating_sub(PRECISE_ANCHOR_ENDPOINT_WINDOW_MS);
    let endpoint_origin = observations
        .iter()
        .rev()
        .find(|observation| observation.timestamp_ms <= endpoint_start_ms)
        .or_else(|| observations.front())
        .map_or(current, |observation| observation.coordinate);
    let endpoint_displacement = squared_tablet_distance(endpoint_origin, current);

    let stability_start_ms = timestamp_ms.saturating_sub(PRECISE_ANCHOR_STABILITY_WINDOW_MS);
    let (observation_count, coordinate_sum_x, coordinate_sum_y) = observations
        .iter()
        .filter(|observation| observation.timestamp_ms >= stability_start_ms)
        .fold(
            (0_i64, 0_i64, 0_i64),
            |(count, sum_x, sum_y), observation| {
                (
                    count + 1,
                    sum_x + i64::from(observation.coordinate.tablet_x),
                    sum_y + i64::from(observation.coordinate.tablet_y),
                )
            },
        );
    let center_x = coordinate_sum_x / observation_count;
    let center_y = coordinate_sum_y / observation_count;
    let dispersion_radius = observations
        .iter()
        .filter(|observation| observation.timestamp_ms >= stability_start_ms)
        .map(|observation| {
            let delta_x = i64::from(observation.coordinate.tablet_x) - center_x;
            let delta_y = i64::from(observation.coordinate.tablet_y) - center_y;
            delta_x * delta_x + delta_y * delta_y
        })
        .max()
        .unwrap_or_default();

    let exit_radius_squared = i64::from(PRECISE_ANCHOR_EXIT_RADIUS).pow(2);
    if squared_tablet_distance(previous, current) > exit_radius_squared
        || endpoint_displacement > exit_radius_squared
        || dispersion_radius > exit_radius_squared
    {
        return HoverStability::Exit;
    }

    if endpoint_displacement <= i64::from(PRECISE_ANCHOR_ENDPOINT_RADIUS).pow(2)
        && dispersion_radius <= i64::from(PRECISE_ANCHOR_DISPERSION_RADIUS).pow(2)
    {
        HoverStability::Core
    } else {
        HoverStability::Tolerance
    }
}

fn squared_tablet_distance(left: PenCoordinate, right: PenCoordinate) -> i64 {
    let delta_x = i64::from(left.tablet_x) - i64::from(right.tablet_x);
    let delta_y = i64::from(left.tablet_y) - i64::from(right.tablet_y);
    delta_x * delta_x + delta_y * delta_y
}

fn apply_coordinate_offset(command: &mut PenInjectionCommand, offset: StrokeCoordinateOffset) {
    command.x -= offset.x;
    command.y -= offset.y;
    command.tablet_x = (i32::from(command.tablet_x) - offset.tablet_x)
        .clamp(0, i32::from(LOGICAL_COORD_MAX)) as u16;
    command.tablet_y = (i32::from(command.tablet_y) - offset.tablet_y)
        .clamp(0, i32::from(LOGICAL_COORD_MAX)) as u16;
}

enum StylusWorkerEvent {
    Stylus {
        session_id: String,
        sample: StylusSample,
    },
    SessionEnded {
        session_id: String,
    },
    Stop,
}

struct StylusSample {
    frame: StylusFrame,
    mapped_command: Option<PenInjectionCommand>,
    queued_at: Option<Instant>,
    accepted_at: Instant,
    preempt_previous: bool,
}

struct InputPipelineMetrics {
    enabled: bool,
    accepted_samples: AtomicU64,
    sequence_gaps: AtomicU64,
    sequence_reorders: AtomicU64,
    last_sequence: AtomicU64,
    injected_samples: AtomicU64,
    injection_errors: AtomicU64,
    max_queue_depth: AtomicUsize,
    queue_wait_samples: AtomicU64,
    queue_wait_total_us: AtomicU64,
    queue_wait_max_us: AtomicU64,
    injection_attempts: AtomicU64,
    injection_total_us: AtomicU64,
    injection_max_us: AtomicU64,
}

impl InputPipelineMetrics {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("AIRSLATE_INPUT_METRICS").is_some(),
            accepted_samples: AtomicU64::new(0),
            sequence_gaps: AtomicU64::new(0),
            sequence_reorders: AtomicU64::new(0),
            last_sequence: AtomicU64::new(0),
            injected_samples: AtomicU64::new(0),
            injection_errors: AtomicU64::new(0),
            max_queue_depth: AtomicUsize::new(0),
            queue_wait_samples: AtomicU64::new(0),
            queue_wait_total_us: AtomicU64::new(0),
            queue_wait_max_us: AtomicU64::new(0),
            injection_attempts: AtomicU64::new(0),
            injection_total_us: AtomicU64::new(0),
            injection_max_us: AtomicU64::new(0),
        }
    }

    fn queued_at(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    fn record_accepted(&self) {
        if self.enabled {
            self.accepted_samples.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_sequence(&self, sequence: u32) {
        if !self.enabled {
            return;
        }

        let previous = self
            .last_sequence
            .swap(u64::from(sequence) + 1, Ordering::Relaxed);
        if previous == 0 {
            return;
        }

        let previous = (previous - 1) as u32;
        let delta = sequence.wrapping_sub(previous);
        if delta == 1 {
            return;
        }

        if delta > 1 && delta < 0x8000_0000 {
            self.sequence_gaps
                .fetch_add(u64::from(delta - 1), Ordering::Relaxed);
        } else {
            self.sequence_reorders.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn reset_sequence(&self) {
        if self.enabled {
            self.last_sequence.store(0, Ordering::Relaxed);
        }
    }

    fn record_queue_depth(&self, depth: usize) {
        if self.enabled {
            update_max_usize(&self.max_queue_depth, depth);
        }
    }

    fn record_queue_wait(&self, queued_at: Option<Instant>) {
        let Some(queued_at) = queued_at else {
            return;
        };
        let elapsed_us = duration_to_micros(queued_at.elapsed());
        self.queue_wait_samples.fetch_add(1, Ordering::Relaxed);
        self.queue_wait_total_us
            .fetch_add(elapsed_us, Ordering::Relaxed);
        update_max_u64(&self.queue_wait_max_us, elapsed_us);
    }

    fn injection_started(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    fn record_injection(&self, started_at: Option<Instant>, succeeded: bool) {
        let Some(started_at) = started_at else {
            return;
        };
        let elapsed_us = duration_to_micros(started_at.elapsed());
        self.injection_attempts.fetch_add(1, Ordering::Relaxed);
        self.injection_total_us
            .fetch_add(elapsed_us, Ordering::Relaxed);
        update_max_u64(&self.injection_max_us, elapsed_us);
        if succeeded {
            self.injected_samples.fetch_add(1, Ordering::Relaxed);
        } else {
            self.injection_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn report(&self, reason: &'static str) {
        if !self.enabled {
            return;
        }

        let accepted = self.accepted_samples.load(Ordering::Relaxed);
        let injected = self.injected_samples.load(Ordering::Relaxed);
        let queue_wait_samples = self.queue_wait_samples.load(Ordering::Relaxed);
        let queue_wait_total_us = self.queue_wait_total_us.load(Ordering::Relaxed);
        let injection_attempts = self.injection_attempts.load(Ordering::Relaxed);
        let injection_total_us = self.injection_total_us.load(Ordering::Relaxed);
        info!(
            reason,
            accepted_samples = accepted,
            sequence_gaps = self.sequence_gaps.load(Ordering::Relaxed),
            sequence_reorders = self.sequence_reorders.load(Ordering::Relaxed),
            injected_samples = injected,
            injection_errors = self.injection_errors.load(Ordering::Relaxed),
            max_queue_depth = self.max_queue_depth.load(Ordering::Relaxed),
            queue_wait_avg_us = average_micros(queue_wait_total_us, queue_wait_samples),
            queue_wait_max_us = self.queue_wait_max_us.load(Ordering::Relaxed),
            injection_avg_us = average_micros(injection_total_us, injection_attempts),
            injection_max_us = self.injection_max_us.load(Ordering::Relaxed),
            "stylus input pipeline metrics"
        );
    }
}

fn update_max_u64(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(next) => current = next,
        }
    }
}

fn update_max_usize(target: &AtomicUsize, value: usize) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(next) => current = next,
        }
    }
}

fn duration_to_micros(duration: Duration) -> u64 {
    duration.as_micros() as u64
}

fn average_micros(total: u64, count: u64) -> u64 {
    total / count.max(1)
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

struct RealtimeQueue<T> {
    state: Mutex<RealtimeQueueState<T>>,
    ready: Condvar,
}

struct RealtimeQueueState<T> {
    events: VecDeque<T>,
    closed: bool,
}

impl<T> RealtimeQueue<T> {
    fn new() -> Self {
        Self {
            state: Mutex::new(RealtimeQueueState {
                events: VecDeque::new(),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    fn push(&self, event: T) -> Result<usize, ()> {
        self.push_with(event, |_, _| false)
    }

    fn push_with(&self, event: T, replace_tail: impl FnOnce(&T, &T) -> bool) -> Result<usize, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.closed {
            return Err(());
        }

        if state
            .events
            .back()
            .is_some_and(|queued| replace_tail(queued, &event))
        {
            *state.events.back_mut().expect("queue tail exists") = event;
        } else {
            state.events.push_back(event);
        }
        self.ready.notify_one();
        Ok(state.events.len())
    }

    fn push_mutating(
        &self,
        event: T,
        mutate: impl FnOnce(&mut VecDeque<T>, &T),
    ) -> Result<usize, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.closed {
            return Err(());
        }
        mutate(&mut state.events, &event);
        state.events.push_back(event);
        self.ready.notify_one();
        Ok(state.events.len())
    }

    fn recv(&self) -> Option<T> {
        let mut state = self.state.lock().ok()?;
        loop {
            if let Some(event) = state.events.pop_front() {
                return Some(event);
            }
            if state.closed {
                return None;
            }
            state = self.ready.wait(state).ok()?;
        }
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            self.ready.notify_all();
        }
    }
}

pub struct StylusInputPipeline {
    stylus_queue: Arc<RealtimeQueue<StylusWorkerEvent>>,
    shortcut_queue: Arc<RealtimeQueue<ShortcutWorkerEvent>>,
    metrics: Arc<InputPipelineMetrics>,
    stylus_worker: Mutex<Option<JoinHandle<()>>>,
    shortcut_worker: Mutex<Option<JoinHandle<()>>>,
    workspace: WorkspaceService,
    pressure_settings: SharedPressureSettings,
    input_processing_settings: SharedInputProcessingSettings,
}

impl StylusInputPipeline {
    #[cfg(test)]
    pub fn new(
        workspace: WorkspaceService,
        injector: Arc<dyn PenInjector>,
        shortcut_executor: Arc<dyn ShortcutExecutor>,
        pressure_settings: SharedPressureSettings,
        shortcut_profile: SharedShortcutProfile,
        radial_overlay: SharedRadialMenuOverlay,
    ) -> Self {
        Self::new_with_settings(
            workspace,
            injector,
            shortcut_executor,
            pressure_settings,
            shortcut_profile,
            radial_overlay,
            Arc::default(),
        )
    }

    pub fn new_with_settings(
        workspace: WorkspaceService,
        injector: Arc<dyn PenInjector>,
        shortcut_executor: Arc<dyn ShortcutExecutor>,
        pressure_settings: SharedPressureSettings,
        shortcut_profile: SharedShortcutProfile,
        radial_overlay: SharedRadialMenuOverlay,
        input_processing_settings: SharedInputProcessingSettings,
    ) -> Self {
        let stylus_queue = Arc::new(RealtimeQueue::new());
        let shortcut_queue = Arc::new(RealtimeQueue::new());
        let metrics = Arc::new(InputPipelineMetrics::new());

        let stylus_workspace = workspace.clone();
        let stylus_pressure_settings = pressure_settings.clone();
        let stylus_worker_queue = stylus_queue.clone();
        let stylus_worker_metrics = metrics.clone();
        let stylus_input_processing_settings = input_processing_settings.clone();
        let stylus_worker = thread::spawn(move || {
            StylusWorker::new(
                stylus_workspace,
                injector,
                stylus_pressure_settings,
                stylus_worker_metrics,
                stylus_input_processing_settings,
            )
            .run(stylus_worker_queue);
        });
        let shortcut_workspace = workspace.clone();
        let shortcut_worker_queue = shortcut_queue.clone();
        let shortcut_worker = thread::spawn(move || {
            ShortcutWorker::new(
                shortcut_executor,
                shortcut_profile,
                radial_overlay,
                shortcut_workspace,
            )
            .run(shortcut_worker_queue);
        });

        Self {
            stylus_queue,
            shortcut_queue,
            metrics,
            stylus_worker: Mutex::new(Some(stylus_worker)),
            shortcut_worker: Mutex::new(Some(shortcut_worker)),
            workspace,
            pressure_settings,
            input_processing_settings,
        }
    }

    fn send_stylus(&self, event: StylusWorkerEvent) {
        let latest_only = self
            .input_processing_settings
            .latest_contact_move_only
            .load(Ordering::Acquire);
        let preempt = self
            .input_processing_settings
            .preempt_previous_stroke
            .load(Ordering::Acquire);
        let tolerance_ms = self
            .input_processing_settings
            .latest_contact_move_tolerance_ms
            .load(Ordering::Acquire);
        let hover_policy = HoverMovePolicy::try_from(
            self.input_processing_settings
                .hover_move_policy
                .load(Ordering::Acquire),
        )
        .expect("runtime hover move policy should be a valid level");
        let result = if latest_only || preempt || hover_policy != HoverMovePolicy::PreserveAll {
            self.stylus_queue.push_mutating(event, |events, incoming| {
                compact_stylus_queue(
                    events,
                    incoming,
                    latest_only,
                    tolerance_ms,
                    hover_policy,
                    preempt,
                )
            })
        } else {
            self.stylus_queue.push(event)
        };
        match result {
            Ok(depth) => self.metrics.record_queue_depth(depth),
            Err(()) => warn!("stylus worker queue is unavailable"),
        }
    }

    fn send_shortcut(&self, event: ShortcutWorkerEvent) {
        let result = self.shortcut_queue.push_with(event, can_coalesce_shortcut);
        if result.is_err() {
            warn!("shortcut worker queue is unavailable");
        }
    }

    fn current_monitor(&self) -> Result<MonitorInfo, AppError> {
        self.workspace
            .current_workspace()
            .map(|workspace| workspace.monitor)
    }
}

fn compact_stylus_queue(
    events: &mut VecDeque<StylusWorkerEvent>,
    incoming: &StylusWorkerEvent,
    latest_only: bool,
    tolerance_ms: u64,
    hover_policy: HoverMovePolicy,
    preempt: bool,
) {
    let StylusWorkerEvent::Stylus { session_id, sample } = incoming else {
        return;
    };
    if preempt && sample.frame.event_type == StylusEventType::Down {
        events.retain(|event| !matches!(event, StylusWorkerEvent::Stylus { session_id: queued, .. } if queued == session_id));
        return;
    }
    if latest_only
        && sample.frame.event_type == StylusEventType::Move
        && sample.frame.flags.is_contact()
    {
        let stroke_start = events.iter().rposition(|event| matches!(event,
            StylusWorkerEvent::Stylus { session_id: queued, sample: queued_sample }
            if queued == session_id && matches!(queued_sample.frame.event_type, StylusEventType::Down | StylusEventType::Up | StylusEventType::Cancel)
        )).map_or(0, |boundary| boundary + 1);
        for index in (stroke_start..events.len()).rev() {
            if matches!(&events[index],
                StylusWorkerEvent::Stylus { session_id: queued, sample: queued_sample }
                if queued == session_id && queued_sample.frame.event_type == StylusEventType::Move && queued_sample.frame.flags.is_contact()
                    && (tolerance_ms == 0 || sample.accepted_at.saturating_duration_since(queued_sample.accepted_at) > Duration::from_millis(tolerance_ms))
            ) {
                events.remove(index);
            }
        }
    }
    if sample.frame.event_type != StylusEventType::Move
        || sample.frame.flags.is_contact()
        || hover_policy == HoverMovePolicy::PreserveAll
    {
        return;
    }

    for index in (0..events.len()).rev() {
        let is_consecutive_hover_move = matches!(
            &events[index],
            StylusWorkerEvent::Stylus {
                session_id: queued_session,
                sample: queued_sample,
            } if queued_session == session_id
                && queued_sample.frame.event_type == StylusEventType::Move
                && !queued_sample.frame.flags.is_contact()
        );
        if !is_consecutive_hover_move {
            break;
        }

        let should_remove = matches!(hover_policy, HoverMovePolicy::Latest)
            || hover_policy.interval_ms().is_some_and(|interval_ms| {
                let StylusWorkerEvent::Stylus {
                    sample: queued_sample,
                    ..
                } = &events[index]
                else {
                    unreachable!("matching hover event should be stylus data");
                };
                sample
                    .accepted_at
                    .saturating_duration_since(queued_sample.accepted_at)
                    < Duration::from_millis(interval_ms)
            });
        if should_remove {
            events.remove(index);
        }
    }
}

fn can_coalesce_shortcut(queued: &ShortcutWorkerEvent, incoming: &ShortcutWorkerEvent) -> bool {
    matches!(
        (queued, incoming),
        (
            ShortcutWorkerEvent::PointerContextUpdated {
                session_id: queued_session,
                ..
            },
            ShortcutWorkerEvent::PointerContextUpdated {
                session_id: incoming_session,
                ..
            },
        ) if queued_session == incoming_session
    )
}

impl IncomingEventSink for StylusInputPipeline {
    fn emit(&self, event: IncomingEvent) {
        match event {
            IncomingEvent::Stylus {
                session_id, frame, ..
            }
            | IncomingEvent::UsbStylus { session_id, frame } => {
                self.metrics.record_sequence(frame.seq);
                self.metrics.record_accepted();
                let shortcut_session_id = session_id.clone();
                let seq = frame.seq;
                let flags = frame.flags;
                let has_pulse = has_shortcut_pulse(flags);
                let queued_at = self.metrics.queued_at();
                let accepted_at = Instant::now();
                let preempt_previous = self
                    .input_processing_settings
                    .preempt_previous_stroke
                    .load(Ordering::Acquire)
                    && frame.event_type == StylusEventType::Down;

                if let Ok(monitor) = self.current_monitor() {
                    let pressure = self
                        .pressure_settings
                        .read()
                        .map(|settings| settings.map_pressure(frame.pressure))
                        .unwrap_or_else(|_| default_pressure(frame.pressure));
                    let pointer = map_stylus_frame(&monitor, &frame, pressure);
                    if !pointer.is_contact || has_pulse {
                        self.send_shortcut(ShortcutWorkerEvent::PointerContextUpdated {
                            session_id: shortcut_session_id.clone(),
                            point: ScreenPoint {
                                x: pointer.x,
                                y: pointer.y,
                            },
                            in_range: pointer.in_range,
                            is_contact: pointer.is_contact,
                        });
                    }

                    self.send_stylus(StylusWorkerEvent::Stylus {
                        session_id,
                        sample: StylusSample {
                            frame,
                            mapped_command: Some(pointer),
                            queued_at,
                            accepted_at,
                            preempt_previous,
                        },
                    });
                } else {
                    warn!(session_id = %shortcut_session_id, "failed to resolve active workspace for shortcut pointer context");

                    self.send_stylus(StylusWorkerEvent::Stylus {
                        session_id,
                        sample: StylusSample {
                            frame,
                            mapped_command: None,
                            queued_at,
                            accepted_at,
                            preempt_previous,
                        },
                    });
                }

                if has_pulse {
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
                self.metrics.record_sequence(frame.seq);
                info!(
                    session_id = %session_id,
                    source_ip = %source_ip,
                    seq = frame.seq,
                    "stage 7 gesture frame received"
                );
                self.send_shortcut(ShortcutWorkerEvent::Gesture { session_id, frame });
            }
            IncomingEvent::UsbGesture { session_id, frame } => {
                self.metrics.record_sequence(frame.seq);
                info!(session_id = %session_id, seq = frame.seq, "stage 7 USB gesture frame received");
                self.send_shortcut(ShortcutWorkerEvent::Gesture { session_id, frame });
            }
            IncomingEvent::SessionEnded { session_id, .. }
            | IncomingEvent::UsbSessionEnded { session_id } => {
                self.metrics.reset_sequence();
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
        let _ = self.stylus_queue.push(StylusWorkerEvent::Stop);
        let _ = self.shortcut_queue.push(ShortcutWorkerEvent::Stop);
        self.stylus_queue.close();
        self.shortcut_queue.close();

        join_worker(&self.stylus_worker, "stylus queue worker");
        join_worker(&self.shortcut_worker, "shortcut queue worker");
        self.metrics.report("pipeline_drop");
    }
}

struct StylusWorker {
    workspace: WorkspaceService,
    injector: Arc<dyn PenInjector>,
    pressure_settings: SharedPressureSettings,
    metrics: Arc<InputPipelineMetrics>,
    input_processing_settings: SharedInputProcessingSettings,
    state: Option<ActivePenState>,
    ended_sessions: HashSet<String>,
    precise_anchor_corrector: PreciseAnchorCorrector,
}

impl StylusWorker {
    fn new(
        workspace: WorkspaceService,
        injector: Arc<dyn PenInjector>,
        pressure_settings: SharedPressureSettings,
        metrics: Arc<InputPipelineMetrics>,
        input_processing_settings: SharedInputProcessingSettings,
    ) -> Self {
        Self {
            workspace,
            injector,
            pressure_settings,
            metrics,
            input_processing_settings,
            state: None,
            ended_sessions: HashSet::new(),
            precise_anchor_corrector: PreciseAnchorCorrector::default(),
        }
    }

    fn run(mut self, queue: Arc<RealtimeQueue<StylusWorkerEvent>>) {
        while let Some(event) = queue.recv() {
            match event {
                StylusWorkerEvent::Stylus { session_id, sample } => {
                    self.handle_stylus(session_id, sample)
                }
                StylusWorkerEvent::SessionEnded { session_id } => {
                    self.handle_session_end(session_id);
                    self.metrics.report("session_end");
                }
                StylusWorkerEvent::Stop => break,
            }
        }
    }

    fn handle_stylus(&mut self, session_id: String, sample: StylusSample) {
        self.metrics.record_queue_wait(sample.queued_at);

        if self.ended_sessions.contains(&session_id) {
            info!(
                session_id = %session_id,
                seq = sample.frame.seq,
                "ignored stale stylus frame after session end"
            );
            return;
        }

        if sample.preempt_previous {
            self.cancel_active_contact(&session_id);
        }

        let command = match sample.mapped_command {
            Some(command) => command,
            None => {
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
                    .map(|settings| settings.map_pressure(sample.frame.pressure))
                    .unwrap_or_else(|_| default_pressure(sample.frame.pressure));
                map_stylus_frame(&monitor, &sample.frame, pressure)
            }
        };
        if self
            .state
            .as_ref()
            .is_some_and(|active| active.session_id != session_id)
        {
            self.precise_anchor_corrector.reset();
        }
        let correction_enabled = self
            .input_processing_settings
            .precise_anchor_correction_enabled
            .load(Ordering::Acquire);
        let command =
            self.precise_anchor_corrector
                .process(&sample.frame, command, correction_enabled);
        self.record_command_state(&session_id, &command);

        let injection_started = self.metrics.injection_started();
        let result = self.injector.inject(command);
        self.metrics
            .record_injection(injection_started, result.is_ok());

        if let Err(error) = result {
            warn!(
                session_id = %session_id,
                seq = sample.frame.seq,
                error = %error,
                "failed to inject stylus frame"
            );
        }
    }

    fn cancel_active_contact(&mut self, incoming_session_id: &str) {
        let Some(active) = self.state.take() else {
            return;
        };
        if !active.is_contact {
            return;
        }
        self.precise_anchor_corrector.clear_stroke();
        let cancel = PenInjectionCommand {
            x: active.x,
            y: active.y,
            tablet_x: active.tablet_x,
            tablet_y: active.tablet_y,
            kind: PenInjectionCommandKind::Cancel,
            in_range: false,
            is_contact: false,
            pressure: 0,
            tilt_x: 0,
            tilt_y: 0,
        };
        let started = self.metrics.injection_started();
        let result = self.injector.inject(cancel);
        self.metrics.record_injection(started, result.is_ok());
        if let Err(error) = result {
            warn!(session_id = %incoming_session_id, error = %error, "failed to cancel preempted stylus contact");
        }
    }

    fn handle_session_end(&mut self, session_id: String) {
        self.ended_sessions.insert(session_id.clone());
        self.precise_anchor_corrector.reset();

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
            tablet_x: active.tablet_x,
            tablet_y: active.tablet_y,
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
            tablet_x: command.tablet_x,
            tablet_y: command.tablet_y,
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

    fn run(mut self, queue: Arc<RealtimeQueue<ShortcutWorkerEvent>>) {
        while let Some(event) = queue.recv() {
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
        tablet_x: frame.x,
        tablet_y: frame.y,
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

    fn pen_command(
        kind: PenInjectionCommandKind,
        in_range: bool,
        is_contact: bool,
        x: i32,
        y: i32,
        tablet_x: u16,
        tablet_y: u16,
    ) -> PenInjectionCommand {
        PenInjectionCommand {
            x,
            y,
            tablet_x,
            tablet_y,
            kind,
            in_range,
            is_contact,
            pressure: if is_contact { 512 } else { 0 },
            tilt_x: 0,
            tilt_y: 0,
        }
    }

    fn hover_observations(samples: &[(u64, u16, u16)]) -> VecDeque<HoverObservation> {
        samples
            .iter()
            .map(|&(timestamp_ms, tablet_x, tablet_y)| HoverObservation {
                timestamp_ms,
                coordinate: PenCoordinate {
                    x: i32::from(tablet_x),
                    y: i32::from(tablet_y),
                    tablet_x,
                    tablet_y,
                },
            })
            .collect()
    }

    #[test]
    fn hover_stability_requires_the_short_window_endpoint_constraint() {
        let observations = hover_observations(&[(1_000, 1_000, 2_000), (1_060, 1_060, 2_000)]);
        let previous = observations[0].coordinate;
        let current = observations[1].coordinate;

        assert_eq!(
            hover_stability(&observations, previous, current, 1_060),
            HoverStability::Tolerance
        );
    }

    #[test]
    fn hover_stability_requires_the_trajectory_center_dispersion_constraint() {
        let observations = hover_observations(&[
            (1_000, 1_000, 2_000),
            (1_040, 1_100, 2_000),
            (1_060, 1_000, 2_000),
            (1_080, 900, 2_000),
            (1_120, 1_000, 2_000),
        ]);
        let previous = observations[3].coordinate;
        let current = observations[4].coordinate;

        assert_eq!(
            hover_stability(&observations, previous, current, 1_120),
            HoverStability::Tolerance
        );
    }

    #[test]
    fn precise_anchor_correction_translates_the_complete_contact_stroke() {
        let mut corrector = PreciseAnchorCorrector::default();
        for (timestamp, x, y) in [
            (1_000, 1_000, 2_000),
            (1_060, 1_006, 2_004),
            (1_120, 1_003, 2_008),
            (1_180, 1_008, 2_006),
        ] {
            let mut frame = stylus_frame(StylusEventType::Move, 0b0000_0001, x, y);
            frame.timestamp = timestamp;
            let command = pen_command(
                PenInjectionCommandKind::Update,
                true,
                false,
                i32::from(x) * 2,
                i32::from(y) * 2,
                x,
                y,
            );
            assert_eq!(corrector.process(&frame, command.clone(), true), command);
        }

        let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 1_608, 1_506);
        down_frame.timestamp = 1_220;
        let down = pen_command(
            PenInjectionCommandKind::Down,
            true,
            true,
            3_216,
            3_012,
            1_608,
            1_506,
        );
        let corrected_down = corrector.process(&down_frame, down, true);
        assert_eq!((corrected_down.x, corrected_down.y), (2_016, 4_012));
        assert_eq!(
            (corrected_down.tablet_x, corrected_down.tablet_y),
            (1_008, 2_006)
        );

        let move_frame = stylus_frame(StylusEventType::Move, 0b0000_0011, 1_658, 1_556);
        let movement = pen_command(
            PenInjectionCommandKind::Update,
            true,
            true,
            3_316,
            3_112,
            1_658,
            1_556,
        );
        let corrected_move = corrector.process(&move_frame, movement, true);
        assert_eq!((corrected_move.x, corrected_move.y), (2_116, 4_112));
        assert_eq!(
            (corrected_move.tablet_x, corrected_move.tablet_y),
            (1_058, 2_056)
        );

        let up_frame = stylus_frame(StylusEventType::Up, 0b0000_0001, 1_688, 1_586);
        let up = pen_command(
            PenInjectionCommandKind::Up,
            true,
            false,
            3_376,
            3_172,
            1_688,
            1_586,
        );
        let corrected_up = corrector.process(&up_frame, up, true);
        assert_eq!((corrected_up.x, corrected_up.y), (2_176, 4_172));
        assert_eq!(
            (corrected_up.tablet_x, corrected_up.tablet_y),
            (1_088, 2_086)
        );
    }

    #[test]
    fn precise_anchor_correction_rejects_short_or_moving_hover() {
        let mut corrector = PreciseAnchorCorrector::default();
        for (timestamp, x) in [(1_000, 1_000), (1_100, 1_200), (1_180, 1_202)] {
            let mut frame = stylus_frame(StylusEventType::Move, 0b0000_0001, x, 2_000);
            frame.timestamp = timestamp;
            let command = pen_command(
                PenInjectionCommandKind::Update,
                true,
                false,
                i32::from(x),
                2_000,
                x,
                2_000,
            );
            corrector.process(&frame, command, true);
        }

        let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 1_250, 2_000);
        down_frame.timestamp = 1_250;
        let down = pen_command(
            PenInjectionCommandKind::Down,
            true,
            true,
            1_250,
            2_000,
            1_250,
            2_000,
        );

        assert_eq!(corrector.process(&down_frame, down.clone(), true), down);
    }

    #[test]
    fn precise_anchor_correction_counts_quiet_hover_until_down() {
        let mut corrector = PreciseAnchorCorrector::default();
        let mut hover_frame = stylus_frame(StylusEventType::Move, 0b0000_0001, 1_000, 2_000);
        hover_frame.timestamp = 1_000;
        let hover = pen_command(
            PenInjectionCommandKind::Update,
            true,
            false,
            2_000,
            4_000,
            1_000,
            2_000,
        );
        corrector.process(&hover_frame, hover, true);

        let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 1_200, 2_100);
        down_frame.timestamp = 1_220;
        let down = pen_command(
            PenInjectionCommandKind::Down,
            true,
            true,
            2_400,
            4_200,
            1_200,
            2_100,
        );

        let corrected = corrector.process(&down_frame, down, true);
        assert_eq!((corrected.x, corrected.y), (2_000, 4_000));
        assert_eq!((corrected.tablet_x, corrected.tablet_y), (1_000, 2_000));
    }

    #[test]
    fn precise_anchor_correction_keeps_stability_across_a_brief_excursion() {
        let mut corrector = PreciseAnchorCorrector::default();
        for (timestamp, x) in [
            (1_000, 1_000),
            (1_100, 1_000),
            (1_200, 1_000),
            (1_210, 1_100),
            (1_220, 1_005),
        ] {
            let mut frame = stylus_frame(StylusEventType::Move, 0b0000_0001, x, 2_000);
            frame.timestamp = timestamp;
            corrector.process(
                &frame,
                pen_command(
                    PenInjectionCommandKind::Update,
                    true,
                    false,
                    i32::from(x),
                    2_000,
                    x,
                    2_000,
                ),
                true,
            );
        }

        let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 1_205, 2_100);
        down_frame.timestamp = 1_240;
        let down = pen_command(
            PenInjectionCommandKind::Down,
            true,
            true,
            1_205,
            2_100,
            1_205,
            2_100,
        );

        let corrected = corrector.process(&down_frame, down, true);
        assert_eq!((corrected.tablet_x, corrected.tablet_y), (1_005, 2_000));
    }

    #[test]
    fn precise_anchor_correction_resets_after_sustained_excursion() {
        let mut corrector = PreciseAnchorCorrector::default();
        for (timestamp, x) in [
            (1_000, 1_000),
            (1_100, 1_000),
            (1_200, 1_000),
            (1_210, 1_100),
            (1_240, 1_100),
        ] {
            let mut frame = stylus_frame(StylusEventType::Move, 0b0000_0001, x, 2_000);
            frame.timestamp = timestamp;
            corrector.process(
                &frame,
                pen_command(
                    PenInjectionCommandKind::Update,
                    true,
                    false,
                    i32::from(x),
                    2_000,
                    x,
                    2_000,
                ),
                true,
            );
        }

        let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 1_250, 2_100);
        down_frame.timestamp = 1_300;
        let down = pen_command(
            PenInjectionCommandKind::Down,
            true,
            true,
            1_250,
            2_100,
            1_250,
            2_100,
        );

        assert_eq!(corrector.process(&down_frame, down.clone(), true), down);
    }

    #[test]
    fn precise_anchor_correction_resets_large_motion_after_a_sampling_gap() {
        let mut corrector = PreciseAnchorCorrector::default();
        for (timestamp, x) in [(1_000, 1_000), (1_300, 2_000)] {
            let mut frame = stylus_frame(StylusEventType::Move, 0b0000_0001, x, 2_000);
            frame.timestamp = timestamp;
            corrector.process(
                &frame,
                pen_command(
                    PenInjectionCommandKind::Update,
                    true,
                    false,
                    i32::from(x),
                    2_000,
                    x,
                    2_000,
                ),
                true,
            );
        }

        let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 2_100, 2_100);
        down_frame.timestamp = 1_400;
        let down = pen_command(
            PenInjectionCommandKind::Down,
            true,
            true,
            2_100,
            2_100,
            2_100,
            2_100,
        );

        assert_eq!(corrector.process(&down_frame, down.clone(), true), down);
    }

    #[test]
    fn precise_anchor_correction_restarts_stability_at_a_quiet_tolerance_point() {
        let mut corrector = PreciseAnchorCorrector::default();
        for (timestamp, x) in [(1_000, 1_000), (1_100, 1_100)] {
            let mut frame = stylus_frame(StylusEventType::Move, 0b0000_0001, x, 2_000);
            frame.timestamp = timestamp;
            corrector.process(
                &frame,
                pen_command(
                    PenInjectionCommandKind::Update,
                    true,
                    false,
                    i32::from(x),
                    2_000,
                    x,
                    2_000,
                ),
                true,
            );
        }

        let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 1_300, 2_100);
        down_frame.timestamp = 1_320;
        let corrected = corrector.process(
            &down_frame,
            pen_command(
                PenInjectionCommandKind::Down,
                true,
                true,
                1_300,
                2_100,
                1_300,
                2_100,
            ),
            true,
        );

        assert_eq!((corrected.tablet_x, corrected.tablet_y), (1_100, 2_000));
    }

    #[test]
    fn precise_anchor_correction_uses_one_longer_threshold_during_writing() {
        let mut corrector = PreciseAnchorCorrector::default();
        for (start_ms, x) in [(1_000, 1_000), (1_100, 1_200)] {
            let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, x, 2_000);
            down_frame.timestamp = start_ms;
            corrector.process(
                &down_frame,
                pen_command(
                    PenInjectionCommandKind::Down,
                    true,
                    true,
                    i32::from(x),
                    2_000,
                    x,
                    2_000,
                ),
                true,
            );

            let moved_x = x + 100;
            let mut move_frame = stylus_frame(StylusEventType::Move, 0b0000_0011, moved_x, 2_000);
            move_frame.timestamp = start_ms + 20;
            corrector.process(
                &move_frame,
                pen_command(
                    PenInjectionCommandKind::Update,
                    true,
                    true,
                    i32::from(moved_x),
                    2_000,
                    moved_x,
                    2_000,
                ),
                true,
            );

            let mut up_frame = stylus_frame(StylusEventType::Up, 0b0000_0001, moved_x, 2_000);
            up_frame.timestamp = start_ms + 40;
            corrector.process(
                &up_frame,
                pen_command(
                    PenInjectionCommandKind::Up,
                    true,
                    false,
                    i32::from(moved_x),
                    2_000,
                    moved_x,
                    2_000,
                ),
                true,
            );
        }

        let mut hover_frame = stylus_frame(StylusEventType::Move, 0b0000_0001, 2_000, 2_000);
        hover_frame.timestamp = 1_200;
        corrector.process(
            &hover_frame,
            pen_command(
                PenInjectionCommandKind::Update,
                true,
                false,
                2_000,
                2_000,
                2_000,
                2_000,
            ),
            true,
        );

        let mut early_down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 2_200, 2_100);
        early_down_frame.timestamp = 1_420;
        let early_down = pen_command(
            PenInjectionCommandKind::Down,
            true,
            true,
            2_200,
            2_100,
            2_200,
            2_100,
        );
        assert_eq!(
            corrector.process(&early_down_frame, early_down.clone(), true),
            early_down
        );

        let mut early_up_frame = stylus_frame(StylusEventType::Up, 0b0000_0001, 2_200, 2_100);
        early_up_frame.timestamp = 1_440;
        corrector.process(
            &early_up_frame,
            pen_command(
                PenInjectionCommandKind::Up,
                true,
                false,
                2_200,
                2_100,
                2_200,
                2_100,
            ),
            true,
        );

        hover_frame.timestamp = 1_500;
        corrector.process(
            &hover_frame,
            pen_command(
                PenInjectionCommandKind::Update,
                true,
                false,
                2_000,
                2_000,
                2_000,
                2_000,
            ),
            true,
        );

        let mut down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 2_200, 2_100);
        down_frame.timestamp = 1_780;
        let corrected = corrector.process(
            &down_frame,
            pen_command(
                PenInjectionCommandKind::Down,
                true,
                true,
                2_200,
                2_100,
                2_200,
                2_100,
            ),
            true,
        );
        assert_eq!((corrected.tablet_x, corrected.tablet_y), (2_000, 2_000));
    }

    #[test]
    fn precise_anchor_correction_discards_anchor_after_leaving_hover_range() {
        let mut corrector = PreciseAnchorCorrector::default();
        for timestamp in [1_000, 1_140, 1_280, 1_400] {
            let mut frame = stylus_frame(StylusEventType::Move, 0b0000_0001, 1_000, 2_000);
            frame.timestamp = timestamp;
            corrector.process(
                &frame,
                pen_command(
                    PenInjectionCommandKind::Update,
                    true,
                    false,
                    1_000,
                    2_000,
                    1_000,
                    2_000,
                ),
                true,
            );
        }

        let out_of_range = stylus_frame(StylusEventType::Move, 0, 1_000, 2_000);
        corrector.process(
            &out_of_range,
            pen_command(
                PenInjectionCommandKind::Update,
                false,
                false,
                1_000,
                2_000,
                1_000,
                2_000,
            ),
            true,
        );

        let down_frame = stylus_frame(StylusEventType::Down, 0b0000_0011, 1_500, 2_500);
        let down = pen_command(
            PenInjectionCommandKind::Down,
            true,
            true,
            1_500,
            2_500,
            1_500,
            2_500,
        );

        assert_eq!(corrector.process(&down_frame, down.clone(), true), down);
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

    fn stylus_worker_event(seq: u32, event_type: StylusEventType) -> StylusWorkerEvent {
        let mut frame = stylus_frame(event_type, 0b0000_0001, seq as u16, seq as u16);
        frame.seq = seq;
        StylusWorkerEvent::Stylus {
            session_id: "session-a".to_string(),
            sample: StylusSample {
                frame,
                mapped_command: None,
                queued_at: None,
                accepted_at: Instant::now(),
                preempt_previous: false,
            },
        }
    }

    fn contact_move(seq: u32, timestamp: u64, pressure: f32) -> StylusWorkerEvent {
        let mut event = stylus_worker_event(seq, StylusEventType::Move);
        let StylusWorkerEvent::Stylus { sample, .. } = &mut event else {
            unreachable!("helper always creates a stylus event");
        };
        sample.frame.timestamp = timestamp;
        sample.frame.flags = StylusFlags(0b0000_0011);
        sample.frame.pressure = pressure;
        sample.mapped_command = None;
        event
    }

    #[test]
    fn latest_priority_removes_only_moves_after_current_stroke_boundary() {
        let mut events = VecDeque::from([
            stylus_worker_event(1, StylusEventType::Down),
            contact_move(2, 2, 0.5),
            stylus_worker_event(3, StylusEventType::Up),
            stylus_worker_event(4, StylusEventType::Down),
            contact_move(5, 5, 0.5),
        ]);
        let incoming = contact_move(6, 6, 0.5);
        let StylusWorkerEvent::Stylus { sample, .. } = &mut events[4] else {
            unreachable!()
        };
        sample.accepted_at = Instant::now() - Duration::from_millis(20);
        compact_stylus_queue(
            &mut events,
            &incoming,
            true,
            10,
            HoverMovePolicy::PreserveAll,
            false,
        );

        assert_eq!(events.len(), 4);
        assert!(events.iter().any(|event| matches!(event, StylusWorkerEvent::Stylus { sample, .. } if sample.frame.seq == 2)));
        assert!(!events.iter().any(|event| matches!(event, StylusWorkerEvent::Stylus { sample, .. } if sample.frame.seq == 5)));
    }

    #[test]
    fn latest_priority_keeps_moves_inside_tolerance_window() {
        let mut events = VecDeque::from([
            stylus_worker_event(1, StylusEventType::Down),
            contact_move(2, 2, 0.5),
        ]);
        let incoming = contact_move(3, 3, 0.5);
        compact_stylus_queue(
            &mut events,
            &incoming,
            true,
            100,
            HoverMovePolicy::PreserveAll,
            false,
        );

        assert_eq!(events.len(), 2);
        assert!(events.iter().any(|event| matches!(event, StylusWorkerEvent::Stylus { sample, .. } if sample.frame.seq == 2)));
    }

    #[test]
    fn stroke_preemption_discards_only_queued_stylus_events_for_session() {
        let mut events = VecDeque::from([
            contact_move(2, 2, 0.5),
            StylusWorkerEvent::SessionEnded {
                session_id: "other-session".to_string(),
            },
        ]);
        let incoming = stylus_worker_event(3, StylusEventType::Down);
        compact_stylus_queue(
            &mut events,
            &incoming,
            false,
            0,
            HoverMovePolicy::PreserveAll,
            true,
        );

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.front(),
            Some(StylusWorkerEvent::SessionEnded { .. })
        ));
    }

    #[test]
    fn realtime_queue_preserves_all_hover_moves_by_default() {
        let queue = RealtimeQueue::new();

        queue
            .push(stylus_worker_event(1, StylusEventType::Move))
            .expect("first move should queue");
        queue
            .push(stylus_worker_event(2, StylusEventType::Move))
            .expect("second move should queue");

        assert!(matches!(
            queue.recv(),
            Some(StylusWorkerEvent::Stylus { sample, .. }) if sample.frame.seq == 1
        ));
        assert!(matches!(
            queue.recv(),
            Some(StylusWorkerEvent::Stylus { sample, .. }) if sample.frame.seq == 2
        ));
    }

    #[test]
    fn hover_reduction_policies_use_their_declared_intervals() {
        let now = Instant::now();
        for (policy, age_ms, should_remove) in [
            (HoverMovePolicy::LightReduction, 3, true),
            (HoverMovePolicy::LightReduction, 5, false),
            (HoverMovePolicy::BalancedReduction, 5, true),
            (HoverMovePolicy::BalancedReduction, 9, false),
        ] {
            let mut queued = stylus_worker_event(1, StylusEventType::Move);
            let mut incoming = stylus_worker_event(2, StylusEventType::Move);
            let StylusWorkerEvent::Stylus {
                sample: queued_sample,
                ..
            } = &mut queued
            else {
                unreachable!("helper always creates a stylus event");
            };
            queued_sample.accepted_at = now - Duration::from_millis(age_ms);
            let StylusWorkerEvent::Stylus {
                sample: incoming_sample,
                ..
            } = &mut incoming
            else {
                unreachable!("helper always creates a stylus event");
            };
            incoming_sample.accepted_at = now;

            let mut events = VecDeque::from([queued]);
            compact_stylus_queue(&mut events, &incoming, false, 0, policy, false);

            assert_eq!(events.is_empty(), should_remove);
        }
    }

    #[test]
    fn latest_hover_policy_removes_only_the_current_consecutive_hover_run() {
        let mut events = VecDeque::from([
            stylus_worker_event(1, StylusEventType::Down),
            stylus_worker_event(2, StylusEventType::Move),
            stylus_worker_event(3, StylusEventType::Move),
            stylus_worker_event(4, StylusEventType::Up),
            stylus_worker_event(5, StylusEventType::Move),
        ]);
        let incoming = stylus_worker_event(6, StylusEventType::Move);

        compact_stylus_queue(
            &mut events,
            &incoming,
            false,
            0,
            HoverMovePolicy::Latest,
            false,
        );

        assert_eq!(events.len(), 4);
        assert!(events.iter().any(|event| matches!(
            event,
            StylusWorkerEvent::Stylus { sample, .. } if sample.frame.seq == 2
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StylusWorkerEvent::Stylus { sample, .. } if sample.frame.seq == 3
        )));
        assert!(!events.iter().any(|event| matches!(
            event,
            StylusWorkerEvent::Stylus { sample, .. } if sample.frame.seq == 5
        )));
    }

    #[test]
    fn realtime_queue_preserves_stylus_events_by_default() {
        let queue = RealtimeQueue::new();
        for (seq, event_type) in [
            (1, StylusEventType::Move),
            (2, StylusEventType::Up),
            (3, StylusEventType::Move),
            (4, StylusEventType::Move),
        ] {
            queue
                .push(stylus_worker_event(seq, event_type))
                .expect("stylus event should queue");
        }

        for (expected_seq, expected_type) in [
            (1, StylusEventType::Move),
            (2, StylusEventType::Up),
            (3, StylusEventType::Move),
            (4, StylusEventType::Move),
        ] {
            assert!(matches!(
                queue.recv(),
                Some(StylusWorkerEvent::Stylus { sample, .. })
                    if sample.frame.seq == expected_seq
                        && sample.frame.event_type == expected_type
            ));
        }
    }

    #[test]
    fn realtime_queue_preserves_every_contact_move_pressure_sample() {
        let queue = RealtimeQueue::new();
        for seq in [1, 2] {
            queue
                .push(contact_move(seq, 10 + u64::from(seq), seq as f32 / 2.0))
                .expect("contact move should queue");
        }

        for (expected_seq, expected_pressure) in [(1, 0.5), (2, 1.0)] {
            assert!(matches!(
                queue.recv(),
            Some(StylusWorkerEvent::Stylus { sample, .. })
                if sample.frame.seq == expected_seq
                    && sample.frame.pressure == expected_pressure
            ));
        }
    }

    #[test]
    fn realtime_queue_preserves_all_contact_moves_under_burst() {
        let queue = RealtimeQueue::new();
        for seq in 1..=9 {
            queue
                .push(contact_move(seq, u64::from(seq), seq as f32 / 9.0))
                .expect("contact move should queue");
        }

        for expected_seq in 1..=9 {
            assert!(matches!(
                queue.recv(),
                Some(StylusWorkerEvent::Stylus { sample, .. })
                    if sample.frame.seq == expected_seq
            ));
        }
    }

    #[test]
    fn realtime_queue_preserves_contact_moves_regardless_of_age() {
        let queue = RealtimeQueue::new();
        for event in [contact_move(1, 100, 0.8), contact_move(2, 117, 0.5)] {
            queue.push(event).expect("contact move should queue");
        }

        assert!(matches!(
            queue.recv(),
            Some(StylusWorkerEvent::Stylus { sample, .. })
                if sample.frame.seq == 1 && sample.frame.pressure == 0.8
        ));
        assert!(matches!(
            queue.recv(),
            Some(StylusWorkerEvent::Stylus { sample, .. })
                if sample.frame.seq == 2 && sample.frame.pressure == 0.5
        ));
    }

    #[test]
    fn realtime_queue_never_compacts_contact_moves_across_pen_up() {
        let queue = RealtimeQueue::new();
        for event in [
            contact_move(1, 100, 0.8),
            stylus_worker_event(2, StylusEventType::Up),
            contact_move(3, 120, 0.5),
        ] {
            queue.push(event).expect("stylus event should queue");
        }

        for expected_seq in [1, 2, 3] {
            assert!(matches!(
                queue.recv(),
                Some(StylusWorkerEvent::Stylus { sample, .. })
                    if sample.frame.seq == expected_seq
            ));
        }
    }

    #[test]
    fn high_frequency_multi_stroke_burst_preserves_every_pen_command() {
        let (entered_sender, entered_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let injector = Arc::new(BlockingInjector::new(entered_sender, release_receiver));
        let shortcut = Arc::new(RecordingShortcutExecutor::default());
        let pipeline = StylusInputPipeline::new(
            test_workspace(),
            injector.clone(),
            shortcut,
            test_pressure_settings(),
            test_shortcut_profile(),
            crate::shortcut::null_radial_menu_overlay(),
        );

        let stroke_count = 8;
        let moves_per_stroke = 16;
        let mut seq = 1;
        let mut emit_frame = |event_type, flags| {
            let mut frame = stylus_frame(event_type, flags, seq as u16, seq as u16);
            frame.seq = seq;
            seq += 1;
            pipeline.emit(IncomingEvent::Stylus {
                session_id: "session-a".to_string(),
                source_ip: Ipv4Addr::new(127, 0, 0, 1),
                frame,
            });
        };

        emit_frame(StylusEventType::Down, 0b0000_0011);
        entered_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("first pen injection should reach the blocking injector");

        for _ in 0..moves_per_stroke {
            emit_frame(StylusEventType::Move, 0b0000_0011);
        }
        emit_frame(StylusEventType::Up, 0);

        for _ in 1..stroke_count {
            emit_frame(StylusEventType::Down, 0b0000_0011);
            for _ in 0..moves_per_stroke {
                emit_frame(StylusEventType::Move, 0b0000_0011);
            }
            emit_frame(StylusEventType::Up, 0);
        }

        release_sender
            .send(())
            .expect("blocking injector should be released");
        let expected_count = stroke_count * (moves_per_stroke + 2);
        wait_until(Duration::from_secs(1), || {
            injector
                .commands
                .lock()
                .expect("injector should lock")
                .len()
                == expected_count
        });

        let commands = injector.commands.lock().expect("injector should lock");
        for stroke in commands.chunks_exact(moves_per_stroke + 2) {
            assert_eq!(stroke[0].kind, PenInjectionCommandKind::Down);
            assert!(
                stroke[1..=moves_per_stroke]
                    .iter()
                    .all(|command| command.kind == PenInjectionCommandKind::Update)
            );
            assert_eq!(
                stroke[moves_per_stroke + 1].kind,
                PenInjectionCommandKind::Up
            );
        }
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
        assert_eq!((origin.tablet_x, origin.tablet_y), (0, 0));
        assert_eq!((corner.tablet_x, corner.tablet_y), (32_767, 32_767));
        assert_eq!((center.tablet_x, center.tablet_y), (16_384, 16_384));
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
        assert!(!commands[1].is_contact);
        assert!(!commands[1].in_range);
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
                ShortcutCommand::ClickAt {
                    button: crate::shortcut::MouseButton::Right,
                    x: 217,
                    y: 332
                },
                ShortcutCommand::ClickAt {
                    button: crate::shortcut::MouseButton::Right,
                    x: 276,
                    y: 365
                },
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
                    tablet_x: 100,
                    tablet_y: 200,
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
                    tablet_x: 100,
                    tablet_y: 200,
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
