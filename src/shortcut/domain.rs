use serde::{Deserialize, Serialize};

use crate::protocol::{GestureFrame, GestureType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    Alt,
    AltLeft,
    AltRight,
    Space,
    Shift,
    ShiftLeft,
    ShiftRight,
    Control,
    ControlLeft,
    ControlRight,
    MetaLeft,
    MetaRight,
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,
    ContextMenu,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    BracketLeft,
    BracketRight,
    Backquote,
    Minus,
    Equal,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,
    VolumeMute,
    VolumeDown,
    VolumeUp,
    MediaPreviousTrack,
    MediaNextTrack,
    MediaPlayPause,
    MediaStop,
    BrowserBack,
    BrowserForward,
    BrowserRefresh,
    BrowserStop,
    BrowserSearch,
    BrowserFavorites,
    BrowserHome,
}

impl KeyCode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Alt => "Alt",
            Self::AltLeft => "左 Alt",
            Self::AltRight => "右 Alt",
            Self::Space => "Space",
            Self::Shift => "Shift",
            Self::ShiftLeft => "左 Shift",
            Self::ShiftRight => "右 Shift",
            Self::Control => "Ctrl",
            Self::ControlLeft => "左 Ctrl",
            Self::ControlRight => "右 Ctrl",
            Self::MetaLeft => "左 Win",
            Self::MetaRight => "右 Win",
            Self::Enter => "Enter",
            Self::Tab => "Tab",
            Self::Escape => "Esc",
            Self::Backspace => "Backspace",
            Self::Delete => "Delete",
            Self::Insert => "Insert",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "Page Up",
            Self::PageDown => "Page Down",
            Self::ArrowUp => "↑",
            Self::ArrowDown => "↓",
            Self::ArrowLeft => "←",
            Self::ArrowRight => "→",
            Self::CapsLock => "Caps Lock",
            Self::NumLock => "Num Lock",
            Self::ScrollLock => "Scroll Lock",
            Self::PrintScreen => "Print Screen",
            Self::Pause => "Pause",
            Self::ContextMenu => "菜单键",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
            Self::H => "H",
            Self::I => "I",
            Self::J => "J",
            Self::K => "K",
            Self::L => "L",
            Self::M => "M",
            Self::N => "N",
            Self::O => "O",
            Self::P => "P",
            Self::Q => "Q",
            Self::R => "R",
            Self::S => "S",
            Self::T => "T",
            Self::U => "U",
            Self::V => "V",
            Self::W => "W",
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
            Self::Digit0 => "0",
            Self::Digit1 => "1",
            Self::Digit2 => "2",
            Self::Digit3 => "3",
            Self::Digit4 => "4",
            Self::Digit5 => "5",
            Self::Digit6 => "6",
            Self::Digit7 => "7",
            Self::Digit8 => "8",
            Self::Digit9 => "9",
            Self::BracketLeft => "[",
            Self::BracketRight => "]",
            Self::Backquote => "`",
            Self::Minus => "-",
            Self::Equal => "=",
            Self::Backslash => "\\",
            Self::Semicolon => ";",
            Self::Quote => "'",
            Self::Comma => ",",
            Self::Period => ".",
            Self::Slash => "/",
            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::F6 => "F6",
            Self::F7 => "F7",
            Self::F8 => "F8",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
            Self::F13 => "F13",
            Self::F14 => "F14",
            Self::F15 => "F15",
            Self::F16 => "F16",
            Self::F17 => "F17",
            Self::F18 => "F18",
            Self::F19 => "F19",
            Self::F20 => "F20",
            Self::F21 => "F21",
            Self::F22 => "F22",
            Self::F23 => "F23",
            Self::F24 => "F24",
            Self::Numpad0 => "Num 0",
            Self::Numpad1 => "Num 1",
            Self::Numpad2 => "Num 2",
            Self::Numpad3 => "Num 3",
            Self::Numpad4 => "Num 4",
            Self::Numpad5 => "Num 5",
            Self::Numpad6 => "Num 6",
            Self::Numpad7 => "Num 7",
            Self::Numpad8 => "Num 8",
            Self::Numpad9 => "Num 9",
            Self::NumpadAdd => "Num +",
            Self::NumpadSubtract => "Num -",
            Self::NumpadMultiply => "Num *",
            Self::NumpadDivide => "Num /",
            Self::NumpadDecimal => "Num .",
            Self::NumpadEnter => "Num Enter",
            Self::VolumeMute => "静音",
            Self::VolumeDown => "音量 -",
            Self::VolumeUp => "音量 +",
            Self::MediaPreviousTrack => "上一曲",
            Self::MediaNextTrack => "下一曲",
            Self::MediaPlayPause => "播放/暂停",
            Self::MediaStop => "停止",
            Self::BrowserBack => "浏览器后退",
            Self::BrowserForward => "浏览器前进",
            Self::BrowserRefresh => "浏览器刷新",
            Self::BrowserStop => "浏览器停止",
            Self::BrowserSearch => "浏览器搜索",
            Self::BrowserFavorites => "浏览器收藏",
            Self::BrowserHome => "浏览器主页",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Alt" => Some(Self::Alt),
            "左 Alt" => Some(Self::AltLeft),
            "右 Alt" => Some(Self::AltRight),
            "Space" => Some(Self::Space),
            "Shift" => Some(Self::Shift),
            "左 Shift" => Some(Self::ShiftLeft),
            "右 Shift" => Some(Self::ShiftRight),
            "Ctrl" => Some(Self::Control),
            "左 Ctrl" => Some(Self::ControlLeft),
            "右 Ctrl" => Some(Self::ControlRight),
            "左 Win" => Some(Self::MetaLeft),
            "右 Win" => Some(Self::MetaRight),
            "Enter" => Some(Self::Enter),
            "Tab" => Some(Self::Tab),
            "Esc" => Some(Self::Escape),
            "Backspace" => Some(Self::Backspace),
            "Delete" => Some(Self::Delete),
            "Insert" => Some(Self::Insert),
            "Home" => Some(Self::Home),
            "End" => Some(Self::End),
            "Page Up" => Some(Self::PageUp),
            "Page Down" => Some(Self::PageDown),
            "↑" => Some(Self::ArrowUp),
            "↓" => Some(Self::ArrowDown),
            "←" => Some(Self::ArrowLeft),
            "→" => Some(Self::ArrowRight),
            "Caps Lock" => Some(Self::CapsLock),
            "Num Lock" => Some(Self::NumLock),
            "Scroll Lock" => Some(Self::ScrollLock),
            "Print Screen" => Some(Self::PrintScreen),
            "Pause" => Some(Self::Pause),
            "菜单键" => Some(Self::ContextMenu),
            "A" => Some(Self::A),
            "B" => Some(Self::B),
            "C" => Some(Self::C),
            "D" => Some(Self::D),
            "E" => Some(Self::E),
            "F" => Some(Self::F),
            "G" => Some(Self::G),
            "H" => Some(Self::H),
            "I" => Some(Self::I),
            "J" => Some(Self::J),
            "K" => Some(Self::K),
            "L" => Some(Self::L),
            "M" => Some(Self::M),
            "N" => Some(Self::N),
            "O" => Some(Self::O),
            "P" => Some(Self::P),
            "Q" => Some(Self::Q),
            "R" => Some(Self::R),
            "S" => Some(Self::S),
            "T" => Some(Self::T),
            "U" => Some(Self::U),
            "V" => Some(Self::V),
            "W" => Some(Self::W),
            "X" => Some(Self::X),
            "Y" => Some(Self::Y),
            "Z" => Some(Self::Z),
            "0" => Some(Self::Digit0),
            "1" => Some(Self::Digit1),
            "2" => Some(Self::Digit2),
            "3" => Some(Self::Digit3),
            "4" => Some(Self::Digit4),
            "5" => Some(Self::Digit5),
            "6" => Some(Self::Digit6),
            "7" => Some(Self::Digit7),
            "8" => Some(Self::Digit8),
            "9" => Some(Self::Digit9),
            "[" => Some(Self::BracketLeft),
            "]" => Some(Self::BracketRight),
            "`" => Some(Self::Backquote),
            "-" => Some(Self::Minus),
            "=" => Some(Self::Equal),
            "\\" => Some(Self::Backslash),
            ";" => Some(Self::Semicolon),
            "'" => Some(Self::Quote),
            "," => Some(Self::Comma),
            "." => Some(Self::Period),
            "/" => Some(Self::Slash),
            "F1" => Some(Self::F1),
            "F2" => Some(Self::F2),
            "F3" => Some(Self::F3),
            "F4" => Some(Self::F4),
            "F5" => Some(Self::F5),
            "F6" => Some(Self::F6),
            "F7" => Some(Self::F7),
            "F8" => Some(Self::F8),
            "F9" => Some(Self::F9),
            "F10" => Some(Self::F10),
            "F11" => Some(Self::F11),
            "F12" => Some(Self::F12),
            "F13" => Some(Self::F13),
            "F14" => Some(Self::F14),
            "F15" => Some(Self::F15),
            "F16" => Some(Self::F16),
            "F17" => Some(Self::F17),
            "F18" => Some(Self::F18),
            "F19" => Some(Self::F19),
            "F20" => Some(Self::F20),
            "F21" => Some(Self::F21),
            "F22" => Some(Self::F22),
            "F23" => Some(Self::F23),
            "F24" => Some(Self::F24),
            "Num 0" => Some(Self::Numpad0),
            "Num 1" => Some(Self::Numpad1),
            "Num 2" => Some(Self::Numpad2),
            "Num 3" => Some(Self::Numpad3),
            "Num 4" => Some(Self::Numpad4),
            "Num 5" => Some(Self::Numpad5),
            "Num 6" => Some(Self::Numpad6),
            "Num 7" => Some(Self::Numpad7),
            "Num 8" => Some(Self::Numpad8),
            "Num 9" => Some(Self::Numpad9),
            "Num +" => Some(Self::NumpadAdd),
            "Num -" => Some(Self::NumpadSubtract),
            "Num *" => Some(Self::NumpadMultiply),
            "Num /" => Some(Self::NumpadDivide),
            "Num ." => Some(Self::NumpadDecimal),
            "Num Enter" => Some(Self::NumpadEnter),
            "静音" => Some(Self::VolumeMute),
            "音量 -" => Some(Self::VolumeDown),
            "音量 +" => Some(Self::VolumeUp),
            "上一曲" => Some(Self::MediaPreviousTrack),
            "下一曲" => Some(Self::MediaNextTrack),
            "播放/暂停" => Some(Self::MediaPlayPause),
            "停止" => Some(Self::MediaStop),
            "浏览器后退" => Some(Self::BrowserBack),
            "浏览器前进" => Some(Self::BrowserForward),
            "浏览器刷新" => Some(Self::BrowserRefresh),
            "浏览器停止" => Some(Self::BrowserStop),
            "浏览器搜索" => Some(Self::BrowserSearch),
            "浏览器收藏" => Some(Self::BrowserFavorites),
            "浏览器主页" => Some(Self::BrowserHome),
            _ => None,
        }
    }

    pub fn sort_rank(self) -> u16 {
        match self {
            Self::Control => 0,
            Self::ControlLeft => 1,
            Self::ControlRight => 2,
            Self::Shift => 3,
            Self::ShiftLeft => 4,
            Self::ShiftRight => 5,
            Self::Alt => 6,
            Self::AltLeft => 7,
            Self::AltRight => 8,
            Self::MetaLeft => 9,
            Self::MetaRight => 10,
            _ => 100 + self.virtual_key(),
        }
    }

    pub fn virtual_key(self) -> u16 {
        match self {
            Self::Backspace => 0x08,
            Self::Tab => 0x09,
            Self::Enter | Self::NumpadEnter => 0x0D,
            Self::Shift => 0x10,
            Self::Control => 0x11,
            Self::Alt => 0x12,
            Self::Pause => 0x13,
            Self::CapsLock => 0x14,
            Self::Escape => 0x1B,
            Self::Space => 0x20,
            Self::PageUp => 0x21,
            Self::PageDown => 0x22,
            Self::End => 0x23,
            Self::Home => 0x24,
            Self::ArrowLeft => 0x25,
            Self::ArrowUp => 0x26,
            Self::ArrowRight => 0x27,
            Self::ArrowDown => 0x28,
            Self::PrintScreen => 0x2C,
            Self::Insert => 0x2D,
            Self::Delete => 0x2E,
            Self::Digit0 => 0x30,
            Self::Digit1 => 0x31,
            Self::Digit2 => 0x32,
            Self::Digit3 => 0x33,
            Self::Digit4 => 0x34,
            Self::Digit5 => 0x35,
            Self::Digit6 => 0x36,
            Self::Digit7 => 0x37,
            Self::Digit8 => 0x38,
            Self::Digit9 => 0x39,
            Self::A => 0x41,
            Self::B => 0x42,
            Self::C => 0x43,
            Self::D => 0x44,
            Self::E => 0x45,
            Self::F => 0x46,
            Self::G => 0x47,
            Self::H => 0x48,
            Self::I => 0x49,
            Self::J => 0x4A,
            Self::K => 0x4B,
            Self::L => 0x4C,
            Self::M => 0x4D,
            Self::N => 0x4E,
            Self::O => 0x4F,
            Self::P => 0x50,
            Self::Q => 0x51,
            Self::R => 0x52,
            Self::S => 0x53,
            Self::T => 0x54,
            Self::U => 0x55,
            Self::V => 0x56,
            Self::W => 0x57,
            Self::X => 0x58,
            Self::Y => 0x59,
            Self::Z => 0x5A,
            Self::MetaLeft => 0x5B,
            Self::MetaRight => 0x5C,
            Self::ContextMenu => 0x5D,
            Self::Numpad0 => 0x60,
            Self::Numpad1 => 0x61,
            Self::Numpad2 => 0x62,
            Self::Numpad3 => 0x63,
            Self::Numpad4 => 0x64,
            Self::Numpad5 => 0x65,
            Self::Numpad6 => 0x66,
            Self::Numpad7 => 0x67,
            Self::Numpad8 => 0x68,
            Self::Numpad9 => 0x69,
            Self::NumpadMultiply => 0x6A,
            Self::NumpadAdd => 0x6B,
            Self::NumpadSubtract => 0x6D,
            Self::NumpadDecimal => 0x6E,
            Self::NumpadDivide => 0x6F,
            Self::F1 => 0x70,
            Self::F2 => 0x71,
            Self::F3 => 0x72,
            Self::F4 => 0x73,
            Self::F5 => 0x74,
            Self::F6 => 0x75,
            Self::F7 => 0x76,
            Self::F8 => 0x77,
            Self::F9 => 0x78,
            Self::F10 => 0x79,
            Self::F11 => 0x7A,
            Self::F12 => 0x7B,
            Self::F13 => 0x7C,
            Self::F14 => 0x7D,
            Self::F15 => 0x7E,
            Self::F16 => 0x7F,
            Self::F17 => 0x80,
            Self::F18 => 0x81,
            Self::F19 => 0x82,
            Self::F20 => 0x83,
            Self::F21 => 0x84,
            Self::F22 => 0x85,
            Self::F23 => 0x86,
            Self::F24 => 0x87,
            Self::NumLock => 0x90,
            Self::ScrollLock => 0x91,
            Self::ShiftLeft => 0xA0,
            Self::ShiftRight => 0xA1,
            Self::ControlLeft => 0xA2,
            Self::ControlRight => 0xA3,
            Self::AltLeft => 0xA4,
            Self::AltRight => 0xA5,
            Self::BrowserBack => 0xA6,
            Self::BrowserForward => 0xA7,
            Self::BrowserRefresh => 0xA8,
            Self::BrowserStop => 0xA9,
            Self::BrowserSearch => 0xAA,
            Self::BrowserFavorites => 0xAB,
            Self::BrowserHome => 0xAC,
            Self::VolumeMute => 0xAD,
            Self::VolumeDown => 0xAE,
            Self::VolumeUp => 0xAF,
            Self::MediaNextTrack => 0xB0,
            Self::MediaPreviousTrack => 0xB1,
            Self::MediaStop => 0xB2,
            Self::MediaPlayPause => 0xB3,
            Self::Semicolon => 0xBA,
            Self::Equal => 0xBB,
            Self::Comma => 0xBC,
            Self::Minus => 0xBD,
            Self::Period => 0xBE,
            Self::Slash => 0xBF,
            Self::Backquote => 0xC0,
            Self::BracketLeft => 0xDB,
            Self::Backslash => 0xDC,
            Self::BracketRight => 0xDD,
            Self::Quote => 0xDE,
        }
    }

    pub fn is_extended(self) -> bool {
        matches!(
            self,
            Self::AltRight
                | Self::ControlRight
                | Self::MetaLeft
                | Self::MetaRight
                | Self::ContextMenu
                | Self::Insert
                | Self::Delete
                | Self::Home
                | Self::End
                | Self::PageUp
                | Self::PageDown
                | Self::ArrowUp
                | Self::ArrowDown
                | Self::ArrowLeft
                | Self::ArrowRight
                | Self::PrintScreen
                | Self::NumLock
                | Self::NumpadDivide
                | Self::NumpadEnter
                | Self::BrowserBack
                | Self::BrowserForward
                | Self::BrowserRefresh
                | Self::BrowserStop
                | Self::BrowserSearch
                | Self::BrowserFavorites
                | Self::BrowserHome
                | Self::VolumeMute
                | Self::VolumeDown
                | Self::VolumeUp
                | Self::MediaPreviousTrack
                | Self::MediaNextTrack
                | Self::MediaPlayPause
                | Self::MediaStop
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwipeAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StylusTrigger {
    Squeeze,
    DoubleTap,
    TwoTap,
    ThreeTap,
    FourTap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GestureBinding {
    TwoPan,
    ThreePan,
    TwoPinch,
    TwoRotate,
    LongPress { fingers: u8 },
    Swipe { fingers: u8, axis: SwipeAxis },
}

impl GestureBinding {
    pub fn from_frame(frame: &GestureFrame) -> Option<Self> {
        match frame.gesture_type {
            GestureType::TwoPan => Some(Self::TwoPan),
            GestureType::ThreePan => Some(Self::ThreePan),
            GestureType::TwoPinch => Some(Self::TwoPinch),
            GestureType::TwoRotate => Some(Self::TwoRotate),
            GestureType::OneLongPress => Some(Self::LongPress { fingers: 1 }),
            GestureType::TwoLongPress => Some(Self::LongPress { fingers: 2 }),
            GestureType::ThreeLongPress => Some(Self::LongPress { fingers: 3 }),
            GestureType::FourLongPress => Some(Self::LongPress { fingers: 4 }),
            GestureType::OneSwipe => Some(Self::Swipe {
                fingers: 1,
                axis: swipe_axis(frame.val1),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BindingId {
    StylusTrigger(StylusTrigger),
    Gesture(GestureBinding),
}

impl BindingId {
    pub fn persisted_key(self) -> String {
        match self {
            Self::StylusTrigger(trigger) => match trigger {
                StylusTrigger::Squeeze => "stylus:squeeze".to_string(),
                StylusTrigger::DoubleTap => "stylus:double_tap".to_string(),
                StylusTrigger::TwoTap => "stylus:two_tap".to_string(),
                StylusTrigger::ThreeTap => "stylus:three_tap".to_string(),
                StylusTrigger::FourTap => "stylus:four_tap".to_string(),
            },
            Self::Gesture(gesture) => match gesture {
                GestureBinding::TwoPan => "gesture:two_pan".to_string(),
                GestureBinding::ThreePan => "gesture:three_pan".to_string(),
                GestureBinding::TwoPinch => "gesture:two_pinch".to_string(),
                GestureBinding::TwoRotate => "gesture:two_rotate".to_string(),
                GestureBinding::LongPress { fingers } => format!("gesture:long_press:{fingers}"),
                GestureBinding::Swipe { fingers, axis } => {
                    let axis = match axis {
                        SwipeAxis::Horizontal => "horizontal",
                        SwipeAxis::Vertical => "vertical",
                    };
                    format!("gesture:swipe:{fingers}:{axis}")
                }
            },
        }
    }

    pub fn parse_persisted_key(value: &str) -> Option<Self> {
        match value {
            "stylus:squeeze" => Some(Self::StylusTrigger(StylusTrigger::Squeeze)),
            "stylus:double_tap" => Some(Self::StylusTrigger(StylusTrigger::DoubleTap)),
            "stylus:two_tap" => Some(Self::StylusTrigger(StylusTrigger::TwoTap)),
            "stylus:three_tap" => Some(Self::StylusTrigger(StylusTrigger::ThreeTap)),
            "stylus:four_tap" => Some(Self::StylusTrigger(StylusTrigger::FourTap)),
            "gesture:two_pan" => Some(Self::Gesture(GestureBinding::TwoPan)),
            "gesture:three_pan" => Some(Self::Gesture(GestureBinding::ThreePan)),
            "gesture:two_pinch" => Some(Self::Gesture(GestureBinding::TwoPinch)),
            "gesture:two_rotate" => Some(Self::Gesture(GestureBinding::TwoRotate)),
            "gesture:long_press:1" => Some(Self::Gesture(GestureBinding::LongPress { fingers: 1 })),
            "gesture:long_press:2" => Some(Self::Gesture(GestureBinding::LongPress { fingers: 2 })),
            "gesture:long_press:3" => Some(Self::Gesture(GestureBinding::LongPress { fingers: 3 })),
            "gesture:long_press:4" => Some(Self::Gesture(GestureBinding::LongPress { fingers: 4 })),
            "gesture:swipe:1:horizontal" => Some(Self::Gesture(GestureBinding::Swipe {
                fingers: 1,
                axis: SwipeAxis::Horizontal,
            })),
            "gesture:swipe:1:vertical" => Some(Self::Gesture(GestureBinding::Swipe {
                fingers: 1,
                axis: SwipeAxis::Vertical,
            })),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerAnchor {
    CurrentHoverOrLastInRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RadialInnerSlot {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadialInnerBindings {
    pub top: KeyCode,
    pub bottom: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
}

impl Default for RadialInnerBindings {
    fn default() -> Self {
        Self {
            top: KeyCode::Shift,
            bottom: KeyCode::Control,
            left: KeyCode::Space,
            right: KeyCode::Alt,
        }
    }
}

impl RadialInnerBindings {
    pub fn key_for_slot(&self, slot: RadialInnerSlot) -> KeyCode {
        match slot {
            RadialInnerSlot::Top => self.top,
            RadialInnerSlot::Bottom => self.bottom,
            RadialInnerSlot::Left => self.left,
            RadialInnerSlot::Right => self.right,
        }
    }

    pub fn slot_entries(&self) -> [(RadialInnerSlot, KeyCode); 4] {
        [
            (RadialInnerSlot::Top, self.top),
            (RadialInnerSlot::Right, self.right),
            (RadialInnerSlot::Bottom, self.bottom),
            (RadialInnerSlot::Left, self.left),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadialOuterBinding {
    pub keys: Vec<KeyCode>,
}

impl RadialOuterBinding {
    pub fn new(keys: Vec<KeyCode>) -> Self {
        Self { keys }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RadialMenuConfig {
    pub inner_enabled: bool,
    pub inner: RadialInnerBindings,
    pub outer: [RadialOuterBinding; 8],
}

impl Default for RadialMenuConfig {
    fn default() -> Self {
        Self {
            inner_enabled: true,
            inner: RadialInnerBindings::default(),
            outer: [
                // 上 → 顺时针: V, M, ], P, L, Ctrl+Shift+N, [, Ctrl+T
                RadialOuterBinding::new(vec![KeyCode::V]),
                RadialOuterBinding::new(vec![KeyCode::M]),
                RadialOuterBinding::new(vec![KeyCode::BracketRight]),
                RadialOuterBinding::new(vec![KeyCode::P]),
                RadialOuterBinding::new(vec![KeyCode::L]),
                RadialOuterBinding::new(vec![KeyCode::Control, KeyCode::Shift, KeyCode::N]),
                RadialOuterBinding::new(vec![KeyCode::BracketLeft]),
                RadialOuterBinding::new(vec![KeyCode::Control, KeyCode::T]),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdvancedAction {
    PointerDrag {
        modifiers: Vec<KeyCode>,
        button: Option<MouseButton>,
    },
    PointerWheel {
        modifiers: Vec<KeyCode>,
    },
    PointerRotate {
        modifiers: Vec<KeyCode>,
    },
    PointerClick {
        keys: Vec<KeyCode>,
        button: MouseButton,
        anchor: PointerAnchor,
    },
    ReleaseActiveKeys,
    ReservedRadialMenu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShortcutAction {
    HoldKeys(Vec<KeyCode>),
    TriggerChord(Vec<KeyCode>),
    Advanced(AdvancedAction),
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialAction {
    None,
    RadialMenu,
    PointerClickLeft,
    PointerClickRight,
    PointerMove,
    PointerDragLeft,
    PointerDragRight,
    PointerWheel,
    PointerRotate,
}

impl SpecialAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "radialMenu" => Some(Self::RadialMenu),
            "pointerClickLeft" => Some(Self::PointerClickLeft),
            "pointerClickRight" => Some(Self::PointerClickRight),
            "pointerMove" => Some(Self::PointerMove),
            "pointerDragLeft" => Some(Self::PointerDragLeft),
            "pointerDragRight" => Some(Self::PointerDragRight),
            "pointerWheel" => Some(Self::PointerWheel),
            "pointerRotate" => Some(Self::PointerRotate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutCommand {
    KeyDown(KeyCode),
    KeyUp(KeyCode),
    PressChord(Vec<KeyCode>),
    MouseMoveRelative { dx: i32, dy: i32 },
    MouseWheel { delta: i32 },
    MouseButtonDown(MouseButton),
    MouseButtonUp(MouseButton),
    ClickAt { button: MouseButton, x: i32, y: i32 },
}

fn swipe_axis(value: f32) -> SwipeAxis {
    if value >= 0.5 {
        SwipeAxis::Vertical
    } else {
        SwipeAxis::Horizontal
    }
}

#[cfg(test)]
mod tests {
    use super::KeyCode;

    #[test]
    fn expanded_key_labels_round_trip_through_desktop_protocol() {
        for key in [
            KeyCode::ControlRight,
            KeyCode::MetaLeft,
            KeyCode::ArrowUp,
            KeyCode::F24,
            KeyCode::NumpadEnter,
            KeyCode::Semicolon,
            KeyCode::MediaPlayPause,
            KeyCode::BrowserBack,
        ] {
            assert_eq!(KeyCode::parse(key.label()), Some(key));
        }
    }

    #[test]
    fn windows_key_facts_distinguish_extended_keys() {
        assert_eq!(
            KeyCode::NumpadEnter.virtual_key(),
            KeyCode::Enter.virtual_key()
        );
        assert!(KeyCode::NumpadEnter.is_extended());
        assert!(!KeyCode::Enter.is_extended());
        assert_eq!(KeyCode::F24.virtual_key(), 0x87);
        assert_eq!(KeyCode::MediaPlayPause.virtual_key(), 0xB3);
    }
}
