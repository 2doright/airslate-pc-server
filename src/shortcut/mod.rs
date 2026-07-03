mod domain;
mod engine;
mod preset;
mod profile;
pub mod radial_menu;

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tracing::{info, warn};

use crate::{
    error::AppError,
    protocol::{GestureFrame, StylusFlags},
    workspace::WorkspaceService,
};

use self::engine::ShortcutEngine;
pub use self::{
    domain::{
        AdvancedAction, BindingId, GestureBinding, KeyCode, MouseButton, PointerAnchor,
        RadialInnerBindings, RadialInnerSlot, RadialMenuConfig, ScreenPoint, ShortcutAction,
        ShortcutCommand, StylusTrigger, SwipeAxis,
    },
    profile::{
        SharedShortcutProfile, ShortcutPreset, ShortcutPresetLibrary, ShortcutProfile, all_bindings,
    },
    radial_menu::{RadialAnchor, RadialSelection},
};

const TTL_SWEEP_INTERVAL: Duration = Duration::from_millis(20);

pub trait ShortcutExecutor: Send + Sync {
    fn execute(&self, command: ShortcutCommand) -> Result<(), AppError>;
}

pub trait RadialMenuOverlay: Send + Sync {
    fn show(&self, state: RadialMenuOverlayState);
    fn update(&self, state: RadialMenuOverlayState);
    fn hide(&self);
    fn sync_hold_indicator(&self, _point: Option<ScreenPoint>) {}
    fn shutdown(&self) {}
}

pub type SharedRadialMenuOverlay = Arc<dyn RadialMenuOverlay>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadialMenuOverlayState {
    pub anchor: RadialAnchor,
    pub selection: RadialSelection,
    pub active_inner_slots: Vec<RadialInnerSlot>,
    pub config: RadialMenuConfig,
}

#[cfg(test)]
struct NullRadialMenuOverlay;

#[cfg(test)]
impl RadialMenuOverlay for NullRadialMenuOverlay {
    fn show(&self, _state: RadialMenuOverlayState) {}

    fn update(&self, _state: RadialMenuOverlayState) {}

    fn hide(&self) {}
}

#[cfg(test)]
pub fn null_radial_menu_overlay() -> SharedRadialMenuOverlay {
    Arc::new(NullRadialMenuOverlay)
}

pub struct ShortcutRuntime {
    engine: Arc<Mutex<ShortcutEngine>>,
    executor: Arc<dyn ShortcutExecutor>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl ShortcutRuntime {
    pub fn new(
        executor: Arc<dyn ShortcutExecutor>,
        profile: SharedShortcutProfile,
        overlay: SharedRadialMenuOverlay,
        workspace: WorkspaceService,
    ) -> Self {
        let engine = Arc::new(Mutex::new(ShortcutEngine::new(profile, overlay, workspace)));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = spawn_ttl_worker(engine.clone(), executor.clone(), stop.clone());

        Self {
            engine,
            executor,
            stop,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn handle_pointer_context(
        &self,
        session_id: &str,
        point: ScreenPoint,
        in_range: bool,
        is_contact: bool,
    ) {
        self.process(session_id, |engine, _| {
            engine.update_pointer_context(point, in_range, is_contact);
            Vec::new()
        });
    }

    pub fn handle_stylus_flags(&self, session_id: &str, seq: u32, flags: StylusFlags) {
        self.process(session_id, |engine, now| {
            engine.process_stylus_flags(seq, flags, now)
        });
    }

    pub fn handle_gesture_frame(&self, session_id: &str, frame: &GestureFrame) {
        self.process(session_id, |engine, now| engine.process_gesture(frame, now));
    }

    pub fn handle_session_end(&self, session_id: &str) {
        self.process(session_id, |engine, _| engine.handle_session_end());
    }

    fn process<F>(&self, session_id: &str, handler: F)
    where
        F: FnOnce(&mut ShortcutEngine, Instant) -> Vec<ShortcutCommand>,
    {
        let commands = match self
            .engine
            .lock()
            .map_err(|_| AppError::StatePoisoned("shortcut_engine"))
        {
            Ok(mut engine) => handler(&mut engine, Instant::now()),
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "failed to lock shortcut engine");
                return;
            }
        };

        execute_commands(&*self.executor, session_id, commands);
    }
}

impl Drop for ShortcutRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);

        if let Ok(mut engine) = self.engine.lock() {
            let commands = engine.handle_session_end();
            execute_commands(&*self.executor, "runtime_shutdown", commands);
        }

        let handle = self
            .worker
            .lock()
            .map_err(|_| AppError::StatePoisoned("shortcut_runtime_worker"))
            .ok()
            .and_then(|mut worker| worker.take());

        if let Some(handle) = handle
            && handle.join().is_err()
        {
            warn!("shortcut ttl worker panicked during shutdown");
        }
    }
}

fn spawn_ttl_worker(
    engine: Arc<Mutex<ShortcutEngine>>,
    executor: Arc<dyn ShortcutExecutor>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            thread::sleep(TTL_SWEEP_INTERVAL);

            let commands = match engine.lock() {
                Ok(mut engine) => engine.expire_due(Instant::now()),
                Err(_) => {
                    warn!("shortcut ttl worker failed to lock engine");
                    break;
                }
            };

            execute_commands(&*executor, "ttl_worker", commands);
        }
    })
}

fn execute_commands(
    executor: &dyn ShortcutExecutor,
    session_id: &str,
    commands: Vec<ShortcutCommand>,
) {
    for command in commands {
        if let Err(error) = executor.execute(command.clone()) {
            warn!(session_id = %session_id, error = %error, command = ?command, "failed to execute shortcut command");
            continue;
        }

        info!(session_id = %session_id, command = ?command, "shortcut command executed");
    }
}
