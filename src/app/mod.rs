mod bootstrap;
pub(crate) mod lifecycle;
pub(crate) mod state;

use std::{path::PathBuf, sync::Arc, thread};

use tracing::info;

use crate::{
    config::{Config, config_path},
    error::AppError,
    handshake::HandshakeService,
    input_pipeline::{PenInjector, StylusInputPipeline},
    radial_overlay::RadialOverlayService,
    session::SessionService,
    shortcut::ShortcutExecutor,
    udp_ingest::{IncomingEventSink, UdpIngestService},
    usb_accessory::{UsbAccessoryService, UsbScanHistory, UsbSessionControl, UsbStatusBus},
    windows_injector::{WindowsPenInjector, WindowsShortcutExecutor},
    workspace::WorkspaceService,
};

use self::lifecycle::{SessionLifecycle, SessionStatusBus, WiredConnectionGate};
use self::state::AppRuntime;

#[derive(Clone)]
pub struct AppContext {
    pub config_path: PathBuf,
    pub workspace: WorkspaceService,
    pub runtime: AppRuntime,
    pub session_lifecycle: Arc<SessionLifecycle>,
    pub usb_status_bus: Arc<UsbStatusBus>,
    pub usb_scan_history: Arc<UsbScanHistory>,
    pub usb_session_control: Arc<UsbSessionControl>,
    _radial_overlay: Arc<RadialOverlayService>,
}

pub fn initialize() -> Result<AppContext, AppError> {
    info!("starting airslate_pc_server");

    let path = config_path()?;
    info!(path = %path.display(), "resolved config path");

    let config = Config::load_or_create(&path)?;
    info!(version = config.config_version, app = %config.app_name, "loaded config");

    bootstrap::validate()?;

    let workspace = WorkspaceService::new(&config)?;
    log_workspace_state(&workspace);

    let session = SessionService::shared();
    info!(
        has_active_session = session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))?
            .has_active_session(),
        "phase 3 session service ready"
    );

    let runtime = AppRuntime::new(
        path.clone(),
        config.clone(),
        workspace.clone(),
        session.clone(),
    );
    let radial_overlay = Arc::new(RadialOverlayService::new()?);
    let injector: Arc<dyn PenInjector> = Arc::new(WindowsPenInjector::new()?);
    let shortcut_executor: Arc<dyn ShortcutExecutor> = Arc::new(WindowsShortcutExecutor::new());
    let input_sink: Arc<dyn IncomingEventSink> = Arc::new(StylusInputPipeline::new_with_settings(
        workspace.clone(),
        injector,
        shortcut_executor,
        runtime.pressure_settings(),
        runtime.shortcut_profile(),
        radial_overlay.controller(),
        runtime.input_processing_settings(),
    ));
    let wired_gate = WiredConnectionGate::shared(config.wired_connection_enabled);
    let session_lifecycle = Arc::new(SessionLifecycle::new_with_wired_gate(
        session.clone(),
        input_sink,
        SessionStatusBus::shared(),
        wired_gate.clone(),
    ));
    let usb_status_bus = UsbStatusBus::shared(config.wired_connection_enabled);
    let usb_scan_history = UsbScanHistory::shared();
    let usb_session_control = UsbSessionControl::shared_with_gate(wired_gate);

    Ok(AppContext {
        config_path: path,
        workspace,
        runtime,
        session_lifecycle,
        usb_status_bus,
        usb_scan_history,
        usb_session_control,
        _radial_overlay: radial_overlay,
    })
}

pub fn start_services(context: &AppContext) -> Result<(), AppError> {
    let handshake =
        HandshakeService::new(context.workspace.clone(), context.session_lifecycle.clone());
    let udp_ingest = UdpIngestService::new(context.session_lifecycle.clone());
    let usb_accessory = UsbAccessoryService::new(
        context.runtime.clone(),
        context.session_lifecycle.clone(),
        context.usb_status_bus.clone(),
        context.usb_session_control.clone(),
        context.usb_scan_history.clone(),
    );

    let _udp_thread = thread::spawn(move || {
        if let Err(error) = udp_ingest.run() {
            tracing::warn!(error = %error, "udp ingest service stopped");
        }
    });
    let _handshake_thread = thread::spawn(move || {
        if let Err(error) = handshake.run() {
            tracing::warn!(error = %error, "handshake service stopped");
        }
    });
    let _usb_thread = thread::spawn(move || usb_accessory.run());

    Ok(())
}

fn log_workspace_state(workspace: &WorkspaceService) {
    let snapshot = match workspace.snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            info!(error = %error, "failed to load workspace snapshot for startup logging");
            return;
        }
    };

    info!(
        monitor_count = snapshot.monitors.len(),
        "loaded workspace snapshot"
    );
    for monitor in &snapshot.monitors {
        info!(
            id = monitor.id.as_str(),
            device = %monitor.device_name,
            is_primary = monitor.is_primary,
            pixel_width = monitor.pixel_width,
            pixel_height = monitor.pixel_height,
            virtual_left = monitor.virtual_left,
            virtual_top = monitor.virtual_top,
            virtual_right = monitor.virtual_right,
            virtual_bottom = monitor.virtual_bottom,
            "detected monitor"
        );
    }

    match workspace.current_workspace() {
        Ok(current_workspace) => {
            info!(
                active_monitor_id = current_workspace.monitor.id.as_str(),
                active_width = current_workspace.monitor.pixel_width,
                active_height = current_workspace.monitor.pixel_height,
                "resolved active workspace"
            );
        }
        Err(error) => {
            info!(error = %error, "no active workspace resolved during startup");
        }
    }
}
