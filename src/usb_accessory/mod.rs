mod stream;

#[cfg(windows)]
#[allow(dead_code)]
#[path = "../bin/winusb_inbox_dry_run.rs"]
mod winusb_tool;

use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

use nusb::{
    DeviceInfo, Endpoint, MaybeFuture,
    descriptors::TransferType,
    transfer::{
        Bulk, Completion, ControlIn, ControlOut, ControlType, In, Out, Recipient, TransferError,
    },
};
use serde::Serialize;
use tracing::{info, warn};

use crate::{
    app::lifecycle::SessionLifecycle,
    handshake::HandshakeService,
    protocol::{HANDSHAKE_REQUEST_SIZE, Packet, PacketType, decode_packet},
    session::{LocalDisconnectDisposition, RealtimeFrameDisposition},
    udp_ingest::IncomingEvent,
    workspace::WorkspaceService,
};

use self::stream::PacketStream;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const REENUMERATION_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(1);
const STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);
const READY_SUBMIT_RETRY_LIMIT: u8 = 6;
const READY_SUBMIT_RETRY_DELAY: Duration = Duration::from_millis(200);
const SCAN_INTERVAL: Duration = Duration::from_secs(1);
const IO_BUFFER_SIZE: usize = 16 * 1024;
const HANDSHAKE_TIMEOUT_REPORT_INTERVAL: u32 = 30;

// USB transport bootstrap only. This is deliberately not a protocol::PacketType:
// wireless still starts with HANDSHAKE_REQUEST and the common session parser must
// never receive USB_READY.
const USB_READY: [u8; 8] = [0x54, 0x4C, 0x53, 0x41, 7, 1, 0, 0];

// OpenHarmony official device-side implementation source and the AirSlate wire contract:
// https://gitee.com/openharmony/usb_manager ; E:/Personal/AirSlate/doc/server.md §7.
const ACCESSORY_GET_PROTOCOL: u8 = 51;
const ACCESSORY_SEND_STRING: u8 = 52;
const ACCESSORY_START: u8 = 53;

// Harmony selects the accessory only by indexes 0 and 1. Keep these exact.
const ACCESSORY_IDENTITY: [(u16, &str); 6] = [
    (0, "AirSlate"),
    (1, "AirSlate PC Server"),
    (2, "AirSlate formal wired session"),
    (3, env!("CARGO_PKG_VERSION")),
    (4, "https://github.com/2doright/airslate-pc-server"),
    (5, "AirSlate-PC"),
];

static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);
pub const USB_STATUS_CHANGED_EVENT: &str = "usb-status-changed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbStatusEvent {
    pub state: &'static str,
    pub detail: String,
    pub retryable: bool,
    pub device: Option<UsbDeviceInfo>,
}

impl Default for UsbStatusEvent {
    fn default() -> Self {
        Self {
            state: "waiting",
            detail: "等待 AirSlate 平板 USB 连接".to_owned(),
            retryable: true,
            device: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbDeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus_id: String,
    pub port_chain: Vec<u8>,
    pub configuration: Option<u8>,
    pub interface_number: Option<u8>,
    pub alternate_setting: Option<u8>,
    pub bulk_in_endpoint: Option<u8>,
    pub bulk_out_endpoint: Option<u8>,
    pub bulk_in_max_packet_size: Option<usize>,
    pub bulk_out_max_packet_size: Option<usize>,
}

pub struct UsbStatusBus {
    subscriber: Mutex<Option<Sender<UsbStatusEvent>>>,
    current: Mutex<UsbStatusEvent>,
}

impl Default for UsbStatusBus {
    fn default() -> Self {
        Self {
            subscriber: Mutex::new(None),
            current: Mutex::new(UsbStatusEvent::default()),
        }
    }
}

#[derive(Default)]
pub struct UsbSessionControl {
    active: AtomicU64,
    cancelled: AtomicU64,
    retry_requested: std::sync::atomic::AtomicBool,
}

impl UsbSessionControl {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn cancel_active(&self) {
        let active = self.active.load(Ordering::Acquire);
        if active != 0 {
            self.cancelled.store(active, Ordering::Release);
        }
    }

    pub fn request_retry(&self) {
        self.retry_requested.store(true, Ordering::Release);
        self.cancel_active();
    }

    fn take_retry_requested(&self) -> bool {
        self.retry_requested.swap(false, Ordering::AcqRel)
    }

    fn is_cancelled(&self, connection_id: u64) -> bool {
        self.cancelled.load(Ordering::Acquire) == connection_id
    }
}

impl UsbStatusBus {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn subscribe(&self) -> Receiver<UsbStatusEvent> {
        let (sender, receiver) = mpsc::channel();
        if let Ok(mut subscriber) = self.subscriber.lock() {
            *subscriber = Some(sender);
        }
        receiver
    }

    pub fn snapshot(&self) -> UsbStatusEvent {
        self.current
            .lock()
            .map(|event| event.clone())
            .unwrap_or_default()
    }

    fn publish(&self, state: &'static str, detail: impl Into<String>) {
        let device = if state == "error" {
            self.snapshot().device
        } else {
            None
        };
        self.publish_with_device(state, detail, device);
    }

    fn publish_with_device(
        &self,
        state: &'static str,
        detail: impl Into<String>,
        device: Option<UsbDeviceInfo>,
    ) {
        let event = UsbStatusEvent {
            state,
            detail: detail.into(),
            retryable: state != "connected",
            device,
        };
        let should_publish = match self.current.lock() {
            Ok(mut current) => {
                let changed = *current != event;
                *current = event.clone();
                changed
            }
            Err(_) => true,
        };
        if !should_publish {
            return;
        }
        if let Ok(subscriber) = self.subscriber.lock()
            && let Some(sender) = subscriber.as_ref()
        {
            let _ = sender.send(event);
        }
    }
}

pub struct UsbAccessoryService {
    handshake: HandshakeService,
    lifecycle: Arc<SessionLifecycle>,
    status: Arc<UsbStatusBus>,
    control: Arc<UsbSessionControl>,
}

impl UsbAccessoryService {
    pub fn new(
        workspace: WorkspaceService,
        lifecycle: Arc<SessionLifecycle>,
        status: Arc<UsbStatusBus>,
        control: Arc<UsbSessionControl>,
    ) -> Self {
        Self {
            handshake: HandshakeService::new(workspace, lifecycle.clone()),
            lifecycle,
            status,
            control,
        }
    }

    pub fn run(&self) {
        info!("formal USBAccessory service started");
        self.status
            .publish("waiting", "等待 AirSlate 平板 USB 连接");
        let mut last_discovery_state = None;
        let mut failed_initial_state = None;
        let mut first_scan = true;
        loop {
            let scan_is_initial = first_scan;
            first_scan = false;
            match discover_candidate() {
                Ok(Discovery::None {
                    visible_devices,
                    initial_candidates,
                    summaries,
                }) => {
                    failed_initial_state = None;
                    let (status_state, status_detail) =
                        waiting_status(visible_devices, initial_candidates, &summaries);
                    self.status.publish(status_state, status_detail);
                    let state = DiscoveryState::None {
                        visible_devices,
                        initial_candidates,
                        summaries,
                    };
                    if discovery_state_changed(last_discovery_state.as_ref(), &state) {
                        let file_transfer_devices = state
                            .summaries()
                            .iter()
                            .filter(|summary| is_known_file_transfer_mode(summary))
                            .count();
                        info!(
                            visible_devices,
                            initial_candidates,
                            file_transfer_devices,
                            backend = nusb_backend_name(),
                            descriptors = ?state.summaries(),
                            "USBAccessory scan found no initial accessory-compatible candidate; waiting for the next physical enumeration"
                        );
                        last_discovery_state = Some(state);
                    }
                    self.wait_for_retry_or_scan_interval();
                }
                Ok(Discovery::Initial(info)) => {
                    let state = DiscoveryState::Initial {
                        bus_id: info.bus_id().to_owned(),
                        port_chain: info.port_chain().to_vec(),
                        vendor_id: info.vendor_id(),
                        product_id: info.product_id(),
                    };
                    if should_wait_after_initial_failure(failed_initial_state.as_ref(), &state) {
                        let retry_requested = self.wait_for_retry_or_scan_interval();
                        if retry_requested {
                            failed_initial_state = None;
                        }
                        continue;
                    }
                    if discovery_state_changed(last_discovery_state.as_ref(), &state) {
                        info!(
                            bus = ?info.bus_id(),
                            port_chain = ?info.port_chain(),
                            vid = format_args!("{:04X}", info.vendor_id()),
                            pid = format_args!("{:04X}", info.product_id()),
                            "USBAccessory scan found the initial accessory-compatible candidate"
                        );
                        last_discovery_state = Some(state.clone());
                    }
                    if should_report_authorizing(scan_is_initial) {
                        self.status.publish_with_device(
                            "authorizing",
                            "已发现唯一 USB 配件，正在等待授权并协商",
                            Some(usb_device_info(&info, None)),
                        );
                    }
                    match negotiate(*info) {
                        Ok(accessory) => {
                            failed_initial_state = None;
                            info!(
                                bus = ?accessory.bus_id(),
                                port_chain = ?accessory.port_chain(),
                                vid = format_args!("{:04X}", accessory.vendor_id()),
                                pid = format_args!("{:04X}", accessory.product_id()),
                                "USBAccessory negotiation completed; opening the re-enumerated accessory function"
                            );
                            self.run_candidate(accessory)
                        }
                        Err(error) => {
                            warn!(error = %error, "USBAccessory negotiation stopped");
                            self.status.publish("error", error.to_string());
                            failed_initial_state = Some(state);
                            info!(
                                "USBAccessory negotiation returned to the initial-device scan; no nusb handle is retained"
                            );
                            if self.wait_for_retry_or_scan_interval() {
                                failed_initial_state = None;
                            }
                        }
                    }
                }
                Ok(Discovery::Direct(info)) => {
                    failed_initial_state = None;
                    let state = DiscoveryState::Direct {
                        bus_id: info.bus_id().to_owned(),
                        port_chain: info.port_chain().to_vec(),
                        vendor_id: info.vendor_id(),
                        product_id: info.product_id(),
                    };
                    if discovery_state_changed(last_discovery_state.as_ref(), &state) {
                        info!(
                            bus = ?info.bus_id(),
                            port_chain = ?info.port_chain(),
                            vid = format_args!("{:04X}", info.vendor_id()),
                            pid = format_args!("{:04X}", info.product_id()),
                            "USBAccessory scan found a unique descriptor-selected accessory function; entering Bulk session without endpoint-zero negotiation"
                        );
                        last_discovery_state = Some(state);
                    }
                    self.status.publish_with_device(
                        "authorizing",
                        "已发现唯一 USB 配件，正在等待平板授权",
                        Some(usb_device_info(&info, None)),
                    );
                    self.run_candidate(*info);
                }
                Err(error) => {
                    warn!(error = %error, "USBAccessory discovery stopped");
                    self.status.publish("error", error.to_string());
                    info!(
                        "USBAccessory discovery error is recoverable; the service remains in its scan loop"
                    );
                    self.wait_for_retry_or_scan_interval();
                }
            }
        }
    }

    fn wait_for_retry_or_scan_interval(&self) -> bool {
        let deadline = Instant::now() + SCAN_INTERVAL;
        while Instant::now() < deadline {
            if self.control.take_retry_requested() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn run_candidate(&self, info: DeviceInfo) {
        let connection_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
        self.control.active.store(connection_id, Ordering::Release);
        info!(
            connection_id,
            vid = format_args!("{:04X}", info.vendor_id()),
            pid = format_args!("{:04X}", info.product_id()),
            bus = ?info.bus_id(),
            port_chain = ?info.port_chain(),
            "opening descriptor-selected USBAccessory Bulk session"
        );
        self.status.publish_with_device(
            "authorizing",
            "正在等待平板授权并打开 USB 会话",
            Some(usb_device_info(&info, None)),
        );
        let session_failed = if let Err(error) = self.run_session(info, connection_id) {
            if !matches!(error, UsbError::Cancelled) {
                warn!(connection_id, error = %error, "formal USB session ended with an I/O/protocol failure");
                self.status.publish("error", error.to_string());
                true
            } else {
                info!(
                    connection_id,
                    "USB session was cancelled by the PC control surface"
                );
                false
            }
        } else {
            false
        };
        info!(
            connection_id,
            "USB session I/O returned; all session-local nusb Device/Interface/Endpoint handles have been dropped"
        );
        if let Err(error) = self.lifecycle.release_usb_connection(connection_id) {
            warn!(connection_id, error = %error, "failed to clean up USB session state");
        }
        self.control.active.store(0, Ordering::Release);
        if !session_failed {
            self.status
                .publish("waiting", "USB 会话已清理，等待 AirSlate 平板");
        }
        info!(
            connection_id,
            "USB session cleanup complete; next service-loop iteration will enumerate the initial accessory-compatible interface"
        );
        self.wait_for_retry_or_scan_interval();
    }

    fn run_session(&self, info: DeviceInfo, connection_id: u64) -> Result<(), UsbError> {
        let identity = physical_usb_identity(&info)?;
        let mut ready_retries = 0;
        let OpenBulkSession {
            output,
            mut input,
            endpoints,
        } = loop {
            let mut opened = open_bulk_session(&info, connection_id)?;
            let descriptor = usb_device_info(&info, Some(&opened.endpoints));
            self.status.publish_with_device(
                "authorizing",
                "正在等待平板授权并发送 USB_READY",
                Some(descriptor),
            );
            match send_usb_ready(&mut opened.output, connection_id, &self.control) {
                Ok(()) => break opened,
                Err(UsbError::ReadySubmitDisconnected) => {
                    if self.control.is_cancelled(connection_id) {
                        return Err(UsbError::Cancelled);
                    }
                    if !same_physical_device_present(&identity)? {
                        return Err(UsbError::Disconnected);
                    }
                    if !ready_retry_allowed(ready_retries, true) {
                        return Err(UsbError::ReadySubmitExhausted {
                            attempts: ready_retries,
                        });
                    }
                    ready_retries = ready_retries.saturating_add(1);
                    warn!(
                        connection_id,
                        retry = ready_retries,
                        retry_limit = READY_SUBMIT_RETRY_LIMIT,
                        "WinUSB rejected USB_READY submit as disconnected (nusb 0.2.4 ERROR_BAD_COMMAND); exact accessory LocationPath is still enumerated, reopening Bulk endpoints"
                    );
                    drop(opened);
                    std::thread::sleep(READY_SUBMIT_RETRY_DELAY);
                }
                Err(error) => return Err(error),
            }
        };
        self.status.publish_with_device(
            "authorizing",
            "USB_READY 已送达，等待平板授权并发送正式握手",
            Some(usb_device_info(&info, Some(&endpoints))),
        );
        let mut writer = output.writer(endpoints.out_max_packet_size);
        let mut packets = PacketStream::default();
        let request =
            read_handshake_packet(&mut input, &mut packets, connection_id, &self.control)?;
        if request.len() != HANDSHAKE_REQUEST_SIZE
            || request.get(4) != Some(&(PacketType::HandshakeRequest as u8))
        {
            return Err(UsbError::Protocol(
                "first USB packet is not a 72-byte HANDSHAKE_REQUEST".to_owned(),
            ));
        }
        self.status.publish_with_device(
            "handshaking",
            "已收到平板握手请求，正在建立有线会话",
            Some(usb_device_info(&info, Some(&endpoints))),
        );
        info!(
            connection_id,
            packet_len = request.len(),
            "pre-handshake HANDSHAKE_REQUEST parsed; dispatching formal handshake"
        );
        let response = self
            .handshake
            .handle_usb_request_bytes(&request, connection_id)
            .map_err(|error| UsbError::Protocol(error.to_string()))?;
        info!(
            connection_id,
            response_len = response.len(),
            "formal handshake response prepared"
        );
        info!(
            connection_id,
            "writing formal handshake response to Bulk OUT"
        );
        writer.write_all(&response).map_err(UsbError::Write)?;
        info!(
            connection_id,
            "formal handshake response buffered; flushing Bulk OUT"
        );
        writer.flush().map_err(UsbError::Write)?;
        info!(connection_id, "formal handshake response write completed");
        if !matches!(decode_packet(&response), Ok(Packet::HandshakeResponse(_))) {
            return Err(UsbError::HandshakeRejected);
        }
        self.status.publish_with_device(
            "connected",
            "AirSlate 正式 USB 会话已连接",
            Some(usb_device_info(&info, Some(&endpoints))),
        );
        info!(connection_id, "formal USB session established");
        let mut reader = input.reader(IO_BUFFER_SIZE).with_read_timeout(READ_TIMEOUT);

        loop {
            let bytes = read_next_packet(&mut reader, &mut packets, connection_id, &self.control)?;
            let packet =
                decode_packet(&bytes).map_err(|error| UsbError::Protocol(error.to_string()))?;
            match packet {
                Packet::StylusFrame(frame) => {
                    let session_id = self.accept_frame(connection_id)?;
                    self.lifecycle
                        .emit_incoming(IncomingEvent::UsbStylus { session_id, frame });
                }
                Packet::GestureFrame(frame) => {
                    let session_id = self.accept_frame(connection_id)?;
                    self.lifecycle
                        .emit_incoming(IncomingEvent::UsbGesture { session_id, frame });
                }
                Packet::SessionDisconnect(packet) => {
                    return match self
                        .lifecycle
                        .handle_usb_disconnect(connection_id, &packet)
                        .map_err(|error| UsbError::Protocol(error.to_string()))?
                    {
                        LocalDisconnectDisposition::Released { .. } => Ok(()),
                        LocalDisconnectDisposition::AlreadyInactive => Err(UsbError::Protocol(
                            "SESSION_DISCONNECT does not match the active USB session".to_owned(),
                        )),
                    };
                }
                _ => {
                    return Err(UsbError::Protocol(
                        "handshake packet received after the USB session became active".to_owned(),
                    ));
                }
            }
        }
    }

    fn accept_frame(&self, connection_id: u64) -> Result<String, UsbError> {
        match self
            .lifecycle
            .accept_usb_realtime_source(connection_id)
            .map_err(|error| UsbError::Protocol(error.to_string()))?
        {
            RealtimeFrameDisposition::Accepted { session_id } => Ok(session_id),
            disposition => Err(UsbError::Protocol(format!(
                "USB realtime frame rejected by the formal session: {disposition:?}"
            ))),
        }
    }
}

fn waiting_status(
    visible_devices: usize,
    initial_candidates: usize,
    summaries: &[VisibleDeviceSummary],
) -> (&'static str, String) {
    if visible_devices == 0 {
        return ("waiting", "等待 AirSlate 平板 USB 连接".to_owned());
    }
    if initial_candidates == 0 {
        if summaries.iter().any(is_known_file_transfer_mode) {
            return (
                "waiting_accessory",
                "已识别平板，但当前为文件传输模式，Windows 未提供可进行配件协商的接口；请在平板 AirSlate 的有线 USB 页发起连接并授权，完成后点击重试"
                    .to_owned(),
            );
        }
        return (
            "waiting",
            format!("已枚举 {visible_devices} 个 USB 设备，未发现可用的 AirSlate 配件接口"),
        );
    }
    (
        "waiting",
        format!("已枚举 {visible_devices} 个 USB 设备，等待配件会话"),
    )
}

fn should_report_authorizing(scan_is_initial: bool) -> bool {
    !scan_is_initial
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsbTransferPhase {
    SendingReady,
    AwaitingHandshake,
    ActiveSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferErrorAction {
    Retry,
    ClearHaltAndRetry,
    Fail,
}

fn transfer_error_action(phase: UsbTransferPhase, error: TransferError) -> TransferErrorAction {
    match (phase, error) {
        (
            UsbTransferPhase::SendingReady | UsbTransferPhase::AwaitingHandshake,
            TransferError::Cancelled,
        ) => TransferErrorAction::Retry,
        (
            UsbTransferPhase::SendingReady | UsbTransferPhase::AwaitingHandshake,
            TransferError::Stall,
        ) => TransferErrorAction::ClearHaltAndRetry,
        _ => TransferErrorAction::Fail,
    }
}

fn active_session_read_error(error: std::io::Error) -> UsbError {
    if error.kind() == std::io::ErrorKind::ConnectionReset {
        debug_assert_eq!(
            transfer_error_action(UsbTransferPhase::ActiveSession, TransferError::Stall),
            TransferErrorAction::Fail
        );
    }
    UsbError::Read(error)
}

#[derive(Debug, Default)]
struct HandshakeReadDiagnostics {
    consecutive_timeouts: u32,
    total_timeouts: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandshakeTimeoutReport {
    consecutive: u32,
    total: u64,
}

impl HandshakeReadDiagnostics {
    fn note_timeout(&mut self) -> Option<HandshakeTimeoutReport> {
        self.consecutive_timeouts = self.consecutive_timeouts.saturating_add(1);
        self.total_timeouts = self.total_timeouts.saturating_add(1);
        if self.consecutive_timeouts == 1
            || self
                .consecutive_timeouts
                .is_multiple_of(HANDSHAKE_TIMEOUT_REPORT_INTERVAL)
        {
            Some(HandshakeTimeoutReport {
                consecutive: self.consecutive_timeouts,
                total: self.total_timeouts,
            })
        } else {
            None
        }
    }

    fn finish_timeout_run(&mut self) -> u32 {
        std::mem::take(&mut self.consecutive_timeouts)
    }
}

fn advance_usb_ready(sent: usize, actual_len: usize) -> Result<usize, UsbError> {
    let remaining = USB_READY.len().saturating_sub(sent);
    if sent > USB_READY.len() || actual_len > remaining {
        return Err(UsbError::Protocol(format!(
            "invalid USB_READY write completion: sent={sent}, actual_len={actual_len}"
        )));
    }
    Ok(sent + actual_len)
}

fn ready_disconnected_submit_can_reopen(sent: usize, actual_len: usize) -> bool {
    sent == 0 && actual_len == 0
}

fn send_usb_ready(
    output: &mut Endpoint<Bulk, Out>,
    connection_id: u64,
    control: &UsbSessionControl,
) -> Result<(), UsbError> {
    let mut sent = 0;
    while sent < USB_READY.len() {
        if control.is_cancelled(connection_id) {
            return Err(UsbError::Cancelled);
        }

        let completion = output.transfer_blocking(USB_READY[sent..].to_vec().into(), READ_TIMEOUT);
        sent = advance_usb_ready(sent, completion.actual_len)?;
        if sent == USB_READY.len() {
            info!(
                connection_id,
                endpoint = format_args!("0x{:02X}", output.endpoint_address()),
                "delivered USB_READY before formal handshake"
            );
            return Ok(());
        }

        match completion.status {
            Ok(()) => std::thread::sleep(STARTUP_RETRY_DELAY),
            Err(error) => match transfer_error_action(UsbTransferPhase::SendingReady, error) {
                TransferErrorAction::Retry => std::thread::sleep(STARTUP_RETRY_DELAY),
                TransferErrorAction::ClearHaltAndRetry => {
                    // transfer_blocking has completed/cancelled and consumed its one transfer,
                    // so nusb's no-pending-transfer precondition for clear_halt holds here.
                    output.clear_halt().wait().map_err(|error| UsbError::Usb {
                        operation: "resetting a stalled Bulk OUT pipe while sending USB_READY",
                        detail: error.to_string(),
                    })?;
                    info!(
                        connection_id,
                        endpoint = format_args!("0x{:02X}", output.endpoint_address()),
                        "recovered pre-handshake Bulk OUT STALL with nusb clear_halt/WinUsb_ResetPipe"
                    );
                    std::thread::sleep(STARTUP_RETRY_DELAY);
                }
                TransferErrorAction::Fail => {
                    if matches!(error, TransferError::Disconnected)
                        && ready_disconnected_submit_can_reopen(sent, completion.actual_len)
                    {
                        return Err(UsbError::ReadySubmitDisconnected);
                    }
                    return Err(UsbError::ReadyWrite(error));
                }
            },
        }
    }
    Ok(())
}

fn read_handshake_packet(
    input: &mut Endpoint<Bulk, In>,
    packets: &mut PacketStream,
    connection_id: u64,
    control: &UsbSessionControl,
) -> Result<Vec<u8>, UsbError> {
    let mut diagnostics = HandshakeReadDiagnostics::default();
    loop {
        if control.is_cancelled(connection_id) {
            return Err(UsbError::Cancelled);
        }
        if let Some(packet) = next_buffered_packet(packets)? {
            return Ok(packet);
        }

        let buffer = input.allocate(IO_BUFFER_SIZE);
        let completion = input.transfer_blocking(buffer, READ_TIMEOUT);
        let actual_len = completion.actual_len;
        let packet = push_completion_and_take_packet(&completion, packets)?;
        if actual_len > 0 {
            let timeouts_before_data = diagnostics.finish_timeout_run();
            info!(
                connection_id,
                endpoint = format_args!("0x{:02X}", input.endpoint_address()),
                actual_len,
                buffered_len = packets.buffered_len(),
                timeouts_before_data,
                "received pre-handshake Bulk IN bytes"
            );
        }
        // nusb documents that a completion may contain transferred bytes even when its
        // status is an error. Once those bytes form a complete packet, consume that packet
        // before attempting clear_halt; otherwise a full request paired with a STALL could
        // block in pipe recovery despite already being ready for the handshake parser.
        if let Some(packet) = packet {
            info!(
                connection_id,
                packet_len = packet.len(),
                "pre-handshake packet became complete in this Bulk IN completion"
            );
            return Ok(packet);
        }
        let Err(error) = completion.status else {
            continue;
        };
        match transfer_error_action(UsbTransferPhase::AwaitingHandshake, error) {
            TransferErrorAction::Retry => {
                if let Some(report) = diagnostics.note_timeout() {
                    info!(
                        connection_id,
                        endpoint = format_args!("0x{:02X}", input.endpoint_address()),
                        consecutive_timeouts = report.consecutive,
                        total_timeouts = report.total,
                        buffered_len = packets.buffered_len(),
                        "still waiting for pre-handshake Bulk IN bytes after timeout"
                    );
                }
            }
            TransferErrorAction::ClearHaltAndRetry => {
                let timeouts_before_stall = diagnostics.finish_timeout_run();
                info!(
                    connection_id,
                    endpoint = format_args!("0x{:02X}", input.endpoint_address()),
                    timeouts_before_stall,
                    buffered_len = packets.buffered_len(),
                    "pre-handshake Bulk IN STALL; clearing halt"
                );
                input.clear_halt().wait().map_err(|error| UsbError::Usb {
                    operation: "resetting a stalled Bulk IN pipe before formal handshake",
                    detail: error.to_string(),
                })?;
                info!(
                    connection_id,
                    endpoint = format_args!("0x{:02X}", input.endpoint_address()),
                    "recovered pre-handshake Bulk IN STALL with nusb clear_halt/WinUsb_ResetPipe"
                );
            }
            TransferErrorAction::Fail => return Err(UsbError::HandshakeRead(error)),
        }
    }
}

fn next_buffered_packet(packets: &mut PacketStream) -> Result<Option<Vec<u8>>, UsbError> {
    packets
        .next_packet()
        .map_err(|error| UsbError::Protocol(error.to_string()))
}

fn push_completion_and_take_packet(
    completion: &Completion,
    packets: &mut PacketStream,
) -> Result<Option<Vec<u8>>, UsbError> {
    if completion.actual_len > 0 {
        let bytes = completion
            .buffer
            .get(..completion.actual_len)
            .ok_or_else(|| {
                UsbError::Protocol(format!(
                    "Bulk IN completion length {} exceeds its buffer length {}",
                    completion.actual_len,
                    completion.buffer.len()
                ))
            })?;
        packets.push(bytes);
    }
    next_buffered_packet(packets)
}

fn read_next_packet<R: Read>(
    reader: &mut R,
    packets: &mut PacketStream,
    connection_id: u64,
    control: &UsbSessionControl,
) -> Result<Vec<u8>, UsbError> {
    let mut buffer = [0_u8; IO_BUFFER_SIZE];
    loop {
        if control.is_cancelled(connection_id) {
            return Err(UsbError::Cancelled);
        }
        if let Some(packet) = packets
            .next_packet()
            .map_err(|error| UsbError::Protocol(error.to_string()))?
        {
            return Ok(packet);
        }
        match reader.read(&mut buffer) {
            Ok(0) => return Err(UsbError::Disconnected),
            Ok(read) => packets.push(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(error) => return Err(active_session_read_error(error)),
        }
    }
}

enum Discovery {
    None {
        visible_devices: usize,
        initial_candidates: usize,
        summaries: Vec<VisibleDeviceSummary>,
    },
    Initial(Box<DeviceInfo>),
    Direct(Box<DeviceInfo>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisibleDeviceSummary {
    bus_id: String,
    port_chain: Vec<u8>,
    vendor_id: u16,
    product_id: u16,
    driver: Option<String>,
    interfaces: Vec<(u8, u8, u8, u8)>,
}

fn is_known_file_transfer_mode(summary: &VisibleDeviceSummary) -> bool {
    // This is a UI diagnosis only. It never enters candidate discovery or opens
    // the MTP Bulk endpoints as a session transport.
    summary.vendor_id == 0x12D1
        && summary.product_id == 0x1101
        && summary
            .interfaces
            .iter()
            .any(|(_, class, subclass, protocol)| {
                (*class, *subclass, *protocol) == (0x06, 0x01, 0x01)
            })
}

fn device_driver(info: &DeviceInfo) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        info.driver().map(str::to_owned)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = info;
        None
    }
}

fn nusb_backend_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "WinUSB"
    }
    #[cfg(target_os = "macos")]
    {
        "IOKit"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "nusb"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiscoveryState {
    None {
        visible_devices: usize,
        initial_candidates: usize,
        summaries: Vec<VisibleDeviceSummary>,
    },
    Initial {
        bus_id: String,
        port_chain: Vec<u8>,
        vendor_id: u16,
        product_id: u16,
    },
    Direct {
        bus_id: String,
        port_chain: Vec<u8>,
        vendor_id: u16,
        product_id: u16,
    },
}

impl DiscoveryState {
    fn summaries(&self) -> &[VisibleDeviceSummary] {
        match self {
            Self::None { summaries, .. } => summaries,
            Self::Initial { .. } | Self::Direct { .. } => &[],
        }
    }
}

fn discovery_state_changed(previous: Option<&DiscoveryState>, current: &DiscoveryState) -> bool {
    previous != Some(current)
}

fn should_wait_after_initial_failure(
    failed_state: Option<&DiscoveryState>,
    current: &DiscoveryState,
) -> bool {
    matches!(current, DiscoveryState::Initial { .. }) && failed_state == Some(current)
}

fn discover_candidate() -> Result<Discovery, UsbError> {
    let devices = nusb::list_devices()
        .wait()
        .map_err(|error| UsbError::Usb {
            operation: "enumerating USB devices",
            detail: error.to_string(),
        })?
        .collect::<Vec<_>>();
    let visible_devices = devices.len();
    let summaries = devices
        .iter()
        .map(|info| VisibleDeviceSummary {
            bus_id: info.bus_id().to_owned(),
            port_chain: info.port_chain().to_vec(),
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            driver: device_driver(info),
            interfaces: info
                .interfaces()
                .map(|interface| {
                    (
                        interface.interface_number(),
                        interface.class(),
                        interface.subclass(),
                        interface.protocol(),
                    )
                })
                .collect(),
        })
        .collect();
    let mut initial = Vec::new();
    let mut accessory_like = Vec::new();
    for info in devices {
        if has_interface(&info, 0xFF, 0x50, 0x01) {
            initial.push(info);
        } else if has_interface(&info, 0xFF, 0xFF, 0x00) {
            accessory_like.push(info);
        }
    }
    match initial.as_slice() {
        [candidate] => Ok(Discovery::Initial(Box::new(candidate.clone()))),
        [] if can_enter_direct_bulk_recovery(initial.len(), accessory_like.len()) => {
            let candidate = &accessory_like[0];
            validate_recovery_bulk_pair(candidate)?;
            Ok(Discovery::Direct(Box::new(candidate.clone())))
        }
        [] if accessory_like.len() > 1 => Err(UsbError::AmbiguousAccessory(accessory_like.len())),
        [] => Ok(Discovery::None {
            visible_devices,
            initial_candidates: 0,
            summaries,
        }),
        _ => Err(UsbError::AmbiguousInitial(initial.len())),
    }
}

fn validate_recovery_bulk_pair(info: &DeviceInfo) -> Result<(), UsbError> {
    let device = info
        .open()
        .wait()
        .map_err(|error| UsbError::RecoveryDescriptor {
            detail: format!("opening the unique descriptor-selected candidate: {error}"),
        })?;
    select_bulk_pair(&device)
        .map(|_| ())
        .map_err(|error| UsbError::RecoveryDescriptor {
            detail: format!(
                "descriptor validation rejected the unique candidate before a Bulk session: {error}"
            ),
        })
}

fn has_interface(info: &DeviceInfo, class: u8, subclass: u8, protocol: u8) -> bool {
    info.interfaces().any(|interface| {
        interface.class() == class
            && interface.subclass() == subclass
            && interface.protocol() == protocol
    })
}

fn can_enter_direct_bulk_recovery(initial_candidates: usize, accessory_candidates: usize) -> bool {
    initial_candidates == 0 && accessory_candidates == 1
}

fn negotiate(info: DeviceInfo) -> Result<DeviceInfo, UsbError> {
    let bus_id = info.bus_id().to_owned();
    let port_chain = info.port_chain().to_vec();
    #[cfg(windows)]
    let location_path = winusb_tool::windows_app::find_present_usb_location(
        info.vendor_id(),
        info.product_id(),
        info.serial_number(),
    )
    .map_err(UsbError::Driver)?;
    let interface_number = info
        .interfaces()
        .find(|interface| {
            interface.class() == 0xFF
                && interface.subclass() == 0x50
                && interface.protocol() == 0x01
        })
        .ok_or_else(|| UsbError::Protocol("pre-negotiation device has no interface".to_owned()))?
        .interface_number();
    let device = info.open().wait().map_err(|error| UsbError::Usb {
        operation: "opening the unique initial accessory-compatible candidate for endpoint-zero negotiation",
        detail: error.to_string(),
    })?;
    let interface = device
        .claim_interface(interface_number)
        .wait()
        .map_err(|error| UsbError::Usb {
            operation: "claiming the selected Harmony interface for endpoint-zero negotiation",
            detail: error.to_string(),
        })?;
    let version = interface
        .control_in(
            ControlIn {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request: ACCESSORY_GET_PROTOCOL,
                value: 0,
                index: 0,
                length: 2,
            },
            CONTROL_TIMEOUT,
        )
        .wait()
        .map_err(|error| UsbError::Control("GET_PROTOCOL", error.to_string()))?;
    if version.len() != 2 || u16::from_le_bytes([version[0], version[1]]) == 0 {
        return Err(UsbError::Protocol(format!(
            "GET_PROTOCOL returned invalid bytes {version:02X?}"
        )));
    }
    for (index, value) in ACCESSORY_IDENTITY {
        let mut data = value.as_bytes().to_vec();
        data.push(0);
        interface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request: ACCESSORY_SEND_STRING,
                    value: 0,
                    index,
                    data: &data,
                },
                CONTROL_TIMEOUT,
            )
            .wait()
            .map_err(|error| UsbError::Control("SEND_STRING", error.to_string()))?;
    }
    interface
        .control_out(
            ControlOut {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request: ACCESSORY_START,
                value: 0,
                index: 0,
                data: &[],
            },
            CONTROL_TIMEOUT,
        )
        .wait()
        .map_err(|error| UsbError::Control("START_ACCESSORY", error.to_string()))?;
    drop(interface);
    drop(device);

    #[cfg(windows)]
    let mut started = Instant::now();
    #[cfg(not(windows))]
    let started = Instant::now();
    #[cfg(windows)]
    let mut driver_install_requested = false;
    while started.elapsed() < REENUMERATION_TIMEOUT {
        let devices = nusb::list_devices().wait().map_err(|error| UsbError::Usb {
            operation: "waiting for physical-port re-enumeration",
            detail: error.to_string(),
        })?;
        let candidates = devices
            .filter(|candidate| {
                candidate.bus_id() == bus_id
                    && candidate.port_chain() == port_chain
                    && has_interface(candidate, 0xFF, 0xFF, 0x00)
            })
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [candidate] => {
                #[cfg(windows)]
                {
                    let candidate_location = winusb_tool::windows_app::find_present_usb_location(
                        candidate.vendor_id(),
                        candidate.product_id(),
                        candidate.serial_number(),
                    )
                    .map_err(UsbError::Driver)?;
                    if !candidate_location.eq_ignore_ascii_case(&location_path) {
                        return Err(UsbError::Driver(format!(
                            "re-enumerated WinUSB device LocationPath changed from {location_path:?} to {candidate_location:?}"
                        )));
                    }
                }
                return Ok(candidate.clone());
            }
            [] => {
                #[cfg(windows)]
                if !driver_install_requested
                    && let Some(instance_id) =
                        winusb_tool::windows_app::code28_instance_at_location(&location_path)
                            .map_err(UsbError::Driver)?
                {
                    install_winusb_elevated(&location_path, &instance_id)?;
                    driver_install_requested = true;
                    started = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => return Err(UsbError::AmbiguousReenumeration(candidates.len())),
        }
    }
    Err(UsbError::ReenumerationTimeout)
}

#[derive(Clone, Copy)]
struct BulkPair {
    configuration: u8,
    interface: u8,
    alternate: u8,
    in_address: u8,
    out_address: u8,
    in_max_packet_size: usize,
    out_max_packet_size: usize,
}

fn usb_device_info(info: &DeviceInfo, pair: Option<&BulkPair>) -> UsbDeviceInfo {
    let interface_number = info
        .interfaces()
        .find(|interface| {
            (
                interface.class(),
                interface.subclass(),
                interface.protocol(),
            ) == (0xFF, 0x50, 0x01)
                || (
                    interface.class(),
                    interface.subclass(),
                    interface.protocol(),
                ) == (0xFF, 0xFF, 0x00)
        })
        .map(|interface| interface.interface_number());

    UsbDeviceInfo {
        vendor_id: info.vendor_id(),
        product_id: info.product_id(),
        bus_id: info.bus_id().to_owned(),
        port_chain: info.port_chain().to_vec(),
        configuration: pair.map(|value| value.configuration),
        interface_number: pair.map(|value| value.interface).or(interface_number),
        alternate_setting: pair.map(|value| value.alternate),
        bulk_in_endpoint: pair.map(|value| value.in_address),
        bulk_out_endpoint: pair.map(|value| value.out_address),
        bulk_in_max_packet_size: pair.map(|value| value.in_max_packet_size),
        bulk_out_max_packet_size: pair.map(|value| value.out_max_packet_size),
    }
}

struct OpenBulkSession {
    output: Endpoint<Bulk, Out>,
    input: Endpoint<Bulk, In>,
    endpoints: BulkPair,
}

fn open_bulk_session(info: &DeviceInfo, connection_id: u64) -> Result<OpenBulkSession, UsbError> {
    let device = info.open().wait().map_err(|error| UsbError::Usb {
        operation: "opening the re-enumerated device (WinUSB/permission required)",
        detail: error.to_string(),
    })?;
    let endpoints = select_bulk_pair(&device)?;
    info!(
        connection_id,
        configuration = endpoints.configuration,
        interface = endpoints.interface,
        alternate = endpoints.alternate,
        bulk_in = format_args!("0x{:02X}", endpoints.in_address),
        bulk_out = format_args!("0x{:02X}", endpoints.out_address),
        in_max_packet = endpoints.in_max_packet_size,
        out_max_packet = endpoints.out_max_packet_size,
        "selected USB descriptors"
    );
    let interface = device
        .claim_interface(endpoints.interface)
        .wait()
        .map_err(|error| UsbError::Usb {
            operation: "claiming the descriptor-selected WinUSB interface",
            detail: error.to_string(),
        })?;
    if endpoints.alternate != 0 {
        interface
            .set_alt_setting(endpoints.alternate)
            .wait()
            .map_err(|error| UsbError::Usb {
                operation: "selecting the descriptor-selected alternate setting",
                detail: error.to_string(),
            })?;
    }
    let output = interface
        .endpoint::<Bulk, Out>(endpoints.out_address)
        .map_err(|_| UsbError::Endpoint(endpoints.out_address))?;
    let input = interface
        .endpoint::<Bulk, In>(endpoints.in_address)
        .map_err(|_| UsbError::Endpoint(endpoints.in_address))?;

    Ok(OpenBulkSession {
        output,
        input,
        endpoints,
    })
}

#[derive(Debug, Clone)]
struct PhysicalUsbIdentity {
    bus_id: String,
    port_chain: Vec<u8>,
    #[cfg(windows)]
    location_path: String,
}

fn physical_usb_identity(info: &DeviceInfo) -> Result<PhysicalUsbIdentity, UsbError> {
    Ok(PhysicalUsbIdentity {
        bus_id: info.bus_id().to_owned(),
        port_chain: info.port_chain().to_vec(),
        #[cfg(windows)]
        location_path: winusb_tool::windows_app::find_present_usb_location(
            info.vendor_id(),
            info.product_id(),
            info.serial_number(),
        )
        .map_err(UsbError::Driver)?,
    })
}

fn same_physical_device_present(identity: &PhysicalUsbIdentity) -> Result<bool, UsbError> {
    let devices = nusb::list_devices().wait().map_err(|error| UsbError::Usb {
        operation: "checking the USBAccessory after a pre-handshake WinUSB submit failure",
        detail: error.to_string(),
    })?;
    let candidates = devices
        .filter(|candidate| {
            candidate.bus_id() == identity.bus_id
                && candidate.port_chain() == identity.port_chain
                && has_interface(candidate, 0xFF, 0xFF, 0x00)
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Ok(false),
        [candidate] => {
            #[cfg(windows)]
            {
                let location_path = winusb_tool::windows_app::find_present_usb_location(
                    candidate.vendor_id(),
                    candidate.product_id(),
                    candidate.serial_number(),
                )
                .map_err(UsbError::Driver)?;
                Ok(location_path.eq_ignore_ascii_case(&identity.location_path))
            }
            #[cfg(not(windows))]
            {
                let _ = candidate;
                Ok(true)
            }
        }
        _ => Err(UsbError::AmbiguousReenumeration(candidates.len())),
    }
}

fn ready_retry_allowed(retries: u8, same_physical_device: bool) -> bool {
    same_physical_device && retries < READY_SUBMIT_RETRY_LIMIT
}

fn select_bulk_pair(device: &nusb::Device) -> Result<BulkPair, UsbError> {
    let configuration = device
        .active_configuration()
        .map_err(|error| UsbError::Protocol(error.to_string()))?;
    let mut pairs = Vec::new();
    for interface in configuration.interfaces() {
        for alternate in interface.alt_settings() {
            let endpoints = alternate.endpoints().collect::<Vec<_>>();
            for input in endpoints.iter().filter(|endpoint| {
                endpoint.transfer_type() == TransferType::Bulk
                    && endpoint.direction() == nusb::transfer::Direction::In
            }) {
                for output in endpoints.iter().filter(|endpoint| {
                    endpoint.transfer_type() == TransferType::Bulk
                        && endpoint.direction() == nusb::transfer::Direction::Out
                }) {
                    pairs.push(BulkPair {
                        configuration: configuration.configuration_value(),
                        interface: alternate.interface_number(),
                        alternate: alternate.alternate_setting(),
                        in_address: input.address(),
                        out_address: output.address(),
                        in_max_packet_size: input.max_packet_size(),
                        out_max_packet_size: output.max_packet_size(),
                    });
                }
            }
        }
    }
    match pairs.as_slice() {
        [pair] => Ok(*pair),
        [] => Err(UsbError::NoBulkPair),
        _ => Err(UsbError::AmbiguousBulkPair(pairs.len())),
    }
}

#[derive(Debug, thiserror::Error)]
enum UsbError {
    #[error(
        "{operation}: {detail}; Windows requires Microsoft inbox WinUSB on the exact re-enumerated LocationPath"
    )]
    Usb {
        operation: &'static str,
        detail: String,
    },
    #[error("Accessory control request {0} failed: {1}")]
    Control(&'static str, String),
    #[error(
        "{0} initial accessory-compatible candidates are present; refusing to send vendor requests without a unique target"
    )]
    AmbiguousInitial(usize),
    #[error(
        "{0} descriptor-selected accessory candidates are present without an initial interface; refusing to guess a Bulk session"
    )]
    AmbiguousAccessory(usize),
    #[error("{0} devices matched the same physical port after re-enumeration")]
    AmbiguousReenumeration(usize),
    #[error(
        "Accessory START succeeded but the WinUSB-bound function did not appear on the same physical port within 15 seconds; inspect Code 28/LocationPath with winusb_inbox_dry_run"
    )]
    ReenumerationTimeout,
    #[error("the active configuration has no Bulk IN/OUT pair")]
    NoBulkPair,
    #[error("the active configuration has {0} Bulk IN/OUT pairs; refusing to guess")]
    AmbiguousBulkPair(usize),
    #[error(
        "unique descriptor-selected accessory cannot be used for raw Bulk session recovery: {detail}"
    )]
    RecoveryDescriptor { detail: String },
    #[error("descriptor-selected endpoint 0x{0:02X} could not be opened")]
    Endpoint(u8),
    #[error("Bulk IN failed (timeout is retried; this is unplug/EOF/WinUSB I/O): {0}")]
    Read(std::io::Error),
    #[error("Bulk OUT short-write/write/flush failed: {0}")]
    Write(std::io::Error),
    #[error("USB_READY Bulk OUT failed before formal handshake: {0}")]
    ReadyWrite(TransferError),
    #[error(
        "USB_READY Bulk OUT submit was rejected by WinUSB as disconnected while the transfer had not started"
    )]
    ReadySubmitDisconnected,
    #[error(
        "USB_READY Bulk OUT submit kept returning nusb Disconnected after {attempts} bounded retries while the same physical accessory remained enumerated; inspect WinUSB pipe/driver state"
    )]
    ReadySubmitExhausted { attempts: u8 },
    #[error("Bulk IN returned EOF; the device was unplugged or stopped the accessory function")]
    Disconnected,
    #[error("formal USB protocol violation: {0}")]
    Protocol(String),
    #[error("formal handshake response was HANDSHAKE_ERROR; USB session was not created")]
    HandshakeRejected,
    #[error("Bulk IN failed while waiting for HANDSHAKE_REQUEST: {0}")]
    HandshakeRead(TransferError),
    #[error("USB session was closed from the PC UI")]
    Cancelled,
    #[error("Windows inbox WinUSB binding failed: {0}")]
    Driver(String),
}

#[cfg(windows)]
fn install_winusb_elevated(location_path: &str, instance_id: &str) -> Result<(), UsbError> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

    use windows::{
        Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            System::Threading::{GetExitCodeProcess, WaitForSingleObject},
            UI::{
                Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
                WindowsAndMessaging::SW_HIDE,
            },
        },
        core::PCWSTR,
    };

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }
    fn quote(value: &str) -> Result<String, UsbError> {
        if value.contains('"') {
            return Err(UsbError::Driver(
                "driver helper argument contains a quote".to_owned(),
            ));
        }
        Ok(format!("\"{value}\""))
    }

    let executable =
        std::env::current_exe().map_err(|error| UsbError::Driver(error.to_string()))?;
    let executable = wide(executable.as_os_str());
    let verb = wide(OsStr::new("runas"));
    let parameters = format!(
        "--airslate-install-winusb --location-path {} --instance-id {}",
        quote(location_path)?,
        quote(instance_id)?
    );
    let parameters = wide(OsStr::new(&parameters));
    let mut execute = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(executable.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut execute) }.map_err(|error| {
        UsbError::Driver(format!("UAC elevation was rejected or failed: {error}"))
    })?;
    if execute.hProcess.is_invalid() {
        return Err(UsbError::Driver(
            "elevated driver helper returned no process handle".to_owned(),
        ));
    }
    let wait = unsafe { WaitForSingleObject(execute.hProcess, 120_000) };
    if wait != WAIT_OBJECT_0 {
        unsafe { CloseHandle(execute.hProcess) }.ok();
        return Err(UsbError::Driver(format!(
            "elevated driver helper wait failed: {wait:?}"
        )));
    }
    let mut exit_code = 1;
    unsafe { GetExitCodeProcess(execute.hProcess, &mut exit_code) }
        .map_err(|error| UsbError::Driver(error.to_string()))?;
    unsafe { CloseHandle(execute.hProcess) }.ok();
    if exit_code != 0 {
        return Err(UsbError::Driver(format!(
            "elevated inbox WinUSB helper exited with code {exit_code}"
        )));
    }
    Ok(())
}

#[cfg(windows)]
pub fn run_driver_helper_if_requested() -> Option<std::process::ExitCode> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) != Some("--airslate-install-winusb") {
        return None;
    }
    let mut location = None;
    let mut instance = None;
    let mut index = 2;
    while index + 1 < args.len() {
        match args[index].as_str() {
            "--location-path" => location = Some(args[index + 1].clone()),
            "--instance-id" => instance = Some(args[index + 1].clone()),
            _ => return Some(std::process::ExitCode::FAILURE),
        }
        index += 2;
    }
    let result = location
        .zip(instance)
        .ok_or_else(|| "missing exact driver target".to_owned())
        .and_then(|(location, instance)| {
            winusb_tool::windows_app::install_confirmed_inbox_winusb(&location, &instance)
        });
    match result {
        Ok(()) => Some(std::process::ExitCode::SUCCESS),
        Err(error) => {
            eprintln!("AirSlate inbox WinUSB helper: {error}");
            Some(std::process::ExitCode::FAILURE)
        }
    }
}

#[cfg(not(windows))]
pub fn run_driver_helper_if_requested() -> Option<std::process::ExitCode> {
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nusb::transfer::TransferError;

    use crate::{
        app::lifecycle::{SessionLifecycle, SessionStatusBus},
        handshake::HandshakeService,
        protocol::{HANDSHAKE_REQUEST_SIZE, Packet, PacketType, decode_packet, encode_packet},
        session::SessionService,
        udp_ingest::{IncomingEvent, IncomingEventSink},
        workspace::{ActiveWorkspace, MonitorId, MonitorInfo, WorkspaceService, WorkspaceSnapshot},
    };

    use super::{
        ACCESSORY_IDENTITY, Completion, DiscoveryState, HANDSHAKE_TIMEOUT_REPORT_INTERVAL,
        HandshakeReadDiagnostics, HandshakeTimeoutReport, PacketStream, READY_SUBMIT_RETRY_LIMIT,
        TransferErrorAction, USB_READY, UsbDeviceInfo, UsbSessionControl, UsbStatusBus,
        UsbStatusEvent, UsbTransferPhase, VisibleDeviceSummary, advance_usb_ready,
        can_enter_direct_bulk_recovery, discovery_state_changed, is_known_file_transfer_mode,
        push_completion_and_take_packet, ready_disconnected_submit_can_reopen, ready_retry_allowed,
        should_report_authorizing, should_wait_after_initial_failure, transfer_error_action,
        waiting_status,
    };

    #[test]
    fn harmony_selection_identity_is_fixed() {
        assert_eq!(ACCESSORY_IDENTITY[0], (0, "AirSlate"));
        assert_eq!(ACCESSORY_IDENTITY[1], (1, "AirSlate PC Server"));
    }

    #[test]
    fn usb_status_snapshot_is_available_before_and_after_ui_subscription() {
        let status = UsbStatusBus::default();
        assert_eq!(status.snapshot(), UsbStatusEvent::default());
        let events = status.subscribe();
        let device = UsbDeviceInfo {
            vendor_id: 0x18D1,
            product_id: 0x2D00,
            bus_id: "bus".to_owned(),
            port_chain: vec![2],
            configuration: Some(1),
            interface_number: Some(0),
            alternate_setting: Some(0),
            bulk_in_endpoint: Some(0x81),
            bulk_out_endpoint: Some(0x01),
            bulk_in_max_packet_size: Some(512),
            bulk_out_max_packet_size: Some(512),
        };
        status.publish_with_device("connected", "formal session", Some(device.clone()));
        let event = events
            .recv()
            .expect("the subscribed UI should receive the transition");
        assert_eq!(event.state, "connected");
        assert_eq!(event.device, Some(device));
        assert!(!event.retryable);
        assert_eq!(status.snapshot(), event);

        status.publish_with_device("connected", "formal session", event.device.clone());
        assert!(
            events.try_recv().is_err(),
            "unchanged facts should not spam the UI"
        );
    }

    #[test]
    fn retry_request_interrupts_scan_wait_and_cancels_active_connection() {
        let control = UsbSessionControl::default();
        control
            .active
            .store(9, std::sync::atomic::Ordering::Release);
        control.request_retry();
        assert!(control.is_cancelled(9));
        assert!(control.take_retry_requested());
        assert!(!control.take_retry_requested());
    }

    #[test]
    fn discovery_state_logs_a_real_reconnect_transition_once() {
        let initial = DiscoveryState::Initial {
            bus_id: "bus".to_owned(),
            port_chain: vec![2],
            vendor_id: 0x1234,
            product_id: 0x5678,
        };
        let absent = DiscoveryState::None {
            visible_devices: 0,
            initial_candidates: 0,
            summaries: Vec::new(),
        };
        assert!(discovery_state_changed(None, &initial));
        assert!(!discovery_state_changed(Some(&initial), &initial));
        assert!(discovery_state_changed(Some(&initial), &absent));
        assert!(!discovery_state_changed(Some(&absent), &absent));
    }

    #[test]
    fn direct_bulk_recovery_requires_one_descriptor_candidate_and_no_initial_candidate() {
        assert!(can_enter_direct_bulk_recovery(0, 1));
        assert!(!can_enter_direct_bulk_recovery(1, 1));
        assert!(!can_enter_direct_bulk_recovery(0, 0));
        assert!(!can_enter_direct_bulk_recovery(0, 2));
    }

    #[test]
    fn waiting_status_guides_the_known_file_transfer_state() {
        let summaries = [VisibleDeviceSummary {
            bus_id: "bus".to_owned(),
            port_chain: vec![2],
            vendor_id: 0x12D1,
            product_id: 0x1101,
            driver: Some("WUDFWpdMtp".to_owned()),
            interfaces: vec![(0, 0x06, 0x01, 0x01)],
        }];

        assert!(is_known_file_transfer_mode(&summaries[0]));
        assert_eq!(
            waiting_status(5, 0, &summaries),
            (
                "waiting_accessory",
                "已识别平板，但当前为文件传输模式，Windows 未提供可进行配件协商的接口；请在平板 AirSlate 的有线 USB 页发起连接并授权，完成后点击重试".to_owned()
            )
        );
    }

    #[test]
    fn waiting_status_does_not_call_other_devices_file_transfer_mode() {
        let summaries = [VisibleDeviceSummary {
            bus_id: "bus".to_owned(),
            port_chain: vec![2],
            vendor_id: 0x12D1,
            product_id: 0x1101,
            driver: Some("WUDFWpdMtp".to_owned()),
            interfaces: vec![(0, 0xFF, 0xFF, 0x00)],
        }];

        assert!(!is_known_file_transfer_mode(&summaries[0]));
        assert_eq!(
            waiting_status(1, 0, &summaries),
            (
                "waiting",
                "已枚举 1 个 USB 设备，未发现可用的 AirSlate 配件接口".to_owned()
            )
        );
    }

    #[test]
    fn waiting_detail_requires_the_known_tablet_identity() {
        let summaries = [VisibleDeviceSummary {
            bus_id: "bus".to_owned(),
            port_chain: vec![2],
            vendor_id: 0x18D1,
            product_id: 0x1101,
            driver: Some("WUDFWpdMtp".to_owned()),
            interfaces: vec![(0, 0x06, 0x01, 0x01)],
        }];

        assert!(!is_known_file_transfer_mode(&summaries[0]));
        assert_eq!(
            waiting_status(1, 0, &summaries),
            (
                "waiting",
                "已枚举 1 个 USB 设备，未发现可用的 AirSlate 配件接口".to_owned()
            )
        );
    }

    #[test]
    fn startup_candidate_does_not_flash_authorization_before_replug() {
        assert!(!should_report_authorizing(true));
        assert!(should_report_authorizing(false));
    }

    #[test]
    fn failed_initial_candidate_waits_for_reenumeration_or_retry() {
        let failed = DiscoveryState::Initial {
            bus_id: "bus".to_owned(),
            port_chain: vec![2],
            vendor_id: 0x12D1,
            product_id: 0x1101,
        };
        let changed = DiscoveryState::Initial {
            bus_id: "bus".to_owned(),
            port_chain: vec![3],
            vendor_id: 0x12D1,
            product_id: 0x1101,
        };

        assert!(should_wait_after_initial_failure(Some(&failed), &failed));
        assert!(!should_wait_after_initial_failure(Some(&failed), &changed));
        assert!(!should_wait_after_initial_failure(None, &failed));
    }

    #[test]
    fn pre_handshake_stall_requires_pipe_reset_and_retry() {
        assert_eq!(
            transfer_error_action(UsbTransferPhase::AwaitingHandshake, TransferError::Stall),
            TransferErrorAction::ClearHaltAndRetry
        );
    }

    #[test]
    fn usb_ready_has_the_transport_only_wire_layout() {
        assert_eq!(USB_READY, [0x54, 0x4C, 0x53, 0x41, 7, 1, 0, 0]);
    }

    #[test]
    fn usb_ready_short_writes_advance_without_resending_the_prefix() {
        let sent = advance_usb_ready(0, 3).expect("first short write is valid");
        assert_eq!(sent, 3);
        let sent = advance_usb_ready(sent, 5).expect("remaining bytes complete the frame");
        assert_eq!(sent, USB_READY.len());
        assert!(advance_usb_ready(7, 2).is_err());
    }

    #[test]
    fn usb_ready_retries_only_recoverable_pre_handshake_errors() {
        assert_eq!(
            transfer_error_action(UsbTransferPhase::SendingReady, TransferError::Cancelled),
            TransferErrorAction::Retry
        );
        assert_eq!(
            transfer_error_action(UsbTransferPhase::SendingReady, TransferError::Stall),
            TransferErrorAction::ClearHaltAndRetry
        );
        assert_eq!(
            transfer_error_action(UsbTransferPhase::SendingReady, TransferError::Disconnected),
            TransferErrorAction::Fail
        );
    }

    #[test]
    fn disconnected_usb_ready_submit_is_retryable_only_before_any_byte_was_sent() {
        assert!(ready_disconnected_submit_can_reopen(0, 0));
        assert!(!ready_disconnected_submit_can_reopen(0, 1));
        assert!(!ready_disconnected_submit_can_reopen(1, 0));
        assert!(ready_retry_allowed(0, true));
        assert!(ready_retry_allowed(READY_SUBMIT_RETRY_LIMIT - 1, true));
        assert!(!ready_retry_allowed(READY_SUBMIT_RETRY_LIMIT, true));
        assert!(!ready_retry_allowed(0, false));
    }

    #[test]
    fn handshake_timeouts_report_first_and_then_periodically() {
        let mut diagnostics = HandshakeReadDiagnostics::default();
        assert_eq!(
            diagnostics.note_timeout(),
            Some(HandshakeTimeoutReport {
                consecutive: 1,
                total: 1
            })
        );
        for _ in 2..HANDSHAKE_TIMEOUT_REPORT_INTERVAL {
            assert_eq!(diagnostics.note_timeout(), None);
        }
        assert_eq!(
            diagnostics.note_timeout(),
            Some(HandshakeTimeoutReport {
                consecutive: HANDSHAKE_TIMEOUT_REPORT_INTERVAL,
                total: u64::from(HANDSHAKE_TIMEOUT_REPORT_INTERVAL)
            })
        );
    }

    #[test]
    fn handshake_progress_starts_a_new_timeout_run() {
        let mut diagnostics = HandshakeReadDiagnostics::default();
        assert!(diagnostics.note_timeout().is_some());
        assert_eq!(diagnostics.finish_timeout_run(), 1);
        assert_eq!(diagnostics.finish_timeout_run(), 0);
        assert_eq!(
            diagnostics.note_timeout(),
            Some(HandshakeTimeoutReport {
                consecutive: 1,
                total: 2
            })
        );
    }

    #[test]
    fn complete_handshake_request_is_consumed_before_transfer_error_recovery() {
        let request = encode_packet(&Packet::HandshakeRequest(
            crate::protocol::HandshakeRequest {
                client_id: "tablet-boundary".to_owned(),
            },
        ))
        .expect("handshake request should encode");
        assert_eq!(request.len(), HANDSHAKE_REQUEST_SIZE);

        let completion = Completion {
            buffer: request.clone().into(),
            actual_len: request.len(),
            status: Err(TransferError::Stall),
        };
        let mut packets = PacketStream::default();
        let parsed = push_completion_and_take_packet(&completion, &mut packets)
            .expect("complete request should parse")
            .expect("complete request should be available");

        assert_eq!(parsed.len(), HANDSHAKE_REQUEST_SIZE);
        assert_eq!(parsed[4], PacketType::HandshakeRequest as u8);
        assert_eq!(packets.buffered_len(), 0);

        let service = HandshakeService::new(test_workspace(), test_lifecycle());
        let response = service
            .handle_usb_request_bytes(&parsed, 7)
            .expect("the complete boundary request should reach USB handshake handling");
        assert!(matches!(
            decode_packet(&response),
            Ok(Packet::HandshakeResponse(_))
        ));
    }

    #[test]
    fn releasing_a_failed_usb_connection_allows_the_next_connection() {
        let lifecycle = test_lifecycle();
        lifecycle
            .create_usb_session("first-tablet", 41)
            .expect("first USB session should be created");
        lifecycle
            .release_usb_connection(41)
            .expect("failed USB session should be released");
        lifecycle
            .create_usb_session("reconnected-tablet", 42)
            .expect("the next physical USB connection should be accepted");
        lifecycle
            .release_usb_connection(42)
            .expect("the reconnect session should be releasable");
    }

    #[test]
    fn active_session_stall_is_not_swallowed() {
        assert_eq!(
            transfer_error_action(UsbTransferPhase::ActiveSession, TransferError::Stall),
            TransferErrorAction::Fail
        );
    }

    #[test]
    fn only_pre_handshake_timeout_cancellation_is_retried() {
        assert_eq!(
            transfer_error_action(
                UsbTransferPhase::AwaitingHandshake,
                TransferError::Cancelled
            ),
            TransferErrorAction::Retry
        );
        assert_eq!(
            transfer_error_action(
                UsbTransferPhase::AwaitingHandshake,
                TransferError::Disconnected
            ),
            TransferErrorAction::Fail
        );
        assert_eq!(
            transfer_error_action(UsbTransferPhase::ActiveSession, TransferError::Cancelled),
            TransferErrorAction::Fail
        );
    }

    fn test_lifecycle() -> Arc<SessionLifecycle> {
        Arc::new(SessionLifecycle::new(
            SessionService::shared(),
            Arc::new(NoopSink),
            SessionStatusBus::shared(),
        ))
    }

    fn test_workspace() -> WorkspaceService {
        let monitor = MonitorInfo {
            id: MonitorId::new("DISPLAY1".to_owned()),
            device_name: "DISPLAY1".to_owned(),
            is_primary: true,
            pixel_width: 2560,
            pixel_height: 1600,
            virtual_left: 0,
            virtual_top: 0,
            virtual_right: 2560,
            virtual_bottom: 1600,
        };
        WorkspaceService::from_snapshot(WorkspaceSnapshot {
            monitors: vec![monitor.clone()],
            active_monitor_id: Some(monitor.id.clone()),
            active_workspace: Some(ActiveWorkspace { monitor }),
        })
    }

    struct NoopSink;

    impl IncomingEventSink for NoopSink {
        fn emit(&self, _event: IncomingEvent) {}
    }
}
