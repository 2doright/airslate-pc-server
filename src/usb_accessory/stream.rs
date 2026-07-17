use std::fmt;

use crate::protocol::{
    GESTURE_FRAME_SIZE, HANDSHAKE_ERROR_SIZE, HANDSHAKE_REQUEST_SIZE, HANDSHAKE_RESPONSE_SIZE,
    PROTOCOL_MAGIC, SESSION_DISCONNECT_SIZE, STYLUS_FRAME_SIZE,
};

#[derive(Debug, Default)]
pub struct PacketStream {
    buffered: Vec<u8>,
}

impl PacketStream {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffered.extend_from_slice(bytes);
    }

    pub(super) fn buffered_len(&self) -> usize {
        self.buffered.len()
    }

    pub fn next_packet(&mut self) -> Result<Option<Vec<u8>>, StreamError> {
        if self.buffered.len() < 5 {
            return Ok(None);
        }
        let magic = u32::from_le_bytes(self.buffered[0..4].try_into().expect("four bytes"));
        if magic != PROTOCOL_MAGIC {
            return Err(StreamError::InvalidMagic(magic));
        }
        let packet_len = match self.buffered[4] {
            1 => HANDSHAKE_REQUEST_SIZE,
            2 => HANDSHAKE_RESPONSE_SIZE,
            3 => HANDSHAKE_ERROR_SIZE,
            4 => SESSION_DISCONNECT_SIZE,
            5 => STYLUS_FRAME_SIZE,
            6 => GESTURE_FRAME_SIZE,
            packet_type => return Err(StreamError::UnknownPacketType(packet_type)),
        };
        if self.buffered.len() < packet_len {
            return Ok(None);
        }
        Ok(Some(self.buffered.drain(..packet_len).collect()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamError {
    InvalidMagic(u32),
    UnknownPacketType(u8),
}

impl fmt::Display for StreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic(magic) => write!(formatter, "invalid ASLT magic 0x{magic:08X}"),
            Self::UnknownPacketType(packet_type) => {
                write!(formatter, "unknown ASLT packet type {packet_type}")
            }
        }
    }
}

impl std::error::Error for StreamError {}

#[cfg(test)]
mod tests {
    use crate::protocol::{
        GestureFrame, GestureState, GestureType, HandshakeRequest, Packet, StylusEventType,
        StylusFlags, StylusFrame, encode_packet,
    };

    use super::*;

    #[test]
    fn waits_for_a_short_handshake_read() {
        let encoded = encode_packet(&Packet::HandshakeRequest(HandshakeRequest {
            client_id: "tablet".to_owned(),
        }))
        .unwrap();
        let mut stream = PacketStream::default();
        stream.push(&encoded[..17]);
        assert_eq!(stream.buffered_len(), 17);
        assert_eq!(stream.next_packet().unwrap(), None);
        stream.push(&encoded[17..]);
        assert_eq!(stream.buffered_len(), encoded.len());
        assert_eq!(stream.next_packet().unwrap(), Some(encoded));
        assert_eq!(stream.buffered_len(), 0);
    }

    #[test]
    fn splits_coalesced_stylus_and_gesture_packets() {
        let stylus = encode_packet(&Packet::StylusFrame(StylusFrame {
            seq: 40,
            timestamp: 1,
            x: 2,
            y: 3,
            pressure: 0.5,
            tilt_x: 0,
            tilt_y: 0,
            event_type: StylusEventType::Move,
            flags: StylusFlags(0),
            reserved_ext: 0,
        }))
        .unwrap();
        let gesture = encode_packet(&Packet::GestureFrame(GestureFrame {
            gesture_type: GestureType::TwoPan,
            state: GestureState::Update,
            seq: 41,
            timestamp: 2,
            val1: 1.0,
            val2: 2.0,
            val3: 3.0,
            val4: 4.0,
        }))
        .unwrap();
        let mut stream = PacketStream::default();
        stream.push(&[stylus.clone(), gesture.clone()].concat());
        assert_eq!(stream.next_packet().unwrap(), Some(stylus));
        assert_eq!(stream.next_packet().unwrap(), Some(gesture));
        assert_eq!(stream.next_packet().unwrap(), None);
    }

    #[test]
    fn invalid_prefix_is_terminal_instead_of_scanned() {
        let mut stream = PacketStream::default();
        stream.push(&[0, 1, 2, 3, 5]);
        assert_eq!(
            stream.next_packet(),
            Err(StreamError::InvalidMagic(0x0302_0100))
        );
    }

    #[test]
    fn unknown_type_is_terminal() {
        let mut stream = PacketStream::default();
        stream.push(&[0x54, 0x4c, 0x53, 0x41, 99]);
        assert_eq!(
            stream.next_packet(),
            Err(StreamError::UnknownPacketType(99))
        );
    }
}
