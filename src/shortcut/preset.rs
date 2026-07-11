use super::domain::{
    AdvancedAction, BindingId, GestureBinding, KeyCode, MouseButton, PointerAnchor, ShortcutAction,
    StylusTrigger, SwipeAxis,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultPreset;

impl DefaultPreset {
    pub fn action_for(&self, binding: BindingId) -> ShortcutAction {
        match binding {
            BindingId::StylusTrigger(StylusTrigger::Squeeze) => {
                ShortcutAction::Advanced(AdvancedAction::PointerClick {
                    keys: Vec::new(),
                    button: MouseButton::Right,
                    anchor: PointerAnchor::CurrentHoverOrLastInRange,
                })
            }
            BindingId::StylusTrigger(StylusTrigger::DoubleTap) => {
                ShortcutAction::Advanced(AdvancedAction::PointerClick {
                    keys: Vec::new(),
                    button: MouseButton::Right,
                    anchor: PointerAnchor::CurrentHoverOrLastInRange,
                })
            }
            BindingId::StylusTrigger(StylusTrigger::TwoTap) => {
                ShortcutAction::TriggerChord(vec![KeyCode::Control, KeyCode::Z])
            }
            BindingId::StylusTrigger(StylusTrigger::ThreeTap) => {
                ShortcutAction::TriggerChord(vec![KeyCode::Control, KeyCode::Shift, KeyCode::Z])
            }
            BindingId::StylusTrigger(StylusTrigger::FourTap) => {
                ShortcutAction::Advanced(AdvancedAction::ReleaseActiveKeys)
            }
            BindingId::Gesture(GestureBinding::TwoPan) => {
                ShortcutAction::Advanced(AdvancedAction::ReservedRadialMenu)
            }
            BindingId::Gesture(GestureBinding::ThreePan) => {
                ShortcutAction::Advanced(AdvancedAction::PointerDrag {
                    modifiers: vec![KeyCode::Alt],
                    button: Some(MouseButton::Right),
                })
            }
            BindingId::Gesture(GestureBinding::TwoPinch) => {
                ShortcutAction::Advanced(AdvancedAction::PointerWheel {
                    modifiers: vec![KeyCode::Alt],
                })
            }
            BindingId::Gesture(GestureBinding::TwoRotate) => {
                ShortcutAction::TriggerChord(vec![KeyCode::R])
            }
            BindingId::Gesture(GestureBinding::LongPress { fingers: 1 }) => {
                ShortcutAction::HoldKeys(vec![KeyCode::Alt])
            }
            BindingId::Gesture(GestureBinding::LongPress { fingers: 2 }) => {
                ShortcutAction::HoldKeys(vec![KeyCode::Space])
            }
            BindingId::Gesture(GestureBinding::LongPress { fingers: 3 }) => {
                ShortcutAction::HoldKeys(vec![KeyCode::Shift])
            }
            BindingId::Gesture(GestureBinding::LongPress { fingers: 4 }) => {
                ShortcutAction::HoldKeys(vec![KeyCode::Control])
            }
            BindingId::Gesture(GestureBinding::Swipe {
                fingers: 1,
                axis: SwipeAxis::Horizontal,
            }) => ShortcutAction::TriggerChord(vec![KeyCode::E]),
            BindingId::Gesture(GestureBinding::Swipe {
                fingers: 1,
                axis: SwipeAxis::Vertical,
            }) => ShortcutAction::TriggerChord(vec![KeyCode::B]),
            BindingId::Gesture(GestureBinding::LongPress { fingers }) => {
                ShortcutAction::HoldKeys(vec![long_press_fallback_key(fingers)])
            }
            BindingId::Gesture(GestureBinding::Swipe { axis, .. }) => {
                ShortcutAction::TriggerChord(vec![swipe_fallback_key(axis)])
            }
        }
    }
}

fn long_press_fallback_key(fingers: u8) -> KeyCode {
    match fingers {
        1 => KeyCode::Alt,
        2 => KeyCode::Space,
        3 => KeyCode::Shift,
        _ => KeyCode::Control,
    }
}

fn swipe_fallback_key(axis: SwipeAxis) -> KeyCode {
    match axis {
        SwipeAxis::Horizontal => KeyCode::E,
        SwipeAxis::Vertical => KeyCode::B,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_current_default_bindings() {
        assert_eq!(
            DefaultPreset.action_for(BindingId::StylusTrigger(StylusTrigger::Squeeze)),
            ShortcutAction::Advanced(AdvancedAction::PointerClick {
                keys: Vec::new(),
                button: MouseButton::Right,
                anchor: PointerAnchor::CurrentHoverOrLastInRange,
            })
        );
        assert_eq!(
            DefaultPreset.action_for(BindingId::StylusTrigger(StylusTrigger::DoubleTap)),
            ShortcutAction::Advanced(AdvancedAction::PointerClick {
                keys: Vec::new(),
                button: MouseButton::Right,
                anchor: PointerAnchor::CurrentHoverOrLastInRange,
            })
        );
        assert_eq!(
            DefaultPreset.action_for(BindingId::Gesture(GestureBinding::ThreePan)),
            ShortcutAction::Advanced(AdvancedAction::PointerDrag {
                modifiers: vec![KeyCode::Alt],
                button: Some(MouseButton::Right),
            })
        );
        assert_eq!(
            DefaultPreset.action_for(BindingId::Gesture(GestureBinding::TwoPinch)),
            ShortcutAction::Advanced(AdvancedAction::PointerWheel {
                modifiers: vec![KeyCode::Alt],
            })
        );
        assert_eq!(
            DefaultPreset.action_for(BindingId::Gesture(GestureBinding::TwoRotate)),
            ShortcutAction::TriggerChord(vec![KeyCode::R])
        );
        assert_eq!(
            DefaultPreset.action_for(BindingId::Gesture(GestureBinding::LongPress { fingers: 1 })),
            ShortcutAction::HoldKeys(vec![KeyCode::Alt])
        );
        assert_eq!(
            DefaultPreset.action_for(BindingId::StylusTrigger(StylusTrigger::FourTap)),
            ShortcutAction::Advanced(AdvancedAction::ReleaseActiveKeys)
        );
    }

    #[test]
    fn keeps_two_pan_reserved_for_future_radial_menu() {
        assert_eq!(
            DefaultPreset.action_for(BindingId::Gesture(GestureBinding::TwoPan)),
            ShortcutAction::Advanced(AdvancedAction::ReservedRadialMenu)
        );
    }
}
