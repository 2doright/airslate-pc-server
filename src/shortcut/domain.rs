use serde::{Deserialize, Serialize};

use crate::protocol::{GestureFrame, GestureType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    Alt,
    Space,
    Shift,
    Control,
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
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
}

impl KeyCode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Alt => "Alt",
            Self::Space => "Space",
            Self::Shift => "Shift",
            Self::Control => "Ctrl",
            Self::Enter => "Enter",
            Self::Tab => "Tab",
            Self::Escape => "Esc",
            Self::Backspace => "Backspace",
            Self::Delete => "Delete",
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
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Alt" => Some(Self::Alt),
            "Space" => Some(Self::Space),
            "Shift" => Some(Self::Shift),
            "Ctrl" => Some(Self::Control),
            "Enter" => Some(Self::Enter),
            "Tab" => Some(Self::Tab),
            "Esc" => Some(Self::Escape),
            "Backspace" => Some(Self::Backspace),
            "Delete" => Some(Self::Delete),
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
            _ => None,
        }
    }

    pub fn sort_rank(self) -> u8 {
        match self {
            Self::Control => 0,
            Self::Shift => 1,
            Self::Alt => 2,
            Self::Space => 3,
            Self::Enter => 4,
            Self::Tab => 5,
            Self::Escape => 6,
            Self::Backspace => 7,
            Self::Delete => 8,
            Self::A => 9,
            Self::B => 10,
            Self::C => 11,
            Self::D => 12,
            Self::E => 13,
            Self::F => 14,
            Self::G => 15,
            Self::H => 16,
            Self::I => 17,
            Self::J => 18,
            Self::K => 19,
            Self::L => 20,
            Self::M => 21,
            Self::N => 22,
            Self::O => 23,
            Self::P => 24,
            Self::Q => 25,
            Self::R => 26,
            Self::S => 27,
            Self::T => 28,
            Self::U => 29,
            Self::V => 30,
            Self::W => 31,
            Self::X => 32,
            Self::Y => 33,
            Self::Z => 34,
            Self::Digit0 => 35,
            Self::Digit1 => 36,
            Self::Digit2 => 37,
            Self::Digit3 => 38,
            Self::Digit4 => 39,
            Self::Digit5 => 40,
            Self::Digit6 => 41,
            Self::Digit7 => 42,
            Self::Digit8 => 43,
            Self::Digit9 => 44,
            Self::BracketLeft => 45,
            Self::BracketRight => 46,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
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
        button: MouseButton,
    },
    PointerWheel {
        modifiers: Vec<KeyCode>,
    },
    PointerRotate {
        modifiers: Vec<KeyCode>,
    },
    SecondaryClick {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutCommand {
    KeyDown(KeyCode),
    KeyUp(KeyCode),
    PressChord(Vec<KeyCode>),
    MouseMoveRelative { dx: i32, dy: i32 },
    MouseWheel { delta: i32 },
    MouseButtonDown(MouseButton),
    MouseButtonUp(MouseButton),
    RightClickAt { x: i32, y: i32 },
}

fn swipe_axis(value: f32) -> SwipeAxis {
    if value >= 0.5 {
        SwipeAxis::Vertical
    } else {
        SwipeAxis::Horizontal
    }
}
