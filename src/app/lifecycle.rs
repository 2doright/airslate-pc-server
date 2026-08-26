use std::{
    net::Ipv4Addr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

use serde::Serialize;
use tracing::warn;

use crate::{
    error::AppError,
    protocol::SessionDisconnect,
    session::{
        DisconnectDisposition, LocalDisconnectDisposition, RealtimeFrameDisposition, SessionSource,
        SharedSessionService,
    },
    udp_ingest::{IncomingEvent, IncomingEventSink},
};

pub const SESSION_STATUS_CHANGED_EVENT: &str = "session-status-changed";

#[derive(Debug)]
pub struct WiredConnectionGate {
    enabled: AtomicBool,
}

impl WiredConnectionGate {
    pub fn shared(enabled: bool) -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(enabled),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusEvent {
    pub has_active_session: bool,
}

#[derive(Debug, Default)]
pub struct SessionStatusBus {
    subscriber: Mutex<Option<Sender<SessionStatusEvent>>>,
}

impl SessionStatusBus {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn subscribe(&self) -> Result<Receiver<SessionStatusEvent>, AppError> {
        let (sender, receiver) = mpsc::channel();
        let mut subscriber = self
            .subscriber
            .lock()
            .map_err(|_| AppError::StatePoisoned("session status bus"))?;
        *subscriber = Some(sender);
        Ok(receiver)
    }

    fn publish(&self, event: SessionStatusEvent) {
        let sender = match self.subscriber.lock() {
            Ok(subscriber) => subscriber.clone(),
            Err(_) => {
                warn!("session status bus is poisoned; status event was not published");
                return;
            }
        };

        if let Some(sender) = sender
            && sender.send(event).is_err()
        {
            warn!("session status event receiver is closed");
        }
    }
}

#[derive(Clone)]
pub struct SessionLifecycle {
    session: SharedSessionService,
    input_sink: Arc<dyn IncomingEventSink>,
    status_bus: Arc<SessionStatusBus>,
    wired_gate: Arc<WiredConnectionGate>,
}

impl SessionLifecycle {
    #[cfg(test)]
    pub fn new(
        session: SharedSessionService,
        input_sink: Arc<dyn IncomingEventSink>,
        status_bus: Arc<SessionStatusBus>,
    ) -> Self {
        Self::new_with_wired_gate(
            session,
            input_sink,
            status_bus,
            WiredConnectionGate::shared(true),
        )
    }

    pub fn new_with_wired_gate(
        session: SharedSessionService,
        input_sink: Arc<dyn IncomingEventSink>,
        status_bus: Arc<SessionStatusBus>,
        wired_gate: Arc<WiredConnectionGate>,
    ) -> Self {
        Self {
            session,
            input_sink,
            status_bus,
            wired_gate,
        }
    }

    pub fn status_bus(&self) -> Arc<SessionStatusBus> {
        self.status_bus.clone()
    }

    pub fn begin_wired_disable(&self) -> Result<(), AppError> {
        self.wired_gate.set_enabled(false);
        let session = match self.session.lock() {
            Ok(session) => session,
            Err(_) => {
                self.wired_gate.set_enabled(true);
                return Err(AppError::StatePoisoned("session"));
            }
        };
        if session.has_active_usb_session() {
            self.wired_gate.set_enabled(true);
            return Err(AppError::WiredSessionActive);
        }
        Ok(())
    }

    pub fn set_wired_enabled(&self, enabled: bool) {
        self.wired_gate.set_enabled(enabled);
    }

    pub fn create_session(
        &self,
        client_id: impl Into<String>,
        peer_ip: Ipv4Addr,
    ) -> Result<String, AppError> {
        let session_id = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))?
            .create_session(client_id, peer_ip)?
            .session_id()
            .as_str()
            .to_owned();

        self.status_bus.publish(SessionStatusEvent {
            has_active_session: true,
        });
        Ok(session_id)
    }

    pub fn accept_realtime_source(
        &self,
        source_ip: Ipv4Addr,
    ) -> Result<RealtimeFrameDisposition, AppError> {
        self.session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))
            .map(|mut session| session.accept_realtime_source(source_ip))
    }

    pub fn create_usb_session(
        &self,
        client_id: impl Into<String>,
        connection_id: u64,
    ) -> Result<String, AppError> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))?;
        if !self.wired_gate.is_enabled() {
            return Err(AppError::WiredConnectionDisabled);
        }
        let session_id = session
            .create_usb_session(client_id, connection_id)?
            .session_id()
            .as_str()
            .to_owned();
        drop(session);
        self.status_bus.publish(SessionStatusEvent {
            has_active_session: true,
        });
        Ok(session_id)
    }

    pub fn accept_usb_realtime_source(
        &self,
        connection_id: u64,
    ) -> Result<RealtimeFrameDisposition, AppError> {
        self.session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))
            .map(|mut session| session.accept_usb_realtime_source(connection_id))
    }

    pub fn handle_udp_disconnect(
        &self,
        source_ip: Ipv4Addr,
        packet: &SessionDisconnect,
    ) -> Result<DisconnectDisposition, AppError> {
        let disposition = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))?
            .handle_udp_disconnect(source_ip, packet);

        if let DisconnectDisposition::Released {
            session_id,
            peer_ip,
        } = &disposition
        {
            self.finish_session(session_id.clone(), SessionSource::Network(*peer_ip));
        }

        Ok(disposition)
    }

    pub fn disconnect_locally(&self) -> Result<SessionStatusEvent, AppError> {
        let disposition = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))?
            .disconnect_locally();

        if let LocalDisconnectDisposition::Released { session_id, source } = disposition {
            self.finish_session(session_id, source);
        }

        Ok(SessionStatusEvent {
            has_active_session: false,
        })
    }

    pub fn emit_incoming(&self, event: IncomingEvent) {
        self.input_sink.emit(event);
    }

    pub fn handle_usb_disconnect(
        &self,
        connection_id: u64,
        packet: &SessionDisconnect,
    ) -> Result<LocalDisconnectDisposition, AppError> {
        let disposition = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))?
            .handle_usb_disconnect(connection_id, packet);
        if let LocalDisconnectDisposition::Released { session_id, source } = &disposition {
            self.finish_session(session_id.clone(), *source);
        }
        Ok(disposition)
    }

    pub fn release_usb_connection(&self, connection_id: u64) -> Result<(), AppError> {
        let disposition = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))?
            .release_usb_connection(connection_id);
        if let LocalDisconnectDisposition::Released { session_id, source } = disposition {
            self.finish_session(session_id, source);
        }
        Ok(())
    }

    fn finish_session(&self, session_id: String, source: SessionSource) {
        self.input_sink.emit(IncomingEvent::SessionEnded {
            session_id: session_id.clone(),
            source_ip: match source {
                SessionSource::Network(ip) => ip,
                SessionSource::Usb(_) => {
                    self.input_sink
                        .emit(IncomingEvent::UsbSessionEnded { session_id });
                    self.status_bus.publish(SessionStatusEvent {
                        has_active_session: false,
                    });
                    return;
                }
            },
        });
        self.status_bus.publish(SessionStatusEvent {
            has_active_session: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::session::SessionService;

    use super::*;

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<IncomingEvent>>,
    }

    impl IncomingEventSink for RecordingSink {
        fn emit(&self, event: IncomingEvent) {
            self.events.lock().expect("sink should lock").push(event);
        }
    }

    fn test_ip() -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 0, 10)
    }

    #[test]
    fn local_disconnect_reuses_session_end_cleanup_and_publishes_status() {
        let session = SessionService::shared();
        let sink = Arc::new(RecordingSink::default());
        let status_bus = SessionStatusBus::shared();
        let status_events = status_bus.subscribe().expect("status bus should subscribe");
        let lifecycle = SessionLifecycle::new(session.clone(), sink.clone(), status_bus);

        let session_id = lifecycle
            .create_session("client-a", test_ip())
            .expect("session should be created");
        assert_eq!(
            status_events.recv().expect("connected event should arrive"),
            SessionStatusEvent {
                has_active_session: true,
            }
        );

        let status = lifecycle
            .disconnect_locally()
            .expect("local disconnect should succeed");

        assert_eq!(
            status,
            SessionStatusEvent {
                has_active_session: false,
            }
        );
        assert_eq!(
            sink.events.lock().expect("sink should lock").as_slice(),
            [IncomingEvent::SessionEnded {
                session_id,
                source_ip: test_ip(),
            }]
        );
        assert_eq!(
            status_events
                .recv()
                .expect("disconnected event should arrive"),
            SessionStatusEvent {
                has_active_session: false,
            }
        );
        assert!(
            !session
                .lock()
                .expect("session should lock")
                .has_active_session()
        );
    }

    #[test]
    fn local_disconnect_when_already_inactive_returns_current_status() {
        let session = SessionService::shared();
        let sink = Arc::new(RecordingSink::default());
        let lifecycle = SessionLifecycle::new(session, sink.clone(), SessionStatusBus::shared());

        let status = lifecycle
            .disconnect_locally()
            .expect("already inactive should be a successful fact");

        assert!(!status.has_active_session);
        assert!(sink.events.lock().expect("sink should lock").is_empty());
    }

    #[test]
    fn active_usb_session_prevents_disabling_and_keeps_gate_open() {
        let session = SessionService::shared();
        let lifecycle = SessionLifecycle::new(
            session,
            Arc::new(RecordingSink::default()),
            SessionStatusBus::shared(),
        );
        lifecycle
            .create_usb_session("tablet", 41)
            .expect("USB session should be created");

        assert!(matches!(
            lifecycle.begin_wired_disable(),
            Err(AppError::WiredSessionActive)
        ));
        assert!(lifecycle.wired_gate.is_enabled());
    }

    #[test]
    fn disabled_gate_rejects_a_racing_usb_session_creation() {
        let session = SessionService::shared();
        let lifecycle = SessionLifecycle::new(
            session,
            Arc::new(RecordingSink::default()),
            SessionStatusBus::shared(),
        );

        lifecycle
            .begin_wired_disable()
            .expect("inactive USB transport can be disabled");

        assert!(matches!(
            lifecycle.create_usb_session("tablet", 42),
            Err(AppError::WiredConnectionDisabled)
        ));
    }
}
