pub const PROTOCOL_MAGIC: u32 = 0x4153_4C54;
pub const PROTOCOL_VERSION: u8 = 1;

pub const PACKET_HEADER_SIZE: usize = 8;
pub const HANDSHAKE_REQUEST_SIZE: usize = 72;
pub const HANDSHAKE_RESPONSE_SIZE: usize = 81;
pub const HANDSHAKE_ERROR_SIZE: usize = 108;
pub const SESSION_DISCONNECT_SIZE: usize = 72;
pub const STYLUS_FRAME_SIZE: usize = 36;
pub const GESTURE_FRAME_SIZE: usize = 36;

pub const CLIENT_ID_LEN: usize = 64;
pub const SESSION_ID_LEN: usize = 64;
pub const ERROR_MESSAGE_LEN: usize = 96;

pub const STYLUS_FLAGS_RESERVED_MASK: u8 = 0b1000_0000;
