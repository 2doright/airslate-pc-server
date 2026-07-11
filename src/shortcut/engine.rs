use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use tracing::{info, warn};

use crate::{
    protocol::{GestureFrame, GestureState, StylusFlags},
    workspace::WorkspaceService,
};

use super::{
    SharedRadialMenuOverlay, SharedShortcutProfile,
    domain::{
        AdvancedAction, BindingId, GestureBinding, KeyCode, MouseButton, PointerAnchor,
        RadialInnerSlot, ScreenPoint, ShortcutAction, ShortcutCommand, StylusTrigger,
    },
    radial_menu::{RadialAnchor, RadialSelection, anchor_from_point, selection_from_offset},
};

const DEFAULT_HOLD_TTL: Duration = Duration::from_millis(100);
const LONG_PRESS_HOLD_TTL: Duration = Duration::from_millis(260);
const PINCH_WHEEL_SCALE: f32 = 1200.0;
const ROTATE_MOVE_SCALE: f32 = 4.0;

#[derive(Debug, Clone)]
struct ActiveHold {
    commands: Vec<ShortcutCommand>,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
struct ActiveAdvancedAction {
    state: ActiveAdvancedState,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
enum ActiveAdvancedState {
    PointerDrag {
        modifiers: Vec<KeyCode>,
        button: Option<MouseButton>,
        last_offset_x: f32,
        last_offset_y: f32,
    },
    PointerWheel {
        modifiers: Vec<KeyCode>,
        last_scale: f32,
    },
    PointerRotate {
        modifiers: Vec<KeyCode>,
        last_angle: f32,
    },
    RadialMenu {
        anchor: RadialAnchor,
        selection: RadialSelection,
    },
}

#[derive(Debug, Default, Clone, Copy)]
struct PointerContext {
    current_hover: Option<ScreenPoint>,
    last_in_range: Option<ScreenPoint>,
}

pub struct ShortcutEngine {
    profile: SharedShortcutProfile,
    overlay: SharedRadialMenuOverlay,
    workspace: WorkspaceService,
    active_holds: HashMap<BindingId, ActiveHold>,
    active_advanced: HashMap<BindingId, ActiveAdvancedAction>,
    last_trigger_seq: HashMap<BindingId, u32>,
    pointer_context: PointerContext,
    toggled_keys: HashSet<KeyCode>,
}

#[cfg(test)]
impl Default for ShortcutEngine {
    fn default() -> Self {
        Self::new(
            super::profile::ShortcutProfile::default().shared(),
            super::null_radial_menu_overlay(),
            WorkspaceService::from_snapshot(crate::workspace::WorkspaceSnapshot {
                monitors: vec![crate::workspace::MonitorInfo {
                    id: crate::workspace::MonitorId::new("default".to_string()),
                    device_name: "DEFAULT".to_string(),
                    is_primary: true,
                    pixel_width: 1920,
                    pixel_height: 1080,
                    virtual_left: 0,
                    virtual_top: 0,
                    virtual_right: 1920,
                    virtual_bottom: 1080,
                }],
                active_monitor_id: Some(crate::workspace::MonitorId::new("default".to_string())),
                active_workspace: Some(crate::workspace::ActiveWorkspace {
                    monitor: crate::workspace::MonitorInfo {
                        id: crate::workspace::MonitorId::new("default".to_string()),
                        device_name: "DEFAULT".to_string(),
                        is_primary: true,
                        pixel_width: 1920,
                        pixel_height: 1080,
                        virtual_left: 0,
                        virtual_top: 0,
                        virtual_right: 1920,
                        virtual_bottom: 1080,
                    },
                }),
            }),
        )
    }
}

impl ShortcutEngine {
    pub fn new(
        profile: SharedShortcutProfile,
        overlay: SharedRadialMenuOverlay,
        workspace: WorkspaceService,
    ) -> Self {
        Self {
            profile,
            overlay,
            workspace,
            active_holds: HashMap::new(),
            active_advanced: HashMap::new(),
            last_trigger_seq: HashMap::new(),
            pointer_context: PointerContext::default(),
            toggled_keys: HashSet::new(),
        }
    }

    pub fn update_pointer_context(&mut self, point: ScreenPoint, in_range: bool, is_contact: bool) {
        if in_range {
            self.pointer_context.last_in_range = Some(point);
            self.pointer_context.current_hover = (!is_contact).then_some(point);
            self.sync_hold_indicator();
            return;
        }

        self.pointer_context.current_hover = None;
        self.sync_hold_indicator();
    }

    pub fn process_gesture(&mut self, frame: &GestureFrame, now: Instant) -> Vec<ShortcutCommand> {
        let mut commands = self.expire_due(now);
        let Some(binding) = GestureBinding::from_frame(frame).map(BindingId::Gesture) else {
            return commands;
        };

        let action = self.action_for(binding);
        if matches!(binding, BindingId::Gesture(GestureBinding::TwoPan)) {
            info!(state = ?frame.state, seq = frame.seq, val1 = frame.val1, val2 = frame.val2, action = ?action, "processing twoPan gesture");
        }

        match action {
            ShortcutAction::HoldKeys(keys) => {
                if !keys.is_empty() {
                    commands.extend(self.handle_hold(
                        binding,
                        key_down_commands(&keys),
                        frame.state,
                        now,
                    ));
                }
            }
            ShortcutAction::TriggerChord(keys) => {
                if !keys.is_empty() && matches!(frame.state, GestureState::Begin) {
                    commands.extend(self.handle_trigger(
                        binding,
                        frame.seq,
                        ShortcutCommand::PressChord(keys),
                    ));
                }
            }
            ShortcutAction::Advanced(action) => {
                commands.extend(self.handle_advanced(binding, action, frame, now));
            }
            ShortcutAction::Disabled => {}
        }

        commands
    }

    pub fn process_stylus_flags(
        &mut self,
        seq: u32,
        flags: StylusFlags,
        now: Instant,
    ) -> Vec<ShortcutCommand> {
        let mut commands = self.expire_due(now);

        for (trigger, is_active) in stylus_flag_states(flags) {
            if !is_active {
                continue;
            }

            let binding = BindingId::StylusTrigger(trigger);
            match self.action_for(binding) {
                ShortcutAction::TriggerChord(keys) => {
                    if !keys.is_empty() {
                        commands.extend(self.handle_trigger(
                            binding,
                            seq,
                            ShortcutCommand::PressChord(keys),
                        ));
                    }
                }
                ShortcutAction::Advanced(AdvancedAction::PointerClick {
                    keys,
                    button,
                    anchor,
                }) => {
                    if self.last_trigger_seq.get(&binding) != Some(&seq) {
                        if !keys.is_empty() {
                            commands.push(ShortcutCommand::PressChord(keys));
                        }
                        if let Some(point) = self.resolve_pointer_anchor(anchor) {
                            commands.push(ShortcutCommand::ClickAt {
                                button,
                                x: point.x,
                                y: point.y,
                            });
                        }
                        self.last_trigger_seq.insert(binding, seq);
                    }
                }
                ShortcutAction::Advanced(AdvancedAction::ReleaseActiveKeys) => {
                    if self.last_trigger_seq.get(&binding) != Some(&seq) {
                        commands.extend(self.release_all());
                        self.last_trigger_seq.insert(binding, seq);
                    }
                }
                ShortcutAction::Advanced(_)
                | ShortcutAction::HoldKeys(_)
                | ShortcutAction::Disabled => {}
            }
        }

        commands
    }

    pub fn expire_due(&mut self, now: Instant) -> Vec<ShortcutCommand> {
        let mut expired: Vec<_> = self
            .active_holds
            .iter()
            .filter_map(|(binding, active)| (active.expires_at <= now).then_some(*binding))
            .collect();
        expired.extend(
            self.active_advanced
                .iter()
                .filter_map(|(binding, active)| (active.expires_at <= now).then_some(*binding)),
        );
        expired.sort_by_key(binding_sort_key);

        let mut commands = Vec::with_capacity(expired.len());
        for binding in expired {
            if let Some(active) = self.active_holds.remove(&binding) {
                commands.extend(release_hold_commands(&active.commands));
            }

            if let Some(active) = self.active_advanced.remove(&binding) {
                commands.extend(self.release_advanced(active.state));
            }
        }

        commands
    }

    pub fn handle_session_end(&mut self) -> Vec<ShortcutCommand> {
        let commands = self.release_all();
        self.pointer_context = PointerContext::default();
        self.sync_hold_indicator();
        commands
    }

    pub fn release_all(&mut self) -> Vec<ShortcutCommand> {
        let mut bindings: Vec<_> = self.active_holds.keys().copied().collect();
        bindings.extend(self.active_advanced.keys().copied());
        bindings.sort_by_key(binding_sort_key);
        bindings.dedup();

        let mut commands = Vec::with_capacity(bindings.len() + self.toggled_keys.len());
        for binding in bindings {
            if let Some(active) = self.active_holds.remove(&binding) {
                commands.extend(release_hold_commands(&active.commands));
            }
            if let Some(active) = self.active_advanced.remove(&binding) {
                commands.extend(self.release_advanced(active.state));
            }
        }

        let mut toggled = self.toggled_keys.drain().collect::<Vec<_>>();
        toggled.sort_by_key(toggle_sort_key);
        commands.extend(toggled.into_iter().map(ShortcutCommand::KeyUp));

        self.last_trigger_seq.clear();
        self.sync_hold_indicator();
        commands
    }

    fn handle_hold(
        &mut self,
        binding: BindingId,
        down_commands: Vec<ShortcutCommand>,
        state: GestureState,
        now: Instant,
    ) -> Vec<ShortcutCommand> {
        match state {
            GestureState::Begin => {
                let ttl = hold_ttl(binding);
                let expires_at = now + ttl;

                if let Some(active) = self.active_holds.get_mut(&binding) {
                    active.expires_at = expires_at;
                    return Vec::new();
                }

                self.active_holds.insert(
                    binding,
                    ActiveHold {
                        commands: down_commands.clone(),
                        expires_at,
                    },
                );
                down_commands
            }
            GestureState::Update => {
                if let Some(active) = self.active_holds.get_mut(&binding) {
                    active.expires_at = now + hold_ttl(binding);
                }
                Vec::new()
            }
            GestureState::End => self
                .active_holds
                .remove(&binding)
                .map(|active| release_hold_commands(&active.commands))
                .unwrap_or_default(),
        }
    }

    fn handle_advanced(
        &mut self,
        binding: BindingId,
        action: AdvancedAction,
        frame: &GestureFrame,
        now: Instant,
    ) -> Vec<ShortcutCommand> {
        match action {
            AdvancedAction::PointerDrag { modifiers, button } => {
                self.handle_pointer_drag(binding, modifiers, button, frame, now)
            }
            AdvancedAction::PointerWheel { modifiers } => {
                self.handle_pointer_wheel(binding, modifiers, frame, now)
            }
            AdvancedAction::PointerRotate { modifiers } => {
                self.handle_pointer_rotate(binding, modifiers, frame, now)
            }
            AdvancedAction::PointerClick { .. } | AdvancedAction::ReleaseActiveKeys => Vec::new(),
            AdvancedAction::ReservedRadialMenu => self.handle_radial_menu(binding, frame, now),
        }
    }

    fn handle_pointer_drag(
        &mut self,
        binding: BindingId,
        modifiers: Vec<KeyCode>,
        button: Option<MouseButton>,
        frame: &GestureFrame,
        now: Instant,
    ) -> Vec<ShortcutCommand> {
        match frame.state {
            GestureState::Begin => {
                let expires_at = now + DEFAULT_HOLD_TTL;
                if let Some(active) = self.active_advanced.get_mut(&binding) {
                    active.expires_at = expires_at;
                    return Vec::new();
                }

                self.active_advanced.insert(
                    binding,
                    ActiveAdvancedAction {
                        state: ActiveAdvancedState::PointerDrag {
                            modifiers: modifiers.clone(),
                            button,
                            last_offset_x: frame.val1,
                            last_offset_y: frame.val2,
                        },
                        expires_at,
                    },
                );

                let mut commands = key_down_commands(&modifiers);
                if let Some(button) = button {
                    commands.push(ShortcutCommand::MouseButtonDown(button));
                }
                commands
            }
            GestureState::Update => {
                let Some(active) = self.active_advanced.get_mut(&binding) else {
                    return Vec::new();
                };
                active.expires_at = now + DEFAULT_HOLD_TTL;
                let ActiveAdvancedState::PointerDrag {
                    last_offset_x,
                    last_offset_y,
                    ..
                } = &mut active.state
                else {
                    return Vec::new();
                };
                let dx = movement_component(frame.val1 - *last_offset_x);
                let dy = movement_component(frame.val2 - *last_offset_y);
                *last_offset_x = frame.val1;
                *last_offset_y = frame.val2;
                if dx == 0 && dy == 0 {
                    return Vec::new();
                }
                vec![ShortcutCommand::MouseMoveRelative { dx, dy }]
            }
            GestureState::End => self
                .active_advanced
                .remove(&binding)
                .map(|active| self.release_advanced(active.state))
                .unwrap_or_default(),
        }
    }

    fn handle_pointer_wheel(
        &mut self,
        binding: BindingId,
        modifiers: Vec<KeyCode>,
        frame: &GestureFrame,
        now: Instant,
    ) -> Vec<ShortcutCommand> {
        match frame.state {
            GestureState::Begin => {
                let expires_at = now + DEFAULT_HOLD_TTL;
                if let Some(active) = self.active_advanced.get_mut(&binding) {
                    active.expires_at = expires_at;
                    if let ActiveAdvancedState::PointerWheel { last_scale, .. } = &mut active.state
                    {
                        *last_scale = frame.val1;
                    }
                    return Vec::new();
                }

                self.active_advanced.insert(
                    binding,
                    ActiveAdvancedAction {
                        state: ActiveAdvancedState::PointerWheel {
                            modifiers: modifiers.clone(),
                            last_scale: frame.val1,
                        },
                        expires_at,
                    },
                );

                key_down_commands(&modifiers)
            }
            GestureState::Update => {
                let Some(active) = self.active_advanced.get_mut(&binding) else {
                    return Vec::new();
                };
                active.expires_at = now + DEFAULT_HOLD_TTL;
                let ActiveAdvancedState::PointerWheel { last_scale, .. } = &mut active.state else {
                    return Vec::new();
                };
                let delta = frame.val1 - *last_scale;
                *last_scale = frame.val1;
                let wheel = wheel_delta(delta);
                (wheel != 0)
                    .then_some(ShortcutCommand::MouseWheel { delta: wheel })
                    .into_iter()
                    .collect()
            }
            GestureState::End => self
                .active_advanced
                .remove(&binding)
                .map(|active| self.release_advanced(active.state))
                .unwrap_or_default(),
        }
    }

    fn handle_pointer_rotate(
        &mut self,
        binding: BindingId,
        modifiers: Vec<KeyCode>,
        frame: &GestureFrame,
        now: Instant,
    ) -> Vec<ShortcutCommand> {
        match frame.state {
            GestureState::Begin => {
                let expires_at = now + DEFAULT_HOLD_TTL;
                if let Some(active) = self.active_advanced.get_mut(&binding) {
                    active.expires_at = expires_at;
                    if let ActiveAdvancedState::PointerRotate { last_angle, .. } = &mut active.state
                    {
                        *last_angle = frame.val1;
                    }
                    return Vec::new();
                }

                self.active_advanced.insert(
                    binding,
                    ActiveAdvancedAction {
                        state: ActiveAdvancedState::PointerRotate {
                            modifiers: modifiers.clone(),
                            last_angle: frame.val1,
                        },
                        expires_at,
                    },
                );

                key_down_commands(&modifiers)
            }
            GestureState::Update => {
                let Some(active) = self.active_advanced.get_mut(&binding) else {
                    return Vec::new();
                };
                active.expires_at = now + DEFAULT_HOLD_TTL;
                let ActiveAdvancedState::PointerRotate { last_angle, .. } = &mut active.state
                else {
                    return Vec::new();
                };
                let delta = normalized_angle_delta(frame.val1 - *last_angle);
                *last_angle = frame.val1;
                let dx = rotate_delta(delta);
                (dx != 0)
                    .then_some(ShortcutCommand::MouseMoveRelative { dx, dy: 0 })
                    .into_iter()
                    .collect()
            }
            GestureState::End => self
                .active_advanced
                .remove(&binding)
                .map(|active| self.release_advanced(active.state))
                .unwrap_or_default(),
        }
    }

    fn handle_radial_menu(
        &mut self,
        binding: BindingId,
        frame: &GestureFrame,
        now: Instant,
    ) -> Vec<ShortcutCommand> {
        match frame.state {
            GestureState::Begin => {
                let Some(anchor) = self.resolve_radial_anchor() else {
                    warn!(
                        seq = frame.seq,
                        "ignored radial menu begin because no workspace center is available"
                    );
                    return Vec::new();
                };
                let selection = selection_from_offset(
                    frame.val1,
                    frame.val2,
                    self.radial_menu_config().inner_enabled,
                );
                let state = ActiveAdvancedState::RadialMenu { anchor, selection };
                self.active_advanced.insert(
                    binding,
                    ActiveAdvancedAction {
                        state: state.clone(),
                        expires_at: now + DEFAULT_HOLD_TTL,
                    },
                );
                info!(x = anchor.center.x, y = anchor.center.y, selection = ?selection, "showing radial menu overlay");
                self.emit_overlay(&state, true);
                Vec::new()
            }
            GestureState::Update => {
                let mut overlay_state = None;
                let inner_enabled = self.radial_menu_config().inner_enabled;
                {
                    let Some(active) = self.active_advanced.get_mut(&binding) else {
                        return Vec::new();
                    };
                    active.expires_at = now + DEFAULT_HOLD_TTL;
                    let ActiveAdvancedState::RadialMenu { anchor, selection } = &mut active.state
                    else {
                        return Vec::new();
                    };
                    let next = selection_from_offset(frame.val1, frame.val2, inner_enabled);
                    if *selection != next {
                        *selection = next;
                        overlay_state = Some(ActiveAdvancedState::RadialMenu {
                            anchor: *anchor,
                            selection: *selection,
                        });
                    }
                }
                if let Some(state) = overlay_state.as_ref() {
                    self.emit_overlay(state, false);
                }
                Vec::new()
            }
            GestureState::End => {
                let Some(active) = self.active_advanced.remove(&binding) else {
                    return Vec::new();
                };
                let ActiveAdvancedState::RadialMenu { selection, .. } = active.state else {
                    return Vec::new();
                };
                info!(selection = ?selection, "closing radial menu overlay");
                self.overlay.hide();
                self.commit_radial_selection(selection)
            }
        }
    }

    fn handle_trigger(
        &mut self,
        binding: BindingId,
        seq: u32,
        command: ShortcutCommand,
    ) -> Vec<ShortcutCommand> {
        if !self.record_trigger_seq(binding, seq) {
            return Vec::new();
        }

        vec![command]
    }

    fn record_trigger_seq(&mut self, binding: BindingId, seq: u32) -> bool {
        if self.last_trigger_seq.get(&binding) == Some(&seq) {
            return false;
        }

        self.last_trigger_seq.insert(binding, seq);
        true
    }

    fn resolve_pointer_anchor(&self, anchor: PointerAnchor) -> Option<ScreenPoint> {
        match anchor {
            PointerAnchor::CurrentHoverOrLastInRange => self
                .pointer_context
                .current_hover
                .or(self.pointer_context.last_in_range),
        }
    }

    fn resolve_radial_anchor(&self) -> Option<RadialAnchor> {
        self.workspace.current_workspace().ok().map(|workspace| {
            anchor_from_point(ScreenPoint {
                x: workspace.monitor.virtual_left + (workspace.monitor.pixel_width as i32 / 2),
                y: workspace.monitor.virtual_top + (workspace.monitor.pixel_height as i32 / 2),
            })
        })
    }

    fn emit_overlay(&self, state: &ActiveAdvancedState, first: bool) {
        let ActiveAdvancedState::RadialMenu { anchor, selection } = state else {
            return;
        };
        let config = self.radial_menu_config();
        let overlay = super::RadialMenuOverlayState {
            anchor: *anchor,
            selection: *selection,
            active_inner_slots: self.active_inner_slots(&config),
            config,
        };
        if first {
            self.overlay.show(overlay);
        } else {
            self.overlay.update(overlay);
        }
    }

    fn commit_radial_selection(&mut self, selection: RadialSelection) -> Vec<ShortcutCommand> {
        let config = self.radial_menu_config();
        match selection {
            RadialSelection::Center => Vec::new(),
            RadialSelection::Inner(slot) => {
                if !config.inner_enabled {
                    return Vec::new();
                }
                let key = config.inner.key_for_slot(slot);
                let commands = if self.toggled_keys.remove(&key) {
                    vec![ShortcutCommand::KeyUp(key)]
                } else {
                    self.toggled_keys.insert(key);
                    vec![ShortcutCommand::KeyDown(key)]
                };
                self.sync_hold_indicator();
                commands
            }
            RadialSelection::Outer(index) => config
                .outer
                .get(index)
                .map(|binding| ShortcutCommand::PressChord(binding.keys.clone()))
                .into_iter()
                .collect(),
        }
    }

    fn radial_menu_config(&self) -> super::RadialMenuConfig {
        self.profile
            .read()
            .map(|profile| profile.radial_menu().clone())
            .unwrap_or_default()
    }

    fn active_inner_slots(&self, config: &super::RadialMenuConfig) -> Vec<RadialInnerSlot> {
        config
            .inner
            .slot_entries()
            .into_iter()
            .filter_map(|(slot, key)| self.toggled_keys.contains(&key).then_some(slot))
            .collect()
    }

    fn hold_indicator_point(&self) -> Option<ScreenPoint> {
        (!self.toggled_keys.is_empty())
            .then_some(
                self.pointer_context
                    .current_hover
                    .or(self.pointer_context.last_in_range),
            )
            .flatten()
    }

    fn sync_hold_indicator(&self) {
        self.overlay
            .sync_hold_indicator(self.hold_indicator_point());
    }

    fn action_for(&self, binding: BindingId) -> ShortcutAction {
        self.profile
            .read()
            .map(|profile| profile.action_for(binding))
            .unwrap_or(ShortcutAction::Disabled)
    }

    fn release_advanced(&self, state: ActiveAdvancedState) -> Vec<ShortcutCommand> {
        match state {
            ActiveAdvancedState::PointerDrag {
                modifiers, button, ..
            } => {
                let mut commands = button
                    .map(ShortcutCommand::MouseButtonUp)
                    .into_iter()
                    .collect::<Vec<_>>();
                commands.extend(key_up_commands(&modifiers));
                commands
            }
            ActiveAdvancedState::PointerWheel { modifiers, .. }
            | ActiveAdvancedState::PointerRotate { modifiers, .. } => key_up_commands(&modifiers),
            ActiveAdvancedState::RadialMenu { .. } => {
                self.overlay.hide();
                Vec::new()
            }
        }
    }
}

fn hold_ttl(binding: BindingId) -> Duration {
    match binding {
        BindingId::Gesture(GestureBinding::LongPress { .. }) => LONG_PRESS_HOLD_TTL,
        _ => DEFAULT_HOLD_TTL,
    }
}

fn stylus_flag_states(flags: StylusFlags) -> [(StylusTrigger, bool); 5] {
    [
        (StylusTrigger::Squeeze, flags.squeeze()),
        (StylusTrigger::DoubleTap, flags.double_tap()),
        (StylusTrigger::TwoTap, flags.two_tap()),
        (StylusTrigger::ThreeTap, flags.three_tap()),
        (StylusTrigger::FourTap, flags.four_tap()),
    ]
}

fn binding_sort_key(binding: &BindingId) -> (u8, u8, u8) {
    match binding {
        BindingId::StylusTrigger(trigger) => match trigger {
            StylusTrigger::Squeeze => (0, 0, 0),
            StylusTrigger::DoubleTap => (0, 1, 0),
            StylusTrigger::TwoTap => (0, 2, 0),
            StylusTrigger::ThreeTap => (0, 3, 0),
            StylusTrigger::FourTap => (0, 4, 0),
        },
        BindingId::Gesture(gesture) => match gesture {
            GestureBinding::TwoPan => (1, 0, 0),
            GestureBinding::ThreePan => (1, 1, 0),
            GestureBinding::TwoPinch => (1, 2, 0),
            GestureBinding::TwoRotate => (1, 3, 0),
            GestureBinding::LongPress { fingers } => (1, 4, *fingers),
            GestureBinding::Swipe { fingers, axis } => (
                1,
                5,
                fingers * 2 + u8::from(matches!(axis, super::domain::SwipeAxis::Vertical)),
            ),
        },
    }
}

fn toggle_sort_key(key: &KeyCode) -> u16 {
    key.sort_rank()
}

fn key_down_commands(modifiers: &[KeyCode]) -> Vec<ShortcutCommand> {
    modifiers
        .iter()
        .copied()
        .map(ShortcutCommand::KeyDown)
        .collect()
}

fn key_up_commands(modifiers: &[KeyCode]) -> Vec<ShortcutCommand> {
    modifiers
        .iter()
        .rev()
        .copied()
        .map(ShortcutCommand::KeyUp)
        .collect()
}

fn release_hold_commands(commands: &[ShortcutCommand]) -> Vec<ShortcutCommand> {
    commands
        .iter()
        .rev()
        .filter_map(|command| match command {
            ShortcutCommand::KeyDown(key) => Some(ShortcutCommand::KeyUp(*key)),
            _ => None,
        })
        .collect()
}

fn movement_component(value: f32) -> i32 {
    value.round() as i32
}

fn wheel_delta(delta: f32) -> i32 {
    (delta * PINCH_WHEEL_SCALE).round() as i32
}

fn normalized_angle_delta(delta: f32) -> f32 {
    if delta > 180.0 {
        delta - 360.0
    } else if delta < -180.0 {
        delta + 360.0
    } else {
        delta
    }
}

fn rotate_delta(delta: f32) -> i32 {
    (delta * ROTATE_MOVE_SCALE).round() as i32
}

#[cfg(test)]
mod tests {
    use crate::protocol::{GestureState, GestureType, StylusFlags};

    use super::super::domain::{KeyCode, MouseButton, ScreenPoint, ShortcutCommand};
    use super::*;

    fn gesture_frame(
        gesture_type: GestureType,
        state: GestureState,
        seq: u32,
        val1: f32,
    ) -> GestureFrame {
        GestureFrame {
            gesture_type,
            state,
            seq,
            timestamp: 10,
            val1,
            val2: 0.0,
            val3: 0.0,
            val4: 0.0,
        }
    }

    fn gesture_frame_xy(
        gesture_type: GestureType,
        state: GestureState,
        seq: u32,
        val1: f32,
        val2: f32,
    ) -> GestureFrame {
        GestureFrame {
            gesture_type,
            state,
            seq,
            timestamp: 10,
            val1,
            val2,
            val3: 0.0,
            val4: 0.0,
        }
    }

    #[test]
    fn hold_begin_update_end_emits_expected_commands() {
        let mut engine = ShortcutEngine::default();
        let start = Instant::now();

        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::OneLongPress, GestureState::Begin, 1, 0.0),
                start,
            ),
            vec![ShortcutCommand::KeyDown(KeyCode::Alt)]
        );
        assert!(
            engine
                .process_gesture(
                    &gesture_frame(GestureType::OneLongPress, GestureState::Update, 2, 0.0),
                    start + Duration::from_millis(200),
                )
                .is_empty()
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::OneLongPress, GestureState::End, 3, 0.0),
                start + Duration::from_millis(250),
            ),
            vec![ShortcutCommand::KeyUp(KeyCode::Alt)]
        );
    }

    #[test]
    fn hold_combo_presses_and_releases_all_keys_in_order() {
        let profile = crate::shortcut::ShortcutProfile {
            custom_bindings: std::collections::HashMap::from([(
                BindingId::Gesture(GestureBinding::LongPress { fingers: 1 }),
                ShortcutAction::HoldKeys(vec![KeyCode::Control, KeyCode::Shift, KeyCode::Z]),
            )]),
            ..crate::shortcut::ShortcutProfile::default()
        };
        let mut engine = ShortcutEngine::new(
            profile.shared(),
            crate::shortcut::null_radial_menu_overlay(),
            WorkspaceService::from_snapshot(crate::workspace::WorkspaceSnapshot {
                monitors: vec![crate::workspace::MonitorInfo {
                    id: crate::workspace::MonitorId::new("default".to_string()),
                    device_name: "DEFAULT".to_string(),
                    is_primary: true,
                    pixel_width: 1920,
                    pixel_height: 1080,
                    virtual_left: 0,
                    virtual_top: 0,
                    virtual_right: 1920,
                    virtual_bottom: 1080,
                }],
                active_monitor_id: Some(crate::workspace::MonitorId::new("default".to_string())),
                active_workspace: Some(crate::workspace::ActiveWorkspace {
                    monitor: crate::workspace::MonitorInfo {
                        id: crate::workspace::MonitorId::new("default".to_string()),
                        device_name: "DEFAULT".to_string(),
                        is_primary: true,
                        pixel_width: 1920,
                        pixel_height: 1080,
                        virtual_left: 0,
                        virtual_top: 0,
                        virtual_right: 1920,
                        virtual_bottom: 1080,
                    },
                }),
            }),
        );
        let start = Instant::now();

        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::OneLongPress, GestureState::Begin, 1, 0.0),
                start,
            ),
            vec![
                ShortcutCommand::KeyDown(KeyCode::Control),
                ShortcutCommand::KeyDown(KeyCode::Shift),
                ShortcutCommand::KeyDown(KeyCode::Z),
            ]
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::OneLongPress, GestureState::End, 2, 0.0),
                start + Duration::from_millis(20),
            ),
            vec![
                ShortcutCommand::KeyUp(KeyCode::Z),
                ShortcutCommand::KeyUp(KeyCode::Shift),
                ShortcutCommand::KeyUp(KeyCode::Control),
            ]
        );
    }

    #[test]
    fn hold_ttl_expiry_releases_non_long_press_binding() {
        let mut engine = ShortcutEngine::default();
        let start = Instant::now();

        assert!(
            engine
                .process_gesture(
                    &gesture_frame(GestureType::TwoPan, GestureState::Begin, 1, 0.0),
                    start,
                )
                .is_empty()
        );
        assert!(
            engine
                .expire_due(start + Duration::from_millis(150))
                .is_empty()
        );

        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::TwoLongPress, GestureState::Begin, 2, 0.0),
                start,
            ),
            vec![ShortcutCommand::KeyDown(KeyCode::Space)]
        );
        assert_eq!(
            engine.expire_due(start + Duration::from_millis(450)),
            vec![ShortcutCommand::KeyUp(KeyCode::Space)]
        );
    }

    #[test]
    fn long_press_update_extends_ttl_for_200ms_cadence() {
        let mut engine = ShortcutEngine::default();
        let start = Instant::now();

        engine.process_gesture(
            &gesture_frame(GestureType::ThreeLongPress, GestureState::Begin, 1, 0.0),
            start,
        );
        engine.process_gesture(
            &gesture_frame(GestureType::ThreeLongPress, GestureState::Update, 2, 0.0),
            start + Duration::from_millis(200),
        );

        assert!(
            engine
                .expire_due(start + Duration::from_millis(450))
                .is_empty()
        );
        assert_eq!(
            engine.expire_due(start + Duration::from_millis(470)),
            vec![ShortcutCommand::KeyUp(KeyCode::Shift)]
        );
    }

    #[test]
    fn trigger_deduplicates_duplicate_swipe_seq() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();

        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::OneSwipe, GestureState::Begin, 7, 0.0),
                now,
            ),
            vec![ShortcutCommand::PressChord(vec![KeyCode::E])]
        );
        assert!(
            engine
                .process_gesture(
                    &gesture_frame(GestureType::OneSwipe, GestureState::Begin, 7, 0.0),
                    now + Duration::from_millis(1),
                )
                .is_empty()
        );
    }

    #[test]
    fn stylus_flag_pulses_fire_once_per_packet_seq() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();

        assert!(
            engine
                .process_stylus_flags(1, StylusFlags(0), now)
                .is_empty()
        );
        assert_eq!(
            engine.process_stylus_flags(
                2,
                StylusFlags(0b0001_0000),
                now + Duration::from_millis(1)
            ),
            vec![ShortcutCommand::PressChord(vec![
                KeyCode::Control,
                KeyCode::Z
            ])]
        );
        assert_eq!(
            engine.process_stylus_flags(
                3,
                StylusFlags(0b0001_0000),
                now + Duration::from_millis(2)
            ),
            vec![ShortcutCommand::PressChord(vec![
                KeyCode::Control,
                KeyCode::Z
            ])]
        );
        assert!(
            engine
                .process_stylus_flags(3, StylusFlags(0b0001_0000), now + Duration::from_millis(3))
                .is_empty()
        );
    }

    #[test]
    fn squeeze_uses_same_secondary_click_anchor_resolution_as_double_tap() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();

        engine.update_pointer_context(ScreenPoint { x: 640, y: 480 }, true, false);
        assert_eq!(
            engine.process_stylus_flags(1, StylusFlags(0b0000_0100), now),
            vec![ShortcutCommand::ClickAt {
                button: MouseButton::Right,
                x: 640,
                y: 480
            }]
        );

        engine.update_pointer_context(ScreenPoint { x: 700, y: 520 }, true, true);
        assert_eq!(
            engine.process_stylus_flags(
                2,
                StylusFlags(0b0000_0100),
                now + Duration::from_millis(1)
            ),
            vec![ShortcutCommand::ClickAt {
                button: MouseButton::Right,
                x: 700,
                y: 520
            }]
        );
    }

    #[test]
    fn stylus_trigger_executes_keyboard_and_pointer_click_components_together() {
        let mut engine = ShortcutEngine::default();
        let binding = BindingId::StylusTrigger(StylusTrigger::Squeeze);
        engine
            .profile
            .write()
            .expect("profile lock")
            .set_custom_binding(
                binding,
                ShortcutAction::Advanced(AdvancedAction::PointerClick {
                    keys: vec![KeyCode::Control, KeyCode::K],
                    button: MouseButton::Left,
                    anchor: PointerAnchor::CurrentHoverOrLastInRange,
                }),
            );
        engine.update_pointer_context(ScreenPoint { x: 320, y: 240 }, true, false);

        assert_eq!(
            engine.process_stylus_flags(1, StylusFlags(0b0000_0100), Instant::now()),
            vec![
                ShortcutCommand::PressChord(vec![KeyCode::Control, KeyCode::K]),
                ShortcutCommand::ClickAt {
                    button: MouseButton::Left,
                    x: 320,
                    y: 240,
                },
            ]
        );
    }

    #[test]
    fn empty_keyboard_component_emits_no_keyboard_command() {
        let mut engine = ShortcutEngine::default();
        engine
            .profile
            .write()
            .expect("profile lock")
            .set_custom_binding(
                BindingId::Gesture(GestureBinding::Swipe {
                    fingers: 1,
                    axis: crate::shortcut::SwipeAxis::Horizontal,
                }),
                ShortcutAction::TriggerChord(Vec::new()),
            );

        assert!(
            engine
                .process_gesture(
                    &gesture_frame(GestureType::OneSwipe, GestureState::Begin, 1, 0.0),
                    Instant::now(),
                )
                .is_empty()
        );
    }

    #[test]
    fn three_pan_begin_update_end_emits_mouse_drag_lifecycle() {
        let mut engine = ShortcutEngine::default();
        let start = Instant::now();

        assert_eq!(
            engine.process_gesture(
                &gesture_frame_xy(GestureType::ThreePan, GestureState::Begin, 1, 0.0, 0.0),
                start,
            ),
            vec![
                ShortcutCommand::KeyDown(KeyCode::Alt),
                ShortcutCommand::MouseButtonDown(MouseButton::Right),
            ]
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame_xy(GestureType::ThreePan, GestureState::Update, 2, 12.0, -8.0),
                start + Duration::from_millis(10),
            ),
            vec![ShortcutCommand::MouseMoveRelative { dx: 12, dy: -8 }]
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame_xy(GestureType::ThreePan, GestureState::End, 3, 0.0, 0.0),
                start + Duration::from_millis(20),
            ),
            vec![
                ShortcutCommand::MouseButtonUp(MouseButton::Right),
                ShortcutCommand::KeyUp(KeyCode::Alt),
            ]
        );
    }

    #[test]
    fn two_pinch_update_emits_wheel_delta() {
        let mut engine = ShortcutEngine::default();
        let start = Instant::now();

        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::TwoPinch, GestureState::Begin, 1, 1.0),
                start,
            ),
            vec![ShortcutCommand::KeyDown(KeyCode::Alt)]
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::TwoPinch, GestureState::Update, 2, 1.1),
                start + Duration::from_millis(10),
            ),
            vec![ShortcutCommand::MouseWheel { delta: 120 }]
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::TwoPinch, GestureState::End, 3, 1.1),
                start + Duration::from_millis(20),
            ),
            vec![ShortcutCommand::KeyUp(KeyCode::Alt)]
        );
    }

    #[test]
    fn two_rotate_begin_triggers_single_r_press() {
        let mut engine = ShortcutEngine::default();
        let start = Instant::now();

        assert_eq!(
            engine.process_gesture(
                &gesture_frame(GestureType::TwoRotate, GestureState::Begin, 1, 30.0),
                start,
            ),
            vec![ShortcutCommand::PressChord(vec![KeyCode::R])]
        );
        assert!(
            engine
                .process_gesture(
                    &gesture_frame(GestureType::TwoRotate, GestureState::Update, 2, 33.0),
                    start + Duration::from_millis(10),
                )
                .is_empty()
        );
        assert!(
            engine
                .process_gesture(
                    &gesture_frame(GestureType::TwoRotate, GestureState::End, 3, 33.0),
                    start + Duration::from_millis(20),
                )
                .is_empty()
        );
    }

    #[test]
    fn double_tap_prefers_current_hover_then_last_in_range() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();

        engine.update_pointer_context(ScreenPoint { x: 640, y: 480 }, true, false);
        assert_eq!(
            engine.process_stylus_flags(1, StylusFlags(0b0000_1000), now),
            vec![ShortcutCommand::ClickAt {
                button: MouseButton::Right,
                x: 640,
                y: 480
            }]
        );

        engine.update_pointer_context(ScreenPoint { x: 700, y: 520 }, true, true);
        assert_eq!(
            engine.process_stylus_flags(
                2,
                StylusFlags(0b0000_1000),
                now + Duration::from_millis(1)
            ),
            vec![ShortcutCommand::ClickAt {
                button: MouseButton::Right,
                x: 700,
                y: 520
            }]
        );
    }

    #[test]
    fn double_tap_without_pointer_context_emits_nothing() {
        let mut engine = ShortcutEngine::default();

        assert!(
            engine
                .process_stylus_flags(1, StylusFlags(0b0000_1000), Instant::now())
                .is_empty()
        );
    }

    #[test]
    fn two_pan_end_on_center_emits_nothing() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();
        engine.update_pointer_context(ScreenPoint { x: 320, y: 240 }, true, false);

        assert!(
            engine
                .process_gesture(
                    &gesture_frame_xy(GestureType::TwoPan, GestureState::Begin, 1, 0.0, 0.0),
                    now,
                )
                .is_empty()
        );
        assert!(
            engine
                .process_gesture(
                    &gesture_frame_xy(GestureType::TwoPan, GestureState::End, 2, 0.0, 0.0),
                    now + Duration::from_millis(10),
                )
                .is_empty()
        );
    }

    #[test]
    fn two_pan_inner_slot_toggles_key_state() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();
        engine.update_pointer_context(ScreenPoint { x: 320, y: 240 }, true, false);

        engine.process_gesture(
            &gesture_frame_xy(GestureType::TwoPan, GestureState::Begin, 1, 0.0, -56.0),
            now,
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame_xy(GestureType::TwoPan, GestureState::End, 2, 0.0, -56.0),
                now + Duration::from_millis(10),
            ),
            vec![ShortcutCommand::KeyDown(KeyCode::Shift)]
        );

        engine.process_gesture(
            &gesture_frame_xy(GestureType::TwoPan, GestureState::Begin, 3, 0.0, -56.0),
            now + Duration::from_millis(20),
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame_xy(GestureType::TwoPan, GestureState::End, 4, 0.0, -56.0),
                now + Duration::from_millis(30),
            ),
            vec![ShortcutCommand::KeyUp(KeyCode::Shift)]
        );
    }

    #[test]
    fn two_pan_with_disabled_inner_ring_uses_outer_slot_command() {
        let profile = crate::shortcut::ShortcutProfile {
            radial_menu: crate::shortcut::RadialMenuConfig {
                inner_enabled: false,
                ..crate::shortcut::RadialMenuConfig::default()
            },
            ..crate::shortcut::ShortcutProfile::default()
        };
        let mut engine = ShortcutEngine::new(
            profile.shared(),
            crate::shortcut::null_radial_menu_overlay(),
            WorkspaceService::from_snapshot(crate::workspace::WorkspaceSnapshot {
                monitors: vec![crate::workspace::MonitorInfo {
                    id: crate::workspace::MonitorId::new("default".to_string()),
                    device_name: "DEFAULT".to_string(),
                    is_primary: true,
                    pixel_width: 1920,
                    pixel_height: 1080,
                    virtual_left: 0,
                    virtual_top: 0,
                    virtual_right: 1920,
                    virtual_bottom: 1080,
                }],
                active_monitor_id: Some(crate::workspace::MonitorId::new("default".to_string())),
                active_workspace: Some(crate::workspace::ActiveWorkspace {
                    monitor: crate::workspace::MonitorInfo {
                        id: crate::workspace::MonitorId::new("default".to_string()),
                        device_name: "DEFAULT".to_string(),
                        is_primary: true,
                        pixel_width: 1920,
                        pixel_height: 1080,
                        virtual_left: 0,
                        virtual_top: 0,
                        virtual_right: 1920,
                        virtual_bottom: 1080,
                    },
                }),
            }),
        );
        let now = Instant::now();
        engine.update_pointer_context(ScreenPoint { x: 320, y: 240 }, true, false);

        engine.process_gesture(
            &gesture_frame_xy(GestureType::TwoPan, GestureState::Begin, 1, 0.0, -56.0),
            now,
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame_xy(GestureType::TwoPan, GestureState::End, 2, 0.0, -56.0),
                now + Duration::from_millis(10),
            ),
            vec![ShortcutCommand::PressChord(vec![KeyCode::V])]
        );
    }

    #[test]
    fn four_tap_releases_all_active_keydown_state_once_per_seq() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();
        engine.update_pointer_context(ScreenPoint { x: 320, y: 240 }, true, false);

        engine.process_gesture(
            &gesture_frame_xy(GestureType::TwoPan, GestureState::Begin, 1, 0.0, -56.0),
            now,
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame_xy(GestureType::TwoPan, GestureState::End, 2, 0.0, -56.0),
                now + Duration::from_millis(10),
            ),
            vec![ShortcutCommand::KeyDown(KeyCode::Shift)]
        );

        assert_eq!(
            engine.process_stylus_flags(
                10,
                StylusFlags(0b0100_0000),
                now + Duration::from_millis(20)
            ),
            vec![ShortcutCommand::KeyUp(KeyCode::Shift)]
        );
        assert!(
            engine
                .process_stylus_flags(
                    10,
                    StylusFlags(0b0100_0000),
                    now + Duration::from_millis(21)
                )
                .is_empty()
        );
    }

    #[test]
    fn two_pan_outer_slot_emits_one_shot_chord() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();
        engine.update_pointer_context(ScreenPoint { x: 320, y: 240 }, true, false);

        engine.process_gesture(
            &gesture_frame_xy(GestureType::TwoPan, GestureState::Begin, 1, 160.0, 0.0),
            now,
        );
        assert_eq!(
            engine.process_gesture(
                &gesture_frame_xy(GestureType::TwoPan, GestureState::End, 2, 160.0, 0.0),
                now + Duration::from_millis(10),
            ),
            vec![ShortcutCommand::PressChord(vec![KeyCode::BracketRight])]
        );
    }

    #[test]
    fn release_all_releases_active_holds_and_advanced_state() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();

        engine.process_gesture(
            &gesture_frame(GestureType::FourLongPress, GestureState::Begin, 1, 0.0),
            now,
        );
        engine.process_gesture(
            &gesture_frame_xy(GestureType::ThreePan, GestureState::Begin, 2, 0.0, 0.0),
            now + Duration::from_millis(1),
        );

        assert_eq!(
            engine.release_all(),
            vec![
                ShortcutCommand::MouseButtonUp(MouseButton::Right),
                ShortcutCommand::KeyUp(KeyCode::Alt),
                ShortcutCommand::KeyUp(KeyCode::Control),
            ]
        );
    }

    #[test]
    fn handle_session_end_releases_active_shortcuts_and_clears_pointer_context() {
        let mut engine = ShortcutEngine::default();
        let now = Instant::now();

        engine.update_pointer_context(ScreenPoint { x: 100, y: 200 }, true, false);
        engine.process_gesture(
            &gesture_frame_xy(GestureType::ThreePan, GestureState::Begin, 1, 0.0, 0.0),
            now,
        );

        assert_eq!(
            engine.handle_session_end(),
            vec![
                ShortcutCommand::MouseButtonUp(MouseButton::Right),
                ShortcutCommand::KeyUp(KeyCode::Alt),
            ]
        );
        assert!(
            engine
                .process_stylus_flags(2, StylusFlags(0b0000_1000), now + Duration::from_millis(1))
                .is_empty()
        );
    }
}
