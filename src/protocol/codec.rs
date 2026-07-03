use crate::protocol::{
    constants::{
        CLIENT_ID_LEN, ERROR_MESSAGE_LEN, GESTURE_FRAME_SIZE, HANDSHAKE_ERROR_SIZE,
        HANDSHAKE_REQUEST_SIZE, HANDSHAKE_RESPONSE_SIZE, PACKET_HEADER_SIZE, PROTOCOL_MAGIC,
        PROTOCOL_VERSION, SESSION_DISCONNECT_SIZE, SESSION_ID_LEN, STYLUS_FRAME_SIZE,
    },
    error::ProtocolError,
    fixed_str::{decode_fixed_utf8, encode_fixed_utf8},
    model::{
        DesktopUnit, GestureFrame, GestureState, GestureType, HandshakeError, HandshakeErrorCode,
        HandshakeRequest, HandshakeResponse, Packet, PacketHeader, PacketType, SessionDisconnect,
        StylusEventType, StylusFlags, StylusFrame,
    },
};

pub fn decode_packet(bytes: &[u8]) -> Result<Packet, ProtocolError> {
    ensure_min_len(bytes, 5)?;

    let magic = read_u32_le(bytes, 0);
    if magic != PROTOCOL_MAGIC {
        return Err(ProtocolError::InvalidMagic { actual: magic });
    }

    let packet_type_raw = bytes[4];
    let packet_type = PacketType::try_from(packet_type_raw)
        .map_err(|_| ProtocolError::UnknownPacketType(packet_type_raw))?;

    match packet_type {
        PacketType::HandshakeRequest => {
            Ok(Packet::HandshakeRequest(HandshakeRequest::decode(bytes)?))
        }
        PacketType::HandshakeResponse => {
            Ok(Packet::HandshakeResponse(HandshakeResponse::decode(bytes)?))
        }
        PacketType::HandshakeError => Ok(Packet::HandshakeError(HandshakeError::decode(bytes)?)),
        PacketType::SessionDisconnect => {
            Ok(Packet::SessionDisconnect(SessionDisconnect::decode(bytes)?))
        }
        PacketType::StylusFrame => Ok(Packet::StylusFrame(StylusFrame::decode(bytes)?)),
        PacketType::GestureFrame => Ok(Packet::GestureFrame(GestureFrame::decode(bytes)?)),
    }
}

pub fn encode_packet(packet: &Packet) -> Result<Vec<u8>, ProtocolError> {
    match packet {
        Packet::HandshakeRequest(packet) => Ok(packet.encode()?.to_vec()),
        Packet::HandshakeResponse(packet) => Ok(packet.encode()?.to_vec()),
        Packet::HandshakeError(packet) => Ok(packet.encode()?.to_vec()),
        Packet::SessionDisconnect(packet) => Ok(packet.encode()?.to_vec()),
        Packet::StylusFrame(packet) => Ok(packet.encode()?.to_vec()),
        Packet::GestureFrame(packet) => Ok(packet.encode()?.to_vec()),
    }
}

pub fn decode_header(bytes: &[u8]) -> Result<PacketHeader, ProtocolError> {
    ensure_min_len(bytes, PACKET_HEADER_SIZE)?;

    let magic = read_u32_le(bytes, 0);
    if magic != PROTOCOL_MAGIC {
        return Err(ProtocolError::InvalidMagic { actual: magic });
    }

    let packet_type_raw = bytes[4];
    let packet_type = PacketType::try_from(packet_type_raw)
        .map_err(|_| ProtocolError::UnknownPacketType(packet_type_raw))?;

    let protocol_version = bytes[5];
    if protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedProtocolVersion(protocol_version));
    }

    let reserved = read_u16_le(bytes, 6);
    if reserved != 0 {
        return Err(ProtocolError::NonZeroReserved {
            field: "header.reserved",
            value: reserved as u32,
        });
    }

    Ok(PacketHeader {
        magic,
        packet_type,
        protocol_version,
        reserved,
    })
}

fn write_header(out: &mut [u8], header: PacketHeader) {
    out[0..4].copy_from_slice(&header.magic.to_le_bytes());
    out[4] = header.packet_type.into();
    out[5] = header.protocol_version;
    out[6..8].copy_from_slice(&header.reserved.to_le_bytes());
}

impl HandshakeRequest {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_exact_len("HandshakeRequest", bytes, HANDSHAKE_REQUEST_SIZE)?;
        let _header = decode_header(bytes)?;
        let client_id = decode_string::<CLIENT_ID_LEN>("clientId", bytes, 8)?;
        let packet = Self { client_id };
        packet.validate()?;
        Ok(packet)
    }

    pub fn encode(&self) -> Result<[u8; HANDSHAKE_REQUEST_SIZE], ProtocolError> {
        self.validate()?;

        let mut out = [0_u8; HANDSHAKE_REQUEST_SIZE];
        write_header(
            &mut out[..PACKET_HEADER_SIZE],
            PacketHeader::new(PacketType::HandshakeRequest),
        );
        out[8..72].copy_from_slice(&encode_fixed_utf8::<CLIENT_ID_LEN>(&self.client_id));
        Ok(out)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.client_id.is_empty() {
            return Err(ProtocolError::SemanticViolation(
                "clientId must not be empty",
            ));
        }
        Ok(())
    }
}

impl HandshakeResponse {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_exact_len("HandshakeResponse", bytes, HANDSHAKE_RESPONSE_SIZE)?;
        let _header = decode_header(bytes)?;
        let session_id = decode_string::<SESSION_ID_LEN>("sessionId", bytes, 8)?;
        let desktop_width_px = read_u32_le(bytes, 72);
        let desktop_height_px = read_u32_le(bytes, 76);
        let desktop_unit_raw = bytes[80];
        let desktop_unit = DesktopUnit::try_from(desktop_unit_raw).map_err(|_| {
            ProtocolError::UnknownEnumValue {
                field: "desktopUnit",
                value: desktop_unit_raw as u32,
            }
        })?;

        let packet = Self {
            session_id,
            desktop_width_px,
            desktop_height_px,
            desktop_unit,
        };
        packet.validate()?;
        Ok(packet)
    }

    pub fn encode(&self) -> Result<[u8; HANDSHAKE_RESPONSE_SIZE], ProtocolError> {
        self.validate()?;

        let mut out = [0_u8; HANDSHAKE_RESPONSE_SIZE];
        write_header(
            &mut out[..PACKET_HEADER_SIZE],
            PacketHeader::new(PacketType::HandshakeResponse),
        );
        out[8..72].copy_from_slice(&encode_fixed_utf8::<SESSION_ID_LEN>(&self.session_id));
        out[72..76].copy_from_slice(&self.desktop_width_px.to_le_bytes());
        out[76..80].copy_from_slice(&self.desktop_height_px.to_le_bytes());
        out[80] = self.desktop_unit.into();
        Ok(out)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.session_id.is_empty() {
            return Err(ProtocolError::SemanticViolation(
                "sessionId must not be empty",
            ));
        }
        if self.desktop_width_px == 0 {
            return Err(ProtocolError::OutOfRange {
                field: "desktopWidthPx",
                details: "must be greater than 0",
            });
        }
        if self.desktop_height_px == 0 {
            return Err(ProtocolError::OutOfRange {
                field: "desktopHeightPx",
                details: "must be greater than 0",
            });
        }
        if self.desktop_unit != DesktopUnit::Pixel {
            return Err(ProtocolError::SemanticViolation(
                "desktopUnit must be PIXEL",
            ));
        }
        Ok(())
    }
}

impl HandshakeError {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_exact_len("HandshakeError", bytes, HANDSHAKE_ERROR_SIZE)?;
        let _header = decode_header(bytes)?;

        let error_code_raw = read_u16_le(bytes, 8);
        let error_code = HandshakeErrorCode::try_from(error_code_raw).map_err(|_| {
            ProtocolError::UnknownEnumValue {
                field: "errorCode",
                value: error_code_raw as u32,
            }
        })?;

        let reserved = read_u16_le(bytes, 10);
        if reserved != 0 {
            return Err(ProtocolError::NonZeroReserved {
                field: "handshakeError.reserved",
                value: reserved as u32,
            });
        }

        let message = decode_string::<ERROR_MESSAGE_LEN>("message", bytes, 12)?;
        let packet = Self {
            error_code,
            message,
        };
        packet.validate()?;
        Ok(packet)
    }

    pub fn encode(&self) -> Result<[u8; HANDSHAKE_ERROR_SIZE], ProtocolError> {
        self.validate()?;

        let mut out = [0_u8; HANDSHAKE_ERROR_SIZE];
        write_header(
            &mut out[..PACKET_HEADER_SIZE],
            PacketHeader::new(PacketType::HandshakeError),
        );
        out[8..10].copy_from_slice(&u16::from(self.error_code).to_le_bytes());
        out[10..12].copy_from_slice(&0_u16.to_le_bytes());
        out[12..108].copy_from_slice(&encode_fixed_utf8::<ERROR_MESSAGE_LEN>(&self.message));
        Ok(out)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        Ok(())
    }
}

impl SessionDisconnect {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_exact_len("SessionDisconnect", bytes, SESSION_DISCONNECT_SIZE)?;
        let _header = decode_header(bytes)?;
        let session_id = decode_string::<SESSION_ID_LEN>("sessionId", bytes, 8)?;
        let packet = Self { session_id };
        packet.validate()?;
        Ok(packet)
    }

    pub fn encode(&self) -> Result<[u8; SESSION_DISCONNECT_SIZE], ProtocolError> {
        self.validate()?;

        let mut out = [0_u8; SESSION_DISCONNECT_SIZE];
        write_header(
            &mut out[..PACKET_HEADER_SIZE],
            PacketHeader::new(PacketType::SessionDisconnect),
        );
        out[8..72].copy_from_slice(&encode_fixed_utf8::<SESSION_ID_LEN>(&self.session_id));
        Ok(out)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.session_id.is_empty() {
            return Err(ProtocolError::SemanticViolation(
                "sessionId must not be empty",
            ));
        }
        Ok(())
    }
}

impl StylusFrame {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_exact_len("StylusFrame", bytes, STYLUS_FRAME_SIZE)?;
        let _header = decode_header(bytes)?;

        let event_type_raw = bytes[30];
        let event_type = StylusEventType::try_from(event_type_raw).map_err(|_| {
            ProtocolError::UnknownEnumValue {
                field: "eventType",
                value: event_type_raw as u32,
            }
        })?;

        let packet = Self {
            seq: read_u32_le(bytes, 8),
            timestamp: read_u64_le(bytes, 12),
            x: read_u16_le(bytes, 20),
            y: read_u16_le(bytes, 22),
            pressure: read_f32_le(bytes, 24),
            tilt_x: bytes[28] as i8,
            tilt_y: bytes[29] as i8,
            event_type,
            flags: StylusFlags(bytes[31]),
            reserved_ext: read_u32_le(bytes, 32),
        };

        packet.validate()?;
        Ok(packet)
    }

    pub fn encode(&self) -> Result<[u8; STYLUS_FRAME_SIZE], ProtocolError> {
        self.validate()?;

        let mut out = [0_u8; STYLUS_FRAME_SIZE];
        write_header(
            &mut out[..PACKET_HEADER_SIZE],
            PacketHeader::new(PacketType::StylusFrame),
        );
        out[8..12].copy_from_slice(&self.seq.to_le_bytes());
        out[12..20].copy_from_slice(&self.timestamp.to_le_bytes());
        out[20..22].copy_from_slice(&self.x.to_le_bytes());
        out[22..24].copy_from_slice(&self.y.to_le_bytes());
        out[24..28].copy_from_slice(&self.pressure.to_le_bytes());
        out[28] = self.tilt_x as u8;
        out[29] = self.tilt_y as u8;
        out[30] = self.event_type.into();
        out[31] = self.flags.0;
        out[32..36].copy_from_slice(&self.reserved_ext.to_le_bytes());
        Ok(out)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.x > 32_767 {
            return Err(ProtocolError::OutOfRange {
                field: "x",
                details: "must be in 0..=32767",
            });
        }
        if self.y > 32_767 {
            return Err(ProtocolError::OutOfRange {
                field: "y",
                details: "must be in 0..=32767",
            });
        }
        if !self.pressure.is_finite() {
            return Err(ProtocolError::InvalidFloat { field: "pressure" });
        }
        if !(0.0..=1.0).contains(&self.pressure) {
            return Err(ProtocolError::OutOfRange {
                field: "pressure",
                details: "must be in [0.0, 1.0]",
            });
        }
        if !(-90..=90).contains(&self.tilt_x) {
            return Err(ProtocolError::OutOfRange {
                field: "tiltX",
                details: "must be in [-90, 90]",
            });
        }
        if !(-90..=90).contains(&self.tilt_y) {
            return Err(ProtocolError::OutOfRange {
                field: "tiltY",
                details: "must be in [-90, 90]",
            });
        }
        if self.flags.has_reserved_bits() {
            return Err(ProtocolError::SemanticViolation(
                "stylus flags bit7 must be zero",
            ));
        }
        if self.reserved_ext != 0 {
            return Err(ProtocolError::NonZeroReserved {
                field: "stylus.reservedExt",
                value: self.reserved_ext,
            });
        }
        Ok(())
    }
}

impl GestureFrame {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_exact_len("GestureFrame", bytes, GESTURE_FRAME_SIZE)?;

        let magic = read_u32_le(bytes, 0);
        if magic != PROTOCOL_MAGIC {
            return Err(ProtocolError::InvalidMagic { actual: magic });
        }

        let packet_type_raw = bytes[4];
        let packet_type = PacketType::try_from(packet_type_raw)
            .map_err(|_| ProtocolError::UnknownPacketType(packet_type_raw))?;
        if packet_type != PacketType::GestureFrame {
            return Err(ProtocolError::SemanticViolation(
                "packet type does not match GestureFrame",
            ));
        }

        let gesture_type_raw = bytes[5];
        let gesture_type = GestureType::try_from(gesture_type_raw).map_err(|_| {
            ProtocolError::UnknownEnumValue {
                field: "gestureType",
                value: gesture_type_raw as u32,
            }
        })?;

        let state_raw = bytes[6];
        let state =
            GestureState::try_from(state_raw).map_err(|_| ProtocolError::UnknownEnumValue {
                field: "state",
                value: state_raw as u32,
            })?;

        if bytes[7] != 0 {
            return Err(ProtocolError::NonZeroReserved {
                field: "gesture.reserved",
                value: bytes[7] as u32,
            });
        }

        let packet = Self {
            gesture_type,
            state,
            seq: read_u32_le(bytes, 8),
            timestamp: read_u64_le(bytes, 12),
            val1: read_f32_le(bytes, 20),
            val2: read_f32_le(bytes, 24),
            val3: read_f32_le(bytes, 28),
            val4: read_f32_le(bytes, 32),
        };

        packet.validate()?;
        Ok(packet)
    }

    pub fn encode(&self) -> Result<[u8; GESTURE_FRAME_SIZE], ProtocolError> {
        self.validate()?;

        let mut out = [0_u8; GESTURE_FRAME_SIZE];
        out[0..4].copy_from_slice(&PROTOCOL_MAGIC.to_le_bytes());
        out[4] = PacketType::GestureFrame.into();
        out[5] = self.gesture_type.into();
        out[6] = self.state.into();
        out[7] = 0;
        out[8..12].copy_from_slice(&self.seq.to_le_bytes());
        out[12..20].copy_from_slice(&self.timestamp.to_le_bytes());
        out[20..24].copy_from_slice(&self.val1.to_le_bytes());
        out[24..28].copy_from_slice(&self.val2.to_le_bytes());
        out[28..32].copy_from_slice(&self.val3.to_le_bytes());
        out[32..36].copy_from_slice(&self.val4.to_le_bytes());
        Ok(out)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("val1", self.val1),
            ("val2", self.val2),
            ("val3", self.val3),
            ("val4", self.val4),
        ] {
            if !value.is_finite() {
                return Err(ProtocolError::InvalidFloat { field });
            }
        }

        if self.gesture_type.is_swipe() {
            if self.state != GestureState::Begin {
                return Err(ProtocolError::SemanticViolation(
                    "swipe gesture only allows Begin state",
                ));
            }
            if self.val1 != 0.0 && self.val1 != 1.0 {
                return Err(ProtocolError::OutOfRange {
                    field: "val1",
                    details: "swipe direction must be 0 or 1",
                });
            }
            if self.val2 != 0.0 || self.val3 != 0.0 || self.val4 != 0.0 {
                return Err(ProtocolError::SemanticViolation(
                    "swipe val2..val4 must be zero",
                ));
            }
        }

        if self.gesture_type.is_rotate()
            && (self.val2 != 0.0 || self.val3 != 0.0 || self.val4 != 0.0)
        {
            return Err(ProtocolError::SemanticViolation(
                "rotate val2..val4 must be zero",
            ));
        }

        if self.gesture_type.is_long_press() && (self.val3 != 0.0 || self.val4 != 0.0) {
            return Err(ProtocolError::SemanticViolation(
                "long press val3..val4 must be zero",
            ));
        }

        if self.gesture_type.is_pinch() && self.val4 != 0.0 {
            return Err(ProtocolError::SemanticViolation("pinch val4 must be zero"));
        }

        Ok(())
    }
}

fn ensure_min_len(bytes: &[u8], expected: usize) -> Result<(), ProtocolError> {
    if bytes.len() < expected {
        return Err(ProtocolError::PacketTooShort {
            expected,
            actual: bytes.len(),
        });
    }
    Ok(())
}

fn ensure_exact_len(
    packet: &'static str,
    bytes: &[u8],
    expected: usize,
) -> Result<(), ProtocolError> {
    if bytes.len() != expected {
        return Err(ProtocolError::InvalidPacketLength {
            packet,
            expected,
            actual: bytes.len(),
        });
    }
    Ok(())
}

fn decode_string<const N: usize>(
    field: &'static str,
    bytes: &[u8],
    offset: usize,
) -> Result<String, ProtocolError> {
    let end = offset + N;
    let field_bytes: [u8; N] = bytes[offset..end]
        .try_into()
        .expect("slice length already validated");
    decode_fixed_utf8(field, &field_bytes)
}

fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("slice length already validated"),
    )
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("slice length already validated"),
    )
}

fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("slice length already validated"),
    )
}

fn read_f32_le(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("slice length already validated"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::{
        GESTURE_FRAME_SIZE, HANDSHAKE_ERROR_SIZE, HANDSHAKE_REQUEST_SIZE, HANDSHAKE_RESPONSE_SIZE,
        SESSION_DISCONNECT_SIZE, STYLUS_FRAME_SIZE,
    };

    #[test]
    fn fixed_length_constants_match_spec() {
        assert_eq!(HANDSHAKE_REQUEST_SIZE, 72);
        assert_eq!(HANDSHAKE_RESPONSE_SIZE, 81);
        assert_eq!(HANDSHAKE_ERROR_SIZE, 108);
        assert_eq!(SESSION_DISCONNECT_SIZE, 72);
        assert_eq!(STYLUS_FRAME_SIZE, 36);
        assert_eq!(GESTURE_FRAME_SIZE, 36);
    }

    #[test]
    fn handshake_request_round_trip_and_layout() {
        let packet = HandshakeRequest {
            client_id: "device-01".to_string(),
        };

        let encoded = packet.encode().unwrap();
        assert_eq!(encoded.len(), HANDSHAKE_REQUEST_SIZE);
        assert_eq!(
            u32::from_le_bytes(encoded[0..4].try_into().unwrap()),
            PROTOCOL_MAGIC
        );
        assert_eq!(encoded[4], PacketType::HandshakeRequest as u8);
        assert_eq!(encoded[5], PROTOCOL_VERSION);
        assert_eq!(u16::from_le_bytes(encoded[6..8].try_into().unwrap()), 0);

        let decoded = HandshakeRequest::decode(&encoded).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn handshake_response_round_trip_and_layout() {
        let packet = HandshakeResponse {
            session_id: "session-01".to_string(),
            desktop_width_px: 3840,
            desktop_height_px: 2160,
            desktop_unit: DesktopUnit::Pixel,
        };

        let encoded = packet.encode().unwrap();
        assert_eq!(encoded.len(), HANDSHAKE_RESPONSE_SIZE);
        assert_eq!(
            u32::from_le_bytes(encoded[72..76].try_into().unwrap()),
            3840
        );
        assert_eq!(
            u32::from_le_bytes(encoded[76..80].try_into().unwrap()),
            2160
        );
        assert_eq!(encoded[80], DesktopUnit::Pixel as u8);

        let decoded = HandshakeResponse::decode(&encoded).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn handshake_error_round_trip_and_layout() {
        let packet = HandshakeError {
            error_code: HandshakeErrorCode::AlreadyConnected,
            message: "busy".to_string(),
        };

        let encoded = packet.encode().unwrap();
        assert_eq!(encoded.len(), HANDSHAKE_ERROR_SIZE);
        assert_eq!(u16::from_le_bytes(encoded[8..10].try_into().unwrap()), 3);
        assert_eq!(u16::from_le_bytes(encoded[10..12].try_into().unwrap()), 0);

        let decoded = HandshakeError::decode(&encoded).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn session_disconnect_round_trip_and_layout() {
        let packet = SessionDisconnect {
            session_id: "session-02".to_string(),
        };

        let encoded = packet.encode().unwrap();
        assert_eq!(encoded.len(), SESSION_DISCONNECT_SIZE);

        let decoded = SessionDisconnect::decode(&encoded).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn stylus_frame_round_trip_and_layout() {
        let packet = StylusFrame {
            seq: 7,
            timestamp: 123456,
            x: 32000,
            y: 123,
            pressure: 0.75,
            tilt_x: -45,
            tilt_y: 60,
            event_type: StylusEventType::Move,
            flags: StylusFlags(0b0000_0011),
            reserved_ext: 0,
        };

        let encoded = packet.encode().unwrap();
        assert_eq!(encoded.len(), STYLUS_FRAME_SIZE);
        assert_eq!(u32::from_le_bytes(encoded[8..12].try_into().unwrap()), 7);
        assert_eq!(
            u64::from_le_bytes(encoded[12..20].try_into().unwrap()),
            123456
        );
        assert_eq!(
            u16::from_le_bytes(encoded[20..22].try_into().unwrap()),
            32000
        );
        assert_eq!(u16::from_le_bytes(encoded[22..24].try_into().unwrap()), 123);
        assert_eq!(
            f32::from_le_bytes(encoded[24..28].try_into().unwrap()),
            0.75
        );
        assert_eq!(encoded[30], StylusEventType::Move as u8);

        let decoded = StylusFrame::decode(&encoded).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn gesture_frame_round_trip_and_layout() {
        let packet = GestureFrame {
            gesture_type: GestureType::TwoPan,
            state: GestureState::Update,
            seq: 9,
            timestamp: 654321,
            val1: 1.0,
            val2: 2.0,
            val3: 3.0,
            val4: 4.0,
        };

        let encoded = packet.encode().unwrap();
        assert_eq!(encoded.len(), GESTURE_FRAME_SIZE);
        assert_eq!(
            u32::from_le_bytes(encoded[0..4].try_into().unwrap()),
            PROTOCOL_MAGIC
        );
        assert_eq!(encoded[4], PacketType::GestureFrame as u8);
        assert_eq!(encoded[5], GestureType::TwoPan as u8);
        assert_eq!(encoded[6], GestureState::Update as u8);
        assert_eq!(encoded[7], 0);

        let decoded = GestureFrame::decode(&encoded).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn decode_packet_dispatches_all_types() {
        let request = Packet::HandshakeRequest(HandshakeRequest {
            client_id: "device-01".to_string(),
        });
        let response = Packet::HandshakeResponse(HandshakeResponse {
            session_id: "session-01".to_string(),
            desktop_width_px: 1920,
            desktop_height_px: 1080,
            desktop_unit: DesktopUnit::Pixel,
        });
        let error = Packet::HandshakeError(HandshakeError {
            error_code: HandshakeErrorCode::InternalError,
            message: "internal".to_string(),
        });
        let disconnect = Packet::SessionDisconnect(SessionDisconnect {
            session_id: "session-01".to_string(),
        });
        let stylus = Packet::StylusFrame(StylusFrame {
            seq: 1,
            timestamp: 1,
            x: 0,
            y: 0,
            pressure: 0.0,
            tilt_x: 0,
            tilt_y: 0,
            event_type: StylusEventType::Cancel,
            flags: StylusFlags(0),
            reserved_ext: 0,
        });
        let gesture = Packet::GestureFrame(GestureFrame {
            gesture_type: GestureType::OneSwipe,
            state: GestureState::Begin,
            seq: 2,
            timestamp: 2,
            val1: 1.0,
            val2: 0.0,
            val3: 0.0,
            val4: 0.0,
        });

        for packet in [request, response, error, disconnect, stylus, gesture] {
            let encoded = encode_packet(&packet).unwrap();
            let decoded = decode_packet(&encoded).unwrap();
            assert_eq!(decoded, packet);
        }
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut bytes = HandshakeRequest {
            client_id: "device-01".to_string(),
        }
        .encode()
        .unwrap();
        bytes[0] = 0;

        let error = HandshakeRequest::decode(&bytes).unwrap_err();
        assert_eq!(
            error,
            ProtocolError::InvalidMagic {
                actual: 0x4153_4C00
            }
        );
    }

    #[test]
    fn rejects_invalid_header_version() {
        let mut bytes = HandshakeRequest {
            client_id: "device-01".to_string(),
        }
        .encode()
        .unwrap();
        bytes[5] = 9;

        let error = HandshakeRequest::decode(&bytes).unwrap_err();
        assert_eq!(error, ProtocolError::UnsupportedProtocolVersion(9));
    }

    #[test]
    fn rejects_invalid_packet_length() {
        let bytes = [0_u8; HANDSHAKE_REQUEST_SIZE - 1];
        let error = HandshakeRequest::decode(&bytes).unwrap_err();
        assert_eq!(
            error,
            ProtocolError::InvalidPacketLength {
                packet: "HandshakeRequest",
                expected: HANDSHAKE_REQUEST_SIZE,
                actual: HANDSHAKE_REQUEST_SIZE - 1,
            }
        );
    }

    #[test]
    fn rejects_invalid_fixed_string_padding() {
        let mut bytes = HandshakeRequest {
            client_id: "abc".to_string(),
        }
        .encode()
        .unwrap();
        bytes[12] = b'x';

        let error = HandshakeRequest::decode(&bytes).unwrap_err();
        assert_eq!(error, ProtocolError::InvalidPadding { field: "clientId" });
    }

    #[test]
    fn truncates_fixed_utf8_without_splitting_code_point() {
        let value = "你你你你你你你你你你你你你你你你你你你你你你你";
        let encoded = encode_fixed_utf8::<10>(value);
        let decoded = decode_fixed_utf8("field", &encoded).unwrap();
        assert_eq!(decoded.as_bytes().len(), 9);
        assert_eq!(decoded, "你你你");
    }

    #[test]
    fn rejects_stylus_pressure_out_of_range() {
        let packet = StylusFrame {
            seq: 1,
            timestamp: 1,
            x: 1,
            y: 1,
            pressure: 1.5,
            tilt_x: 0,
            tilt_y: 0,
            event_type: StylusEventType::Move,
            flags: StylusFlags(0),
            reserved_ext: 0,
        };

        let error = packet.encode().unwrap_err();
        assert_eq!(
            error,
            ProtocolError::OutOfRange {
                field: "pressure",
                details: "must be in [0.0, 1.0]",
            }
        );
    }

    #[test]
    fn rejects_stylus_reserved_bits() {
        let packet = StylusFrame {
            seq: 1,
            timestamp: 1,
            x: 1,
            y: 1,
            pressure: 0.5,
            tilt_x: 0,
            tilt_y: 0,
            event_type: StylusEventType::Move,
            flags: StylusFlags(0b1000_0000),
            reserved_ext: 0,
        };

        let error = packet.encode().unwrap_err();
        assert_eq!(
            error,
            ProtocolError::SemanticViolation("stylus flags bit7 must be zero")
        );
    }

    #[test]
    fn rejects_gesture_swipe_non_begin_state() {
        let packet = GestureFrame {
            gesture_type: GestureType::OneSwipe,
            state: GestureState::Update,
            seq: 1,
            timestamp: 1,
            val1: 0.0,
            val2: 0.0,
            val3: 0.0,
            val4: 0.0,
        };

        let error = packet.encode().unwrap_err();
        assert_eq!(
            error,
            ProtocolError::SemanticViolation("swipe gesture only allows Begin state")
        );
    }

    #[test]
    fn rejects_gesture_swipe_non_zero_tail_values() {
        let packet = GestureFrame {
            gesture_type: GestureType::OneSwipe,
            state: GestureState::Begin,
            seq: 1,
            timestamp: 1,
            val1: 1.0,
            val2: 1.0,
            val3: 0.0,
            val4: 0.0,
        };

        let error = packet.encode().unwrap_err();
        assert_eq!(
            error,
            ProtocolError::SemanticViolation("swipe val2..val4 must be zero")
        );
    }

    #[test]
    fn rejects_removed_swipe_gesture_types() {
        for gesture_type_raw in [10_u8, 11_u8] {
            let mut bytes = GestureFrame {
                gesture_type: GestureType::OneSwipe,
                state: GestureState::Begin,
                seq: 1,
                timestamp: 1,
                val1: 0.0,
                val2: 0.0,
                val3: 0.0,
                val4: 0.0,
            }
            .encode()
            .unwrap();
            bytes[5] = gesture_type_raw;

            let error = GestureFrame::decode(&bytes).unwrap_err();
            assert_eq!(
                error,
                ProtocolError::UnknownEnumValue {
                    field: "gestureType",
                    value: gesture_type_raw as u32,
                }
            );
        }
    }

    #[test]
    fn rejects_gesture_rotate_non_zero_tail_values() {
        let packet = GestureFrame {
            gesture_type: GestureType::TwoRotate,
            state: GestureState::Update,
            seq: 1,
            timestamp: 1,
            val1: 90.0,
            val2: 1.0,
            val3: 0.0,
            val4: 0.0,
        };

        let error = packet.encode().unwrap_err();
        assert_eq!(
            error,
            ProtocolError::SemanticViolation("rotate val2..val4 must be zero")
        );
    }

    #[test]
    fn rejects_gesture_pinch_non_zero_val4() {
        let packet = GestureFrame {
            gesture_type: GestureType::TwoPinch,
            state: GestureState::Update,
            seq: 1,
            timestamp: 1,
            val1: 1.2,
            val2: 10.0,
            val3: 11.0,
            val4: 1.0,
        };

        let error = packet.encode().unwrap_err();
        assert_eq!(
            error,
            ProtocolError::SemanticViolation("pinch val4 must be zero")
        );
    }
}
