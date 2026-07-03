use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    sync::Arc,
};

use tracing::{info, warn};

use crate::{
    error::AppError,
    protocol::{GestureFrame, Packet, SessionDisconnect, StylusFrame, decode_packet},
    session::{DisconnectDisposition, RealtimeFrameDisposition, SharedSessionService},
};

pub const STYLUS_DATA_PORT: u16 = 48563;
const MAX_PACKET_SIZE: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum IncomingEvent {
    Stylus {
        session_id: String,
        source_ip: Ipv4Addr,
        frame: StylusFrame,
    },
    Gesture {
        session_id: String,
        source_ip: Ipv4Addr,
        frame: GestureFrame,
    },
    SessionEnded {
        session_id: String,
        source_ip: Ipv4Addr,
    },
}

pub trait IncomingEventSink: Send + Sync {
    fn emit(&self, event: IncomingEvent);
}

pub struct UdpIngestService {
    session: SharedSessionService,
    sink: Arc<dyn IncomingEventSink>,
}

impl UdpIngestService {
    pub fn new(session: SharedSessionService, sink: Arc<dyn IncomingEventSink>) -> Self {
        Self { session, sink }
    }

    pub fn run(&self) -> Result<(), AppError> {
        let bind_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, STYLUS_DATA_PORT);
        let socket = UdpSocket::bind(bind_addr)?;
        info!(address = %socket.local_addr()?, "phase 5 udp ingest listening");

        let mut buffer = [0_u8; MAX_PACKET_SIZE];
        loop {
            let (len, source_addr) = socket.recv_from(&mut buffer)?;
            self.process_datagram(source_addr, &buffer[..len]);
        }
    }

    pub fn process_datagram(&self, source_addr: SocketAddr, bytes: &[u8]) -> Option<IncomingEvent> {
        let SocketAddr::V4(source_addr) = source_addr else {
            warn!(source = %source_addr, "ignored non-ipv4 udp packet");
            return None;
        };
        let source_ip = *source_addr.ip();

        let packet = match decode_packet(bytes) {
            Ok(packet) => packet,
            Err(error) => {
                warn!(source = %source_addr, error = %error, "failed to decode udp packet");
                return None;
            }
        };

        let event = match packet {
            Packet::StylusFrame(frame) => self.handle_stylus_frame(source_ip, frame),
            Packet::GestureFrame(frame) => self.handle_gesture_frame(source_ip, frame),
            Packet::SessionDisconnect(packet) => self.handle_disconnect_packet(source_ip, packet),
            Packet::HandshakeRequest(_)
            | Packet::HandshakeResponse(_)
            | Packet::HandshakeError(_) => {
                warn!(source = %source_addr, "ignored non-realtime udp packet type");
                None
            }
        };

        if let Some(event) = event.clone() {
            self.sink.emit(event);
        }

        event
    }

    fn handle_stylus_frame(
        &self,
        source_ip: Ipv4Addr,
        frame: StylusFrame,
    ) -> Option<IncomingEvent> {
        let disposition = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))
            .ok()?
            .accept_realtime_source(source_ip);

        match disposition {
            RealtimeFrameDisposition::Accepted {
                session_id,
                newly_bound,
            } => {
                if newly_bound {
                    info!(session_id = %session_id, source_ip = %source_ip, "bound udp source ip to active session");
                }
                Some(IncomingEvent::Stylus {
                    session_id,
                    source_ip,
                    frame,
                })
            }
            RealtimeFrameDisposition::IgnoredNoActiveSession => {
                info!(source_ip = %source_ip, "ignored stylus frame without active session");
                None
            }
            RealtimeFrameDisposition::IgnoredSourceIpMismatch { bound_ip } => {
                info!(source_ip = %source_ip, bound_ip = %bound_ip, "ignored stylus frame from mismatched source ip");
                None
            }
        }
    }

    fn handle_gesture_frame(
        &self,
        source_ip: Ipv4Addr,
        frame: GestureFrame,
    ) -> Option<IncomingEvent> {
        let disposition = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))
            .ok()?
            .accept_realtime_source(source_ip);

        match disposition {
            RealtimeFrameDisposition::Accepted {
                session_id,
                newly_bound,
            } => {
                if newly_bound {
                    info!(session_id = %session_id, source_ip = %source_ip, "bound udp source ip to active session");
                }
                info!(
                    session_id = %session_id,
                    source_ip = %source_ip,
                    gesture = ?frame.gesture_type,
                    state = ?frame.state,
                    seq = frame.seq,
                    val1 = frame.val1,
                    val2 = frame.val2,
                    val3 = frame.val3,
                    val4 = frame.val4,
                    "accepted gesture frame"
                );
                Some(IncomingEvent::Gesture {
                    session_id,
                    source_ip,
                    frame,
                })
            }
            RealtimeFrameDisposition::IgnoredNoActiveSession => {
                info!(source_ip = %source_ip, "ignored gesture frame without active session");
                None
            }
            RealtimeFrameDisposition::IgnoredSourceIpMismatch { bound_ip } => {
                info!(source_ip = %source_ip, bound_ip = %bound_ip, "ignored gesture frame from mismatched source ip");
                None
            }
        }
    }

    fn handle_disconnect_packet(
        &self,
        source_ip: Ipv4Addr,
        packet: SessionDisconnect,
    ) -> Option<IncomingEvent> {
        let disposition = self
            .session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))
            .ok()?
            .handle_udp_disconnect(source_ip, &packet);

        match disposition {
            DisconnectDisposition::Released { session_id } => Some(IncomingEvent::SessionEnded {
                session_id,
                source_ip,
            }),
            DisconnectDisposition::IgnoredNoActiveSession => {
                info!(source_ip = %source_ip, "ignored session disconnect without active session");
                None
            }
            DisconnectDisposition::IgnoredSessionIdMismatch => {
                info!(source_ip = %source_ip, session_id = %packet.session_id, "ignored session disconnect with mismatched session id");
                None
            }
            DisconnectDisposition::IgnoredSourceIpMismatch { bound_ip } => {
                info!(source_ip = %source_ip, bound_ip = %bound_ip, session_id = %packet.session_id, "ignored session disconnect from mismatched source ip");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::{
        protocol::{
            GestureFrame, GestureState, GestureType, HandshakeRequest, Packet, SessionDisconnect,
            StylusEventType, StylusFlags, StylusFrame, encode_packet,
        },
        session::SessionService,
    };

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

    fn test_service() -> (UdpIngestService, Arc<RecordingSink>, SharedSessionService) {
        let session = SessionService::shared();
        let sink = Arc::new(RecordingSink::default());
        let service = UdpIngestService::new(session.clone(), sink.clone());
        (service, sink, session)
    }

    fn test_addr(ip_last: u8, port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, ip_last), port))
    }

    fn stylus_packet() -> Vec<u8> {
        encode_packet(&Packet::StylusFrame(StylusFrame {
            seq: 1,
            timestamp: 10,
            x: 100,
            y: 200,
            pressure: 0.5,
            tilt_x: 0,
            tilt_y: 0,
            event_type: StylusEventType::Move,
            flags: StylusFlags(0b0000_0001),
            reserved_ext: 0,
        }))
        .expect("stylus packet should encode")
    }

    fn gesture_packet() -> Vec<u8> {
        encode_packet(&Packet::GestureFrame(GestureFrame {
            gesture_type: GestureType::TwoPan,
            state: GestureState::Update,
            seq: 2,
            timestamp: 11,
            val1: 1.0,
            val2: 2.0,
            val3: 3.0,
            val4: 4.0,
        }))
        .expect("gesture packet should encode")
    }

    #[test]
    fn stylus_packet_emits_event_with_active_session() {
        let (service, sink, session) = test_service();
        let session_id = session
            .lock()
            .expect("session should lock")
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_string();

        let event = service.process_datagram(test_addr(10, 9000), &stylus_packet());

        assert!(matches!(
            event,
            Some(IncomingEvent::Stylus {
                session_id: ref actual_session_id,
                source_ip,
                ..
            }) if actual_session_id == &session_id && source_ip == Ipv4Addr::new(192, 168, 0, 10)
        ));
        assert_eq!(sink.events.lock().expect("sink should lock").len(), 1);
    }

    #[test]
    fn gesture_packet_emits_event_with_active_session() {
        let (service, _, session) = test_service();
        let session_id = session
            .lock()
            .expect("session should lock")
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_string();

        let event = service.process_datagram(test_addr(10, 9001), &gesture_packet());

        assert!(matches!(
            event,
            Some(IncomingEvent::Gesture {
                session_id: ref actual_session_id,
                source_ip,
                ..
            }) if actual_session_id == &session_id && source_ip == Ipv4Addr::new(192, 168, 0, 10)
        ));
    }

    #[test]
    fn realtime_packet_without_active_session_is_ignored() {
        let (service, sink, _) = test_service();

        let event = service.process_datagram(test_addr(10, 9000), &stylus_packet());

        assert_eq!(event, None);
        assert!(sink.events.lock().expect("sink should lock").is_empty());
    }

    #[test]
    fn realtime_packet_from_different_ip_is_ignored_after_binding() {
        let (service, sink, session) = test_service();
        session
            .lock()
            .expect("session should lock")
            .create_session("client-a")
            .expect("session should be created");

        let first_event = service.process_datagram(test_addr(10, 9000), &stylus_packet());
        let second_event = service.process_datagram(test_addr(11, 9001), &gesture_packet());

        assert!(first_event.is_some());
        assert_eq!(second_event, None);
        assert_eq!(sink.events.lock().expect("sink should lock").len(), 1);
    }

    #[test]
    fn same_ip_different_port_is_accepted() {
        let (service, sink, session) = test_service();
        session
            .lock()
            .expect("session should lock")
            .create_session("client-a")
            .expect("session should be created");

        let first_event = service.process_datagram(test_addr(10, 9000), &stylus_packet());
        let second_event = service.process_datagram(test_addr(10, 9100), &gesture_packet());

        assert!(first_event.is_some());
        assert!(second_event.is_some());
        assert_eq!(sink.events.lock().expect("sink should lock").len(), 2);
    }

    #[test]
    fn valid_session_disconnect_releases_session() {
        let (service, sink, session) = test_service();
        let session_id = session
            .lock()
            .expect("session should lock")
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_string();

        let _ = service.process_datagram(test_addr(10, 9000), &stylus_packet());
        let packet = encode_packet(&Packet::SessionDisconnect(SessionDisconnect {
            session_id: session_id.clone(),
        }))
        .expect("disconnect packet should encode");

        let event = service.process_datagram(test_addr(10, 9002), &packet);

        assert_eq!(
            event,
            Some(IncomingEvent::SessionEnded {
                session_id,
                source_ip: Ipv4Addr::new(192, 168, 0, 10),
            })
        );
        assert_eq!(sink.events.lock().expect("sink should lock").len(), 2);
        assert!(
            !session
                .lock()
                .expect("session should lock")
                .has_active_session()
        );
    }

    #[test]
    fn wrong_session_disconnect_is_ignored() {
        let (service, sink, session) = test_service();
        session
            .lock()
            .expect("session should lock")
            .create_session("client-a")
            .expect("session should be created");

        let packet = encode_packet(&Packet::SessionDisconnect(SessionDisconnect {
            session_id: "wrong-session-id".to_string(),
        }))
        .expect("disconnect packet should encode");

        let event = service.process_datagram(test_addr(10, 9000), &packet);

        assert_eq!(event, None);
        assert!(sink.events.lock().expect("sink should lock").is_empty());
    }

    #[test]
    fn invalid_udp_packet_is_ignored() {
        let (service, sink, _) = test_service();

        let event = service.process_datagram(test_addr(10, 9000), &[1, 2, 3, 4]);

        assert_eq!(event, None);
        assert!(sink.events.lock().expect("sink should lock").is_empty());
    }

    #[test]
    fn handshake_packet_over_udp_is_ignored() {
        let (service, sink, session) = test_service();
        session
            .lock()
            .expect("session should lock")
            .create_session("client-a")
            .expect("session should be created");

        let packet = encode_packet(&Packet::HandshakeRequest(HandshakeRequest {
            client_id: "client-a".to_string(),
        }))
        .expect("handshake packet should encode");

        let event = service.process_datagram(test_addr(10, 9000), &packet);

        assert_eq!(event, None);
        assert!(sink.events.lock().expect("sink should lock").is_empty());
    }
}
