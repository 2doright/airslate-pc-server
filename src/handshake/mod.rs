use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    sync::Arc,
};

use tracing::{info, warn};

use crate::{
    app::lifecycle::SessionLifecycle,
    error::AppError,
    protocol::{
        DesktopUnit, HANDSHAKE_REQUEST_SIZE, HandshakeError as ProtocolHandshakeError,
        HandshakeErrorCode, HandshakeResponse, Packet, PacketType, ProtocolError, decode_header,
        decode_packet, encode_packet,
    },
    workspace::WorkspaceService,
};

pub const HANDSHAKE_PORT: u16 = 48562;

pub struct HandshakeService {
    workspace: WorkspaceService,
    lifecycle: Arc<SessionLifecycle>,
}

impl HandshakeService {
    pub fn new(workspace: WorkspaceService, lifecycle: Arc<SessionLifecycle>) -> Self {
        Self {
            workspace,
            lifecycle,
        }
    }

    pub fn run(&self) -> Result<(), AppError> {
        let bind_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, HANDSHAKE_PORT);
        let listener = TcpListener::bind(bind_addr)?;
        info!(address = %listener.local_addr()?, "phase 4 handshake service listening");

        for stream in listener.incoming() {
            let mut stream = stream?;
            let peer_addr = match stream.peer_addr() {
                Ok(peer_addr) => peer_addr,
                Err(error) => {
                    warn!(error = %error, "failed to resolve handshake peer address");
                    continue;
                }
            };
            let SocketAddr::V4(peer_addr) = peer_addr else {
                warn!(peer = %peer_addr, "ignored non-ipv4 handshake peer");
                continue;
            };

            if let Err(error) = self.handle_connection(&mut stream, *peer_addr.ip()) {
                warn!(peer = %peer_addr, error = %error, "failed to handle handshake connection");
            }
        }

        Ok(())
    }

    fn handle_connection(&self, stream: &mut TcpStream, peer_ip: Ipv4Addr) -> Result<(), AppError> {
        let request_bytes = read_handshake_request(stream)?;
        let response_bytes = self.handle_request_bytes(&request_bytes, peer_ip)?;
        stream.write_all(&response_bytes)?;
        Ok(())
    }

    pub fn handle_request_bytes(
        &self,
        request_bytes: &[u8],
        peer_ip: Ipv4Addr,
    ) -> Result<Vec<u8>, AppError> {
        let response_packet = self.build_response_packet(request_bytes, peer_ip);
        encode_packet(&response_packet).map_err(AppError::from)
    }

    fn build_response_packet(&self, request_bytes: &[u8], peer_ip: Ipv4Addr) -> Packet {
        match self.build_handshake_response(request_bytes, peer_ip) {
            Ok(response) => Packet::HandshakeResponse(response),
            Err(error) => Packet::HandshakeError(error),
        }
    }

    fn build_handshake_response(
        &self,
        request_bytes: &[u8],
        peer_ip: Ipv4Addr,
    ) -> Result<HandshakeResponse, ProtocolHandshakeError> {
        match decode_header(request_bytes) {
            Ok(header) if header.packet_type == PacketType::HandshakeRequest => {}
            Ok(_) => {
                return Err(handshake_error(
                    HandshakeErrorCode::InvalidRequest,
                    "expected handshake request",
                ));
            }
            Err(ProtocolError::UnsupportedProtocolVersion(_)) => {
                return Err(handshake_error(
                    HandshakeErrorCode::UnsupportedProtocol,
                    "unsupported protocol version",
                ));
            }
            Err(_) => {
                return Err(handshake_error(
                    HandshakeErrorCode::InvalidRequest,
                    "invalid handshake header",
                ));
            }
        }

        let request = match decode_packet(request_bytes) {
            Ok(Packet::HandshakeRequest(request)) => request,
            Ok(_) => {
                return Err(handshake_error(
                    HandshakeErrorCode::InvalidRequest,
                    "expected handshake request",
                ));
            }
            Err(ProtocolError::UnsupportedProtocolVersion(_)) => {
                return Err(handshake_error(
                    HandshakeErrorCode::UnsupportedProtocol,
                    "unsupported protocol version",
                ));
            }
            Err(_) => {
                return Err(handshake_error(
                    HandshakeErrorCode::InvalidRequest,
                    "invalid handshake request",
                ));
            }
        };

        let workspace = self
            .workspace
            .current_workspace()
            .map_err(map_app_error_to_handshake)?;
        let desktop_width_px = workspace.monitor.pixel_width;
        let desktop_height_px = workspace.monitor.pixel_height;

        let session_id = self
            .lifecycle
            .create_session(request.client_id, peer_ip)
            .map_err(map_app_error_to_handshake)?;

        Ok(HandshakeResponse {
            session_id,
            desktop_width_px,
            desktop_height_px,
            desktop_unit: DesktopUnit::Pixel,
        })
    }
}

fn read_handshake_request(stream: &mut TcpStream) -> Result<Vec<u8>, AppError> {
    let mut request = [0_u8; HANDSHAKE_REQUEST_SIZE];
    let mut read_len = 0;

    while read_len < HANDSHAKE_REQUEST_SIZE {
        let chunk_len = stream.read(&mut request[read_len..])?;
        if chunk_len == 0 {
            break;
        }
        read_len += chunk_len;
    }

    Ok(request[..read_len].to_vec())
}

fn map_app_error_to_handshake(error: AppError) -> ProtocolHandshakeError {
    match error {
        AppError::SessionAlreadyActive => handshake_error(
            HandshakeErrorCode::AlreadyConnected,
            "session already active",
        ),
        AppError::Workspace(_) => handshake_error(
            HandshakeErrorCode::NoActiveWorkspace,
            "no active workspace is available",
        ),
        _ => handshake_error(HandshakeErrorCode::InternalError, "internal server error"),
    }
}

fn handshake_error(code: HandshakeErrorCode, message: &'static str) -> ProtocolHandshakeError {
    ProtocolHandshakeError {
        error_code: code,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        app::lifecycle::{SessionLifecycle, SessionStatusBus},
        protocol::{
            HandshakeErrorCode, HandshakeRequest, Packet, SessionDisconnect, decode_packet,
            encode_packet,
        },
        session::SessionService,
        udp_ingest::{IncomingEvent, IncomingEventSink},
        workspace::{ActiveWorkspace, MonitorId, MonitorInfo, WorkspaceSnapshot},
    };

    use super::*;

    #[test]
    fn valid_handshake_request_returns_handshake_response() {
        let service = HandshakeService::new(test_workspace_service(), test_lifecycle());
        let request_bytes = encode_packet(&Packet::HandshakeRequest(HandshakeRequest {
            client_id: "client-a".to_string(),
        }))
        .expect("request should encode");

        let response_bytes = service
            .handle_request_bytes(&request_bytes, test_ip())
            .expect("response should encode");

        let packet = decode_packet(&response_bytes).expect("response should decode");
        let Packet::HandshakeResponse(response) = packet else {
            panic!("expected handshake response");
        };

        assert!(!response.session_id.is_empty());
        assert_eq!(response.desktop_width_px, 2560);
        assert_eq!(response.desktop_height_px, 1600);
        assert_eq!(response.desktop_unit, DesktopUnit::Pixel);
    }

    #[test]
    fn unsupported_protocol_version_returns_handshake_error() {
        let service = HandshakeService::new(test_workspace_service(), test_lifecycle());
        let mut request_bytes = encode_packet(&Packet::HandshakeRequest(HandshakeRequest {
            client_id: "client-a".to_string(),
        }))
        .expect("request should encode");
        request_bytes[5] = 2;

        let response = decode_error_packet(
            service
                .handle_request_bytes(&request_bytes, test_ip())
                .expect("response should encode"),
        );

        assert_eq!(response.error_code, HandshakeErrorCode::UnsupportedProtocol);
    }

    #[test]
    fn invalid_request_length_returns_handshake_error() {
        let service = HandshakeService::new(test_workspace_service(), test_lifecycle());
        let mut request_bytes = encode_packet(&Packet::HandshakeRequest(HandshakeRequest {
            client_id: "client-a".to_string(),
        }))
        .expect("request should encode");
        request_bytes.pop();

        let response = decode_error_packet(
            service
                .handle_request_bytes(&request_bytes, test_ip())
                .expect("response should encode"),
        );

        assert_eq!(response.error_code, HandshakeErrorCode::InvalidRequest);
    }

    #[test]
    fn invalid_packet_type_returns_handshake_error() {
        let service = HandshakeService::new(test_workspace_service(), test_lifecycle());
        let request_bytes = encode_packet(&Packet::SessionDisconnect(SessionDisconnect {
            session_id: "session-a".to_string(),
        }))
        .expect("request should encode");

        let response = decode_error_packet(
            service
                .handle_request_bytes(&request_bytes, test_ip())
                .expect("response should encode"),
        );

        assert_eq!(response.error_code, HandshakeErrorCode::InvalidRequest);
    }

    #[test]
    fn empty_client_id_returns_handshake_error() {
        let service = HandshakeService::new(test_workspace_service(), test_lifecycle());
        let mut request_bytes = [0_u8; HANDSHAKE_REQUEST_SIZE];
        request_bytes[0..4].copy_from_slice(&0x4153_4C54_u32.to_le_bytes());
        request_bytes[4] = 1;
        request_bytes[5] = 1;

        let response = decode_error_packet(
            service
                .handle_request_bytes(&request_bytes, test_ip())
                .expect("response should encode"),
        );

        assert_eq!(response.error_code, HandshakeErrorCode::InvalidRequest);
    }

    #[test]
    fn active_session_returns_already_connected_error() {
        let service = HandshakeService::new(test_workspace_service(), test_lifecycle());
        let request_bytes = encode_packet(&Packet::HandshakeRequest(HandshakeRequest {
            client_id: "client-a".to_string(),
        }))
        .expect("request should encode");

        let first_packet = decode_packet(
            &service
                .handle_request_bytes(&request_bytes, test_ip())
                .expect("first response should encode"),
        )
        .expect("first response should decode");
        assert!(matches!(first_packet, Packet::HandshakeResponse(_)));

        let response = decode_error_packet(
            service
                .handle_request_bytes(&request_bytes, test_ip())
                .expect("second response should encode"),
        );

        assert_eq!(response.error_code, HandshakeErrorCode::AlreadyConnected);
    }

    #[test]
    fn missing_workspace_returns_no_active_workspace_error() {
        let service = HandshakeService::new(no_workspace_service(), test_lifecycle());
        let request_bytes = encode_packet(&Packet::HandshakeRequest(HandshakeRequest {
            client_id: "client-a".to_string(),
        }))
        .expect("request should encode");

        let response = decode_error_packet(
            service
                .handle_request_bytes(&request_bytes, test_ip())
                .expect("response should encode"),
        );

        assert_eq!(response.error_code, HandshakeErrorCode::NoActiveWorkspace);
    }

    fn decode_error_packet(response_bytes: Vec<u8>) -> ProtocolHandshakeError {
        let packet = decode_packet(&response_bytes).expect("response should decode");
        let Packet::HandshakeError(response) = packet else {
            panic!("expected handshake error");
        };
        response
    }

    fn test_ip() -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 0, 10)
    }

    fn test_lifecycle() -> Arc<SessionLifecycle> {
        Arc::new(SessionLifecycle::new(
            SessionService::shared(),
            Arc::new(NoopSink),
            SessionStatusBus::shared(),
        ))
    }

    struct NoopSink;

    impl IncomingEventSink for NoopSink {
        fn emit(&self, _event: IncomingEvent) {}
    }

    fn test_workspace_service() -> WorkspaceService {
        WorkspaceService::from_snapshot(WorkspaceSnapshot {
            monitors: vec![test_monitor()],
            active_monitor_id: Some(MonitorId::new("DISPLAY1".to_string())),
            active_workspace: Some(ActiveWorkspace {
                monitor: test_monitor(),
            }),
        })
    }

    fn no_workspace_service() -> WorkspaceService {
        WorkspaceService::from_snapshot(WorkspaceSnapshot {
            monitors: vec![],
            active_monitor_id: None,
            active_workspace: None,
        })
    }

    fn test_monitor() -> MonitorInfo {
        MonitorInfo {
            id: MonitorId::new("DISPLAY1".to_string()),
            device_name: "\\\\.\\DISPLAY1".to_string(),
            is_primary: true,
            pixel_width: 2560,
            pixel_height: 1600,
            virtual_left: 0,
            virtual_top: 0,
            virtual_right: 2560,
            virtual_bottom: 1600,
        }
    }
}
