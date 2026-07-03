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
    udp_peer_ip: Option<Ipv4Addr>,
}

impl ActiveSession {
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeFrameDisposition {
    Accepted {
        session_id: String,
        newly_bound: bool,
    },
    IgnoredNoActiveSession,
    IgnoredSourceIpMismatch {
        bound_ip: Ipv4Addr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectDisposition {
    Released { session_id: String },
    IgnoredNoActiveSession,
    IgnoredSessionIdMismatch,
    IgnoredSourceIpMismatch { bound_ip: Ipv4Addr },
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
    ) -> Result<&ActiveSession, AppError> {
        if self.active.is_some() {
            return Err(AppError::SessionAlreadyActive);
        }

        let active = ActiveSession {
            session_id: SessionId::generate()?,
            client_id: client_id.into(),
            udp_peer_ip: None,
        };

        self.active = Some(active);
        Ok(self.active.as_ref().expect("active session inserted"))
    }

    pub fn accept_realtime_source(&mut self, source_ip: Ipv4Addr) -> RealtimeFrameDisposition {
        let Some(active) = self.active.as_mut() else {
            return RealtimeFrameDisposition::IgnoredNoActiveSession;
        };

        match active.udp_peer_ip {
            Some(bound_ip) if bound_ip != source_ip => {
                RealtimeFrameDisposition::IgnoredSourceIpMismatch { bound_ip }
            }
            Some(_) => RealtimeFrameDisposition::Accepted {
                session_id: active.session_id.as_str().to_string(),
                newly_bound: false,
            },
            None => {
                active.udp_peer_ip = Some(source_ip);
                RealtimeFrameDisposition::Accepted {
                    session_id: active.session_id.as_str().to_string(),
                    newly_bound: true,
                }
            }
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

        if let Some(bound_ip) = active.udp_peer_ip
            && bound_ip != source_ip
        {
            return DisconnectDisposition::IgnoredSourceIpMismatch { bound_ip };
        }

        let session_id = active.session_id.as_str().to_string();
        self.active = None;
        DisconnectDisposition::Released { session_id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ip(a: u8) -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 0, a)
    }

    #[test]
    fn create_first_session_succeeds() {
        let mut service = SessionService::new();

        let session_id = service
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_owned();

        assert!(!session_id.is_empty());
        assert!(service.has_active_session());
    }

    #[test]
    fn second_concurrent_session_is_rejected() {
        let mut service = SessionService::new();
        let first_session_id = service
            .create_session("client-a")
            .expect("first session should be created")
            .session_id()
            .as_str()
            .to_owned();

        let error = service
            .create_session("client-b")
            .expect_err("second session should be rejected");

        assert!(matches!(error, AppError::SessionAlreadyActive));
        assert!(service.has_active_session());
        assert!(!first_session_id.is_empty());
    }

    #[test]
    fn session_can_be_created_again_after_udp_release() {
        let mut service = SessionService::new();
        let first_session_id = service
            .create_session("client-a")
            .expect("first session should be created")
            .session_id()
            .as_str()
            .to_owned();

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

        let second_session_id = service
            .create_session("client-b")
            .expect("second session should be created after release")
            .session_id()
            .as_str()
            .to_owned();

        assert!(!second_session_id.is_empty());
    }

    #[test]
    fn generated_session_id_stays_within_protocol_limit() {
        let mut service = SessionService::new();
        let session_id = service
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_owned();

        assert!(!session_id.is_empty());
        assert!(session_id.len() <= SESSION_ID_LEN);
    }

    #[test]
    fn first_realtime_frame_binds_udp_source_ip() {
        let mut service = SessionService::new();
        let session_id = service
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_string();

        let disposition = service.accept_realtime_source(test_ip(10));

        assert_eq!(
            disposition,
            RealtimeFrameDisposition::Accepted {
                session_id: session_id.clone(),
                newly_bound: true,
            }
        );
        assert_eq!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::Accepted {
                session_id,
                newly_bound: false,
            }
        );
    }

    #[test]
    fn realtime_frame_from_same_ip_is_accepted_after_binding() {
        let mut service = SessionService::new();
        let session_id = service
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_string();

        assert!(matches!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::Accepted {
                newly_bound: true,
                ..
            }
        ));

        assert_eq!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::Accepted {
                session_id,
                newly_bound: false,
            }
        );
    }

    #[test]
    fn realtime_frame_from_different_ip_is_ignored_after_binding() {
        let mut service = SessionService::new();
        service
            .create_session("client-a")
            .expect("session should be created");

        assert!(matches!(
            service.accept_realtime_source(test_ip(10)),
            RealtimeFrameDisposition::Accepted {
                newly_bound: true,
                ..
            }
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
    fn udp_disconnect_releases_matching_unbound_session() {
        let mut service = SessionService::new();
        let session_id = service
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_string();

        let disposition = service.handle_udp_disconnect(
            test_ip(10),
            &SessionDisconnect {
                session_id: session_id.clone(),
            },
        );

        assert_eq!(disposition, DisconnectDisposition::Released { session_id });
        assert!(!service.has_active_session());
    }

    #[test]
    fn udp_disconnect_releases_matching_bound_session_from_same_ip() {
        let mut service = SessionService::new();
        let session_id = service
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_string();

        let _ = service.accept_realtime_source(test_ip(10));

        let disposition = service.handle_udp_disconnect(
            test_ip(10),
            &SessionDisconnect {
                session_id: session_id.clone(),
            },
        );

        assert_eq!(disposition, DisconnectDisposition::Released { session_id });
        assert!(!service.has_active_session());
    }

    #[test]
    fn udp_disconnect_from_wrong_ip_is_ignored_after_binding() {
        let mut service = SessionService::new();
        let session_id = service
            .create_session("client-a")
            .expect("session should be created")
            .session_id()
            .as_str()
            .to_string();

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
        service
            .create_session("client-a")
            .expect("session should be created");

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
