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
        let stylus_worker = thread::spawn(move || {
            StylusWorker::new(
                stylus_workspace,
                injector,
                stylus_pressure_settings,
                stylus_worker_metrics,
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
        let result = if latest_only || preempt {
            self.stylus_queue.push_mutating(event, |events, incoming| {
                compact_stylus_queue(events, incoming, latest_only, tolerance_ms, preempt)
            })
        } else {
            self.stylus_queue.push_with(event, can_coalesce_stylus)
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
}

fn can_coalesce_stylus(queued: &StylusWorkerEvent, incoming: &StylusWorkerEvent) -> bool {
    matches!(
        (queued, incoming),
        (
            StylusWorkerEvent::Stylus {
                session_id: queued_session,
                sample: queued_sample,
            },
            StylusWorkerEvent::Stylus {
                session_id: incoming_session,
                sample: incoming_sample,
            },
        ) if queued_session == incoming_session
            && queued_sample.frame.event_type == StylusEventType::Move
            && incoming_sample.frame.event_type == StylusEventType::Move
            && !queued_sample.frame.flags.is_contact()
            && !incoming_sample.frame.flags.is_contact()
    )
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
            } => {
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
            IncomingEvent::SessionEnded { session_id, .. } => {
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
    state: Option<ActivePenState>,
    ended_sessions: HashSet<String>,
}

impl StylusWorker {
    fn new(
        workspace: WorkspaceService,
        injector: Arc<dyn PenInjector>,
        pressure_settings: SharedPressureSettings,
        metrics: Arc<InputPipelineMetrics>,
    ) -> Self {
        Self {
            workspace,
            injector,
            pressure_settings,
            metrics,
            state: None,
            ended_sessions: HashSet::new(),
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
        let cancel = PenInjectionCommand {
            x: active.x,
            y: active.y,
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
        compact_stylus_queue(&mut events, &incoming, true, 10, false);

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
        compact_stylus_queue(&mut events, &incoming, true, 100, false);

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
        compact_stylus_queue(&mut events, &incoming, false, 0, true);

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.front(),
            Some(StylusWorkerEvent::SessionEnded { .. })
        ));
    }

    #[test]
    fn realtime_queue_keeps_only_latest_consecutive_stylus_move() {
        let queue = RealtimeQueue::new();

        queue
            .push_with(
                stylus_worker_event(1, StylusEventType::Move),
                can_coalesce_stylus,
            )
            .expect("first move should queue");
        queue
            .push_with(
                stylus_worker_event(2, StylusEventType::Move),
                can_coalesce_stylus,
            )
            .expect("latest move should replace queued move");

        assert!(matches!(
            queue.recv(),
            Some(StylusWorkerEvent::Stylus { sample, .. }) if sample.frame.seq == 2
        ));
    }

    #[test]
    fn realtime_queue_preserves_key_events_between_stylus_moves() {
        let queue = RealtimeQueue::new();
        for (seq, event_type) in [
            (1, StylusEventType::Move),
            (2, StylusEventType::Up),
            (3, StylusEventType::Move),
            (4, StylusEventType::Move),
        ] {
            queue
                .push_with(stylus_worker_event(seq, event_type), can_coalesce_stylus)
                .expect("stylus event should queue");
        }

        for (expected_seq, expected_type) in [
            (1, StylusEventType::Move),
            (2, StylusEventType::Up),
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
                .push_with(
                    contact_move(seq, 10 + u64::from(seq), seq as f32 / 2.0),
                    can_coalesce_stylus,
                )
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
                .push_with(
                    contact_move(seq, u64::from(seq), seq as f32 / 9.0),
                    can_coalesce_stylus,
                )
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
            queue
                .push_with(event, can_coalesce_stylus)
                .expect("contact move should queue");
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
            queue
                .push_with(event, can_coalesce_stylus)
                .expect("stylus event should queue");
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
