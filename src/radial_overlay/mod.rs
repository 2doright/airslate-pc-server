use std::{
    sync::{
        Arc,
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
};

use tracing::warn;

use crate::{
    error::AppError,
    shortcut::{RadialMenuOverlay, RadialMenuOverlayState, ScreenPoint, SharedRadialMenuOverlay},
};

#[cfg(target_os = "macos")]
mod portable;
#[cfg(windows)]
mod windows;

#[derive(Clone)]
pub struct RadialOverlayController {
    sender: Sender<OverlayCommand>,
}

impl RadialMenuOverlay for RadialOverlayController {
    fn show(&self, state: RadialMenuOverlayState) {
        let _ = self.sender.send(OverlayCommand::Show(state));
    }

    fn update(&self, state: RadialMenuOverlayState) {
        let _ = self.sender.send(OverlayCommand::Update(state));
    }

    fn hide(&self) {
        let _ = self.sender.send(OverlayCommand::Hide);
    }

    fn sync_hold_indicator(&self, point: Option<ScreenPoint>) {
        let _ = self.sender.send(OverlayCommand::SyncHoldIndicator(point));
    }

    fn shutdown(&self) {
        let _ = self.sender.send(OverlayCommand::Shutdown);
    }
}

pub struct RadialOverlayService {
    controller: SharedRadialMenuOverlay,
    worker: Option<JoinHandle<()>>,
}

impl RadialOverlayService {
    pub fn new() -> Result<Self, AppError> {
        let (sender, receiver) = mpsc::channel();
        let controller: SharedRadialMenuOverlay = Arc::new(RadialOverlayController { sender });
        let worker = thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let result = portable::run_overlay_thread(receiver);
            #[cfg(windows)]
            let result = windows::run_overlay_thread(receiver);

            if let Err(error) = result {
                warn!(error = %error, "radial overlay thread stopped");
            }
        });

        Ok(Self {
            controller,
            worker: Some(worker),
        })
    }

    pub fn controller(&self) -> SharedRadialMenuOverlay {
        self.controller.clone()
    }
}

impl Drop for RadialOverlayService {
    fn drop(&mut self) {
        self.controller.shutdown();

        if let Some(handle) = self.worker.take()
            && handle.join().is_err()
        {
            warn!("radial overlay worker panicked during shutdown");
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum OverlayCommand {
    Show(RadialMenuOverlayState),
    Update(RadialMenuOverlayState),
    Hide,
    SyncHoldIndicator(Option<ScreenPoint>),
    Shutdown,
}
