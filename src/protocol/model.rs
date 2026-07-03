use crate::protocol::constants::{PROTOCOL_MAGIC, PROTOCOL_VERSION, STYLUS_FLAGS_RESERVED_MASK};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketHeader {
    pub magic: u32,
    pub packet_type: PacketType,
    pub protocol_version: u8,
    pub reserved: u16,
}

impl PacketHeader {
    pub fn new(packet_type: PacketType) -> Self {
        Self {
            magic: PROTOCOL_MAGIC,
            packet_type,
            protocol_version: PROTOCOL_VERSION,
            reserved: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    HandshakeRequest = 1,
    HandshakeResponse = 2,
    HandshakeError = 3,
    SessionDisconnect = 4,
    StylusFrame = 5,
    GestureFrame = 6,
}

impl TryFrom<u8> for PacketType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::HandshakeRequest),
            2 => Ok(Self::HandshakeResponse),
            3 => Ok(Self::HandshakeError),
            4 => Ok(Self::SessionDisconnect),
            5 => Ok(Self::StylusFrame),
            6 => Ok(Self::GestureFrame),
            _ => Err(()),
        }
    }
}

impl From<PacketType> for u8 {
    fn from(value: PacketType) -> Self {
        value as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DesktopUnit {
    Pixel = 1,
}

impl TryFrom<u8> for DesktopUnit {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Pixel),
            _ => Err(()),
        }
    }
}

impl From<DesktopUnit> for u8 {
    fn from(value: DesktopUnit) -> Self {
        value as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum HandshakeErrorCode {
    InvalidRequest = 1,
    UnsupportedProtocol = 2,
    AlreadyConnected = 3,
    NoActiveWorkspace = 4,
    InternalError = 5,
}

impl TryFrom<u16> for HandshakeErrorCode {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::InvalidRequest),
            2 => Ok(Self::UnsupportedProtocol),
            3 => Ok(Self::AlreadyConnected),
            4 => Ok(Self::NoActiveWorkspace),
            5 => Ok(Self::InternalError),
            _ => Err(()),
        }
    }
}

impl From<HandshakeErrorCode> for u16 {
    fn from(value: HandshakeErrorCode) -> Self {
        value as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StylusEventType {
    Cancel = 0,
    Down = 1,
    Move = 2,
    Up = 3,
}

impl TryFrom<u8> for StylusEventType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Cancel),
            1 => Ok(Self::Down),
            2 => Ok(Self::Move),
            3 => Ok(Self::Up),
            _ => Err(()),
        }
    }
}

impl From<StylusEventType> for u8 {
    fn from(value: StylusEventType) -> Self {
        value as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GestureType {
    TwoPan = 1,
    ThreePan = 2,
    TwoPinch = 3,
    TwoRotate = 4,
    OneLongPress = 5,
    TwoLongPress = 6,
    ThreeLongPress = 7,
    FourLongPress = 8,
    OneSwipe = 9,
}

impl GestureType {
    pub fn is_swipe(self) -> bool {
        matches!(self, Self::OneSwipe)
    }

    pub fn is_rotate(self) -> bool {
        matches!(self, Self::TwoRotate)
    }

    pub fn is_long_press(self) -> bool {
        matches!(
            self,
            Self::OneLongPress | Self::TwoLongPress | Self::ThreeLongPress | Self::FourLongPress
        )
    }

    pub fn is_pinch(self) -> bool {
        matches!(self, Self::TwoPinch)
    }
}

impl TryFrom<u8> for GestureType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::TwoPan),
            2 => Ok(Self::ThreePan),
            3 => Ok(Self::TwoPinch),
            4 => Ok(Self::TwoRotate),
            5 => Ok(Self::OneLongPress),
            6 => Ok(Self::TwoLongPress),
            7 => Ok(Self::ThreeLongPress),
            8 => Ok(Self::FourLongPress),
            9 => Ok(Self::OneSwipe),
            _ => Err(()),
        }
    }
}

impl From<GestureType> for u8 {
    fn from(value: GestureType) -> Self {
        value as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GestureState {
    Begin = 1,
    Update = 2,
    End = 3,
}

impl TryFrom<u8> for GestureState {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Begin),
            2 => Ok(Self::Update),
            3 => Ok(Self::End),
            _ => Err(()),
        }
    }
}

impl From<GestureState> for u8 {
    fn from(value: GestureState) -> Self {
        value as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StylusFlags(pub u8);

impl StylusFlags {
    pub fn has_reserved_bits(self) -> bool {
        self.0 & STYLUS_FLAGS_RESERVED_MASK != 0
    }

    pub fn in_range(self) -> bool {
        self.0 & 0b0000_0001 != 0
    }

    pub fn is_contact(self) -> bool {
        self.0 & 0b0000_0010 != 0
    }

    pub fn squeeze(self) -> bool {
        self.0 & 0b0000_0100 != 0
    }

    pub fn double_tap(self) -> bool {
        self.0 & 0b0000_1000 != 0
    }

    pub fn two_tap(self) -> bool {
        self.0 & 0b0001_0000 != 0
    }

    pub fn three_tap(self) -> bool {
        self.0 & 0b0010_0000 != 0
    }

    pub fn four_tap(self) -> bool {
        self.0 & 0b0100_0000 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeRequest {
    pub client_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeResponse {
    pub session_id: String,
    pub desktop_width_px: u32,
    pub desktop_height_px: u32,
    pub desktop_unit: DesktopUnit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeError {
    pub error_code: HandshakeErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDisconnect {
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StylusFrame {
    pub seq: u32,
    pub timestamp: u64,
    pub x: u16,
    pub y: u16,
    pub pressure: f32,
    pub tilt_x: i8,
    pub tilt_y: i8,
    pub event_type: StylusEventType,
    pub flags: StylusFlags,
    pub reserved_ext: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GestureFrame {
    pub gesture_type: GestureType,
    pub state: GestureState,
    pub seq: u32,
    pub timestamp: u64,
    pub val1: f32,
    pub val2: f32,
    pub val3: f32,
    pub val4: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    HandshakeRequest(HandshakeRequest),
    HandshakeResponse(HandshakeResponse),
    HandshakeError(HandshakeError),
    SessionDisconnect(SessionDisconnect),
    StylusFrame(StylusFrame),
    GestureFrame(GestureFrame),
}
