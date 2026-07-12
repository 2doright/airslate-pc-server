use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
};

use uuid::Uuid;

use crate::{
    error::AppError,
    protocol::{SESSION_ID_LEN, SessionDisconnect},
};

pub type SharedSessionService = Arc<Mutex<SessionService>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(value: String) -> Result<Self, AppError> {
        if value.is_empty() {
            return Err(AppError::InvalidSessionId("session id must not be empty"));
        }

        if value.len() > SESSION_ID_LEN {
            return Err(AppError::InvalidSessionId(
                "session id exceeds protocol limit",
            ));
        }

        Ok(Self(value))
    }

    pub fn generate() -> Result<Self, AppError> {
        Self::new(Uuid::new_v4().simple().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSession {
    session_id: SessionId,
    client_id: String,
    peer_ip: Ipv4Addr,
}

impl ActiveSession {
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeFrameDisposition {
    Accepted { session_id: String },
    IgnoredNoActiveSession,
    IgnoredSourceIpMismatch { bound_ip: Ipv4Addr },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectDisposition {
    Released {
        session_id: String,
        peer_ip: Ipv4Addr,
    },
    IgnoredNoActiveSession,
    IgnoredSessionIdMismatch,
    IgnoredSourceIpMismatch {
        bound_ip: Ipv4Addr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalDisconnectDisposition {
    Released {
        session_id: String,
        peer_ip: Ipv4Addr,
    },
    AlreadyInactive,
}

#[derive(Debug, Default)]
pub struct SessionService {
    active: Option<ActiveSession>,
}

impl SessionService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> SharedSessionService {
        Arc::new(Mutex::new(Self::new()))
    }

    pub fn has_active_session(&self) -> bool {
        self.active.is_some()
    }

    pub fn create_session(
        &mut self,
        client_id: impl Into<String>,
        peer_ip: Ipv4Addr,
    ) -> Result<&ActiveSession, AppError> {
        if self.active.is_some() {
            return Err(AppError::SessionAlreadyActive);
        }

        let active = ActiveSession {
            session_id: SessionId::generate()?,
            client_id: client_id.into(),
            peer_ip,
        };

        self.active = Some(active);
        Ok(self.active.as_ref().expect("active session inserted"))
    }

    pub fn accept_realtime_source(&mut self, source_ip: Ipv4Addr) -> RealtimeFrameDisposition {
        let Some(active) = self.active.as_mut() else {
            return RealtimeFrameDisposition::IgnoredNoActiveSession;
        };

        if active.peer_ip != source_ip {
            return RealtimeFrameDisposition::IgnoredSourceIpMismatch {
                bound_ip: active.peer_ip,
            };
        }

        RealtimeFrameDisposition::Accepted {
            session_id: active.session_id.as_str().to_string(),
        }
    }

    pub fn handle_udp_disconnect(
        &mut self,
        source_ip: Ipv4Addr,
        packet: &SessionDisconnect,
    ) -> DisconnectDisposition {
        let Some(active) = self.active.as_ref() else {
            return DisconnectDisposition::IgnoredNoActiveSession;
        };

        if active.session_id.as_str() != packet.session_id {
            return DisconnectDisposition::IgnoredSessionIdMismatch;
        }

        if active.peer_ip != source_ip {
            return DisconnectDisposition::IgnoredSourceIpMismatch {
                bound_ip: active.peer_ip,
            };
        }

        let session_id = active.session_id.as_str().to_string();
        let peer_ip = active.peer_ip;
        self.active = None;
        DisconnectDisposition::Released {
            session_id,
            peer_ip,
        }
    }

    pub fn disconnect_locally(&mut self) -> LocalDisconnectDisposition {
        let Some(active) = self.active.take() else {
            return LocalDisconnectDisposition::AlreadyInactive;
        };

        LocalDisconnectDisposition::Released {
            session_id: active.session_id.as_str().to_string(),
            peer_ip: active.peer_ip,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ip(a: u8) -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 0, a)
    }

    fn create_test_session(service: &mut SessionService, client_id: &str, ip_last: u8) -> String {
        service
            .create_session(client_id, test_ip(ip_last))
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_owned()
    }

    #[test]
    fn create_first_session_succeeds() {
        let mut service = SessionService::new();

        let session_id = create_test_session(&mut service, "client-a", 10);

        assert!(!session_id.is_empty());
        assert!(service.has_active_session());
    }

    #[test]
    fn second_concurrent_session_is_rejected() {
        let mut service = SessionService::new();
        let first_session_id = create_test_session(&mut service, "client-a", 10);

        let error = service
            .create_session("client-b", test_ip(10))
            .expect_err("second session should be rejected");

        assert!(matches!(error, AppError::SessionAlreadyActive));
        assert!(service.has_active_session());
        assert!(!first_session_id.is_empty());
    }

    #[test]
    fn session_can_be_created_again_after_udp_release() {
        let mut service = SessionService::new();
        let first_session_id = create_test_session(&mut service, "client-a", 10);

        let disposition = service.handle_udp_disconnect(
            test_ip(10),
            &SessionDisconnect {
                session_id: first_session_id,
            },
        );
        assert!(matches!(
            disposition,
            DisconnectDisposition::Released { .. }
        ));

        let second_session_id = create_test_session(&mut service, "client-b", 10);

        assert!(!second_session_id.is_empty());
    }

    #[test]
    fn generated_session_id_stays_within_protocol_limit() {
        let mut service = SessionService::new();
        let session_id = create_test_session(&mut service, "client-a", 10);

        assert!(!session_id.is_empty());
        assert!(session_id.len() <= SESSION_ID_LEN);
    }

    #[test]
    fn realtime_frame_from_handshake_ip_is_accepted() {
        let mut service = SessionService::new();
        let session_id = create_test_session(&mut service, "client-a", 10);

        let disposition = service.accept_realtime_source(test_ip(10));

        assert_eq!(
            disposition,
            RealtimeFrameDisposition::Accepted {
                session_id: session_id.clone(),
            }
        );
        assert_eq!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::Accepted { session_id }
        );
    }

    #[test]
    fn realtime_frame_from_handshake_ip_is_accepted_repeatedly() {
        let mut service = SessionService::new();
        let session_id = create_test_session(&mut service, "client-a", 10);

        assert!(matches!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::Accepted { .. }
        ));

        assert_eq!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::Accepted { session_id }
        );
    }

    #[test]
    fn realtime_frame_from_different_ip_is_ignored() {
        let mut service = SessionService::new();
        create_test_session(&mut service, "client-a", 10);

        assert!(matches!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::Accepted { .. }
        ));

        assert_eq!(
            service.accept_realtime_source(test_ip(11)),
            RealtimeFrameDisposition::IgnoredSourceIpMismatch {
                bound_ip: test_ip(10),
            }
        );
    }

    #[test]
    fn realtime_frame_without_active_session_is_ignored() {
        let mut service = SessionService::new();

        assert_eq!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::IgnoredNoActiveSession
        );
    }

    #[test]
    fn udp_disconnect_releases_matching_handshake_peer() {
        let mut service = SessionService::new();
        let session_id = create_test_session(&mut service, "client-a", 10);

        let disposition = service.handle_udp_disconnect(
            test_ip(10),
            &SessionDisconnect {
                session_id: session_id.clone(),
            },
        );

        assert_eq!(
            disposition,
            DisconnectDisposition::Released {
                session_id,
                peer_ip: test_ip(10),
            }
        );
        assert!(!service.has_active_session());
    }

    #[test]
    fn udp_disconnect_releases_matching_handshake_peer_after_realtime() {
        let mut service = SessionService::new();
        let session_id = create_test_session(&mut service, "client-a", 10);

        let _ = service.accept_realtime_source(test_ip(10));

        let disposition = service.handle_udp_disconnect(
            test_ip(10),
            &SessionDisconnect {
                session_id: session_id.clone(),
            },
        );

        assert_eq!(
            disposition,
            DisconnectDisposition::Released {
                session_id,
                peer_ip: test_ip(10),
            }
        );
        assert!(!service.has_active_session());
    }

    #[test]
    fn udp_disconnect_from_wrong_ip_is_ignored() {
        let mut service = SessionService::new();
        let session_id = create_test_session(&mut service, "client-a", 10);

        let _ = service.accept_realtime_source(test_ip(10));

        let disposition =
            service.handle_udp_disconnect(test_ip(11), &SessionDisconnect { session_id });

        assert_eq!(
            disposition,
            DisconnectDisposition::IgnoredSourceIpMismatch {
                bound_ip: test_ip(10),
            }
        );
        assert!(service.has_active_session());
    }

    #[test]
    fn udp_disconnect_with_wrong_session_id_is_ignored() {
        let mut service = SessionService::new();
        create_test_session(&mut service, "client-a", 10);

        let disposition = service.handle_udp_disconnect(
            test_ip(10),
            &SessionDisconnect {
                session_id: "wrong-session-id".to_string(),
            },
        );

        assert_eq!(disposition, DisconnectDisposition::IgnoredSessionIdMismatch);
        assert!(service.has_active_session());
    }

    #[test]
    fn session_id_rejects_empty_value() {
        let error = SessionId::new(String::new()).expect_err("empty session id should be rejected");

        assert!(matches!(
            error,
            AppError::InvalidSessionId("session id must not be empty")
        ));
    }

    #[test]
    fn session_id_rejects_oversized_value() {
        let error = SessionId::new("a".repeat(SESSION_ID_LEN + 1))
            .expect_err("oversized session id should be rejected");

        assert!(matches!(
            error,
            AppError::InvalidSessionId("session id exceeds protocol limit")
        ));
    }
}
