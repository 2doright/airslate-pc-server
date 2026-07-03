use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ProtocolError {
    #[error("packet is too short: expected at least {expected} bytes, got {actual}")]
    PacketTooShort { expected: usize, actual: usize },
    #[error("invalid packet length for {packet}: expected {expected} bytes, got {actual}")]
    InvalidPacketLength {
        packet: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("invalid packet magic: expected 0x41534C54, got 0x{actual:08X}")]
    InvalidMagic { actual: u32 },
    #[error("unknown packet type: {0}")]
    UnknownPacketType(u8),
    #[error("unsupported protocol version: {0}")]
    UnsupportedProtocolVersion(u8),
    #[error("non-zero reserved field {field}: {value}")]
    NonZeroReserved { field: &'static str, value: u32 },
    #[error("invalid utf-8 in {field}")]
    InvalidUtf8 { field: &'static str },
    #[error("invalid zero padding in {field}")]
    InvalidPadding { field: &'static str },
    #[error("unknown enum value for {field}: {value}")]
    UnknownEnumValue { field: &'static str, value: u32 },
    #[error("out of range field {field}: {details}")]
    OutOfRange {
        field: &'static str,
        details: &'static str,
    },
    #[error("invalid float in {field}")]
    InvalidFloat { field: &'static str },
    #[error("protocol constraint violated: {0}")]
    SemanticViolation(&'static str),
}
