use std::sync::mpsc::Receiver;

use tracing::debug;

use crate::{error::AppError, radial_overlay::OverlayCommand};

pub(crate) fn run_overlay_thread(receiver: Receiver<OverlayCommand>) -> Result<(), AppError> {
    while let Ok(command) = receiver.recv() {
        match command {
            OverlayCommand::Show(state) | OverlayCommand::Update(state) => {
                debug!(
                    x = state.anchor.center.x,
                    y = state.anchor.center.y,
                    selection = ?state.selection,
                    "radial overlay is not implemented on this platform"
                );
            }
            OverlayCommand::Hide => {}
            OverlayCommand::SyncHoldIndicator(point) => {
                debug!(
                    ?point,
                    "hold indicator overlay is not implemented on this platform"
                );
            }
            OverlayCommand::Shutdown => return Ok(()),
        }
    }

    Ok(())
}
