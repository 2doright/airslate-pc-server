use serde::{Deserialize, Serialize};

use crate::{
    app::state::AppRuntime,
    error::AppError,
    shortcut::{
        AdvancedAction, BindingId, GestureBinding, KeyCode, MouseButton, PointerAnchor,
        RadialInnerSlot, ShortcutAction, ShortcutPresetLibrary, ShortcutProfile, StylusTrigger,
        SwipeAxis, all_bindings,
    },
    usb_accessory::UsbStatusEvent,
    workspace::WorkspaceService,
};

use super::local_ip::lan_ipv4_values;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrapDto {
    pub app_name: String,
    pub distribution: AppDistributionDto,
    pub config_version: u32,
    pub config_path: String,
    pub launch_at_startup: bool,
    pub show_launch_at_startup_on_main_page: bool,
    pub latest_contact_move_only: bool,
    pub latest_contact_move_tolerance_ms: u32,
    pub hover_move_policy: u8,
    pub preempt_previous_stroke: bool,
    pub usb_interface: String,
    pub ipv4_values: Vec<String>,
    pub pressure_curve: PressureCurveDto,
    pub monitors: Vec<MonitorDto>,
    pub active_monitor: Option<ActiveMonitorDto>,
    pub active_preset_id: String,
    pub presets: Vec<ShortcutPresetDto>,
    pub effective_bindings: Vec<BindingDto>,
    pub session_status: SessionStatusDto,
    pub usb_status: UsbStatusEvent,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppDistributionDto {
    Installed,
    Portable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PressureControlPointDto {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PressureCurveDto {
    pub control_point1: PressureControlPointDto,
    pub control_point2: PressureControlPointDto,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PressureCurvePayload {
    pub control_point1: PressureControlPointDto,
    pub control_point2: PressureControlPointDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshotDto {
    pub monitors: Vec<MonitorDto>,
    pub active_monitor: Option<ActiveMonitorDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorDto {
    pub id: String,
    pub device_name: String,
    pub label: String,
    pub is_primary: bool,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveMonitorDto {
    pub id: String,
    pub device_name: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutPresetDto {
    pub id: String,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingDto {
    pub id: String,
    pub label: String,
    pub category: String,
    pub preset_action: ActionDto,
    pub current_action: ActionDto,
    pub uses_preset: bool,
    pub editable_keys: Option<Vec<String>>,
    pub special_actions: Vec<SpecialActionOptionDto>,
    pub active_special_action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecialActionOptionDto {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusDto {
    pub has_active_session: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadialInnerSlotDto {
    pub slot: String,
    pub label: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadialOuterSlotDto {
    pub index: usize,
    pub label: String,
    pub angle_label: String,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ActionDto {
    Disabled,
    HoldKey {
        keys: Vec<String>,
    },
    TriggerChord {
        keys: Vec<String>,
    },
    Advanced {
        label: String,
        detail: String,
        #[serde(rename = "radialInnerEnabled")]
        radial_inner_enabled: bool,
        #[serde(rename = "radialInnerSlots")]
        radial_inner_slots: Vec<RadialInnerSlotDto>,
        #[serde(rename = "radialOuterSlots")]
        radial_outer_slots: Vec<RadialOuterSlotDto>,
    },
}

pub fn app_bootstrap(
    runtime: &AppRuntime,
    config_path: &str,
    usb_status: UsbStatusEvent,
) -> Result<AppBootstrapDto, AppError> {
    let config = runtime.config_snapshot()?;
    let presets = runtime.shortcut_presets_snapshot()?;
    let active_preset = presets
        .active()
        .ok_or_else(|| AppError::ShortcutPreset("active preset is missing".to_string()))?;
    let workspace = runtime.workspace();
    let workspace_snapshot = workspace_snapshot(&workspace)?;
    let has_active_session = runtime.has_active_session()?;

    Ok(AppBootstrapDto {
        app_name: config.app_name,
        distribution: app_distribution(),
        config_version: config.config_version,
        config_path: config_path.to_string(),
        launch_at_startup: config.launch_at_startup,
        show_launch_at_startup_on_main_page: config.show_launch_at_startup_on_main_page,
        latest_contact_move_only: config.latest_contact_move_only,
        latest_contact_move_tolerance_ms: config.latest_contact_move_tolerance_ms,
        hover_move_policy: config.hover_move_policy.level(),
        preempt_previous_stroke: config.preempt_previous_stroke,
        usb_interface: config.usb_interface.to_string(),
        ipv4_values: lan_ipv4_values(),
        pressure_curve: PressureCurveDto::from(config.pressure_curve),
        monitors: workspace_snapshot.monitors,
        active_monitor: workspace_snapshot.active_monitor,
        active_preset_id: presets.active_preset_id.clone(),
        presets: preset_dtos(&presets),
        effective_bindings: binding_dtos(&active_preset.profile, false),
        session_status: SessionStatusDto { has_active_session },
        usb_status,
    })
}

fn app_distribution() -> AppDistributionDto {
    if cfg!(feature = "portable") {
        AppDistributionDto::Portable
    } else {
        AppDistributionDto::Installed
    }
}

pub fn workspace_snapshot(workspace: &WorkspaceService) -> Result<WorkspaceSnapshotDto, AppError> {
    let snapshot = workspace.snapshot()?;
    let active_monitor = snapshot
        .active_workspace
        .clone()
        .map(|workspace| ActiveMonitorDto {
            id: workspace.monitor.id.as_str().to_string(),
            device_name: workspace.monitor.device_name,
            pixel_width: workspace.monitor.pixel_width,
            pixel_height: workspace.monitor.pixel_height,
        });

    let active_monitor_id = snapshot
        .active_monitor_id
        .as_ref()
        .map(|monitor| monitor.as_str().to_string());

    Ok(WorkspaceSnapshotDto {
        monitors: snapshot
            .monitors
            .into_iter()
            .map(|monitor| {
                let selected = active_monitor_id
                    .as_ref()
                    .map(|active| active == monitor.id.as_str())
                    .unwrap_or(false);
                MonitorDto {
                    label: format!(
                        "{} ({}×{}){}",
                        monitor.device_name,
                        monitor.pixel_width,
                        monitor.pixel_height,
                        if monitor.is_primary {
                            " · 主显示器"
                        } else {
                            ""
                        }
                    ),
                    id: monitor.id.as_str().to_string(),
                    device_name: monitor.device_name,
                    is_primary: monitor.is_primary,
                    pixel_width: monitor.pixel_width,
                    pixel_height: monitor.pixel_height,
                    selected,
                }
            })
            .collect(),
        active_monitor,
    })
}

fn preset_dtos(presets: &ShortcutPresetLibrary) -> Vec<ShortcutPresetDto> {
    presets
        .presets
        .iter()
        .map(|preset| ShortcutPresetDto {
            id: preset.id.clone(),
            name: preset.name.clone(),
            active: preset.id == presets.active_preset_id,
        })
        .collect()
}

pub fn binding_dtos(profile: &ShortcutProfile, preset_only: bool) -> Vec<BindingDto> {
    all_bindings()
        .into_iter()
        .map(|binding| {
            let preset_action = profile.preset_action_for(binding);
            let current_action = if preset_only {
                preset_action.clone()
            } else {
                profile.action_for(binding)
            };
            let uses_preset = !profile.custom_bindings.contains_key(&binding);
            BindingDto {
                id: binding_id(binding),
                label: binding_label(binding),
                category: category_label(binding).to_string(),
                preset_action: action_dto(&preset_action, profile),
                current_action: action_dto(&current_action, profile),
                uses_preset: if preset_only { true } else { uses_preset },
                editable_keys: editable_keys(&current_action),
                special_actions: special_action_options(binding),
                active_special_action: active_special_action(&current_action).to_string(),
            }
        })
        .collect()
}

fn binding_id(binding: BindingId) -> String {
    binding.persisted_key()
}

pub fn parse_binding_id(value: &str) -> Result<BindingId, AppError> {
    BindingId::parse_persisted_key(value)
        .ok_or_else(|| AppError::DesktopShell(format!("unknown binding id: {value}")))
}

fn binding_label(binding: BindingId) -> String {
    match binding {
        BindingId::StylusTrigger(trigger) => match trigger {
            StylusTrigger::Squeeze => "笔侧键挤压".to_string(),
            StylusTrigger::DoubleTap => "笔双击".to_string(),
            StylusTrigger::TwoTap => "笔轻点 ×2".to_string(),
            StylusTrigger::ThreeTap => "笔轻点 ×3".to_string(),
            StylusTrigger::FourTap => "笔轻点 ×4".to_string(),
        },
        BindingId::Gesture(gesture) => match gesture {
            GestureBinding::TwoPan => "双指平移".to_string(),
            GestureBinding::ThreePan => "三指平移".to_string(),
            GestureBinding::TwoPinch => "双指捏合".to_string(),
            GestureBinding::TwoRotate => "双指旋转".to_string(),
            GestureBinding::LongPress { fingers } => format!("{fingers} 指长按"),
            GestureBinding::Swipe { fingers, axis } => {
                let axis = match axis {
                    SwipeAxis::Horizontal => "横向",
                    SwipeAxis::Vertical => "纵向",
                };
                format!("{fingers} 指滑动 · {axis}")
            }
        },
    }
}

fn category_label(binding: BindingId) -> &'static str {
    match binding {
        BindingId::Gesture(GestureBinding::TwoPan | GestureBinding::ThreePan) => "Pan",
        BindingId::StylusTrigger(_) => "笔触发",
        BindingId::Gesture(GestureBinding::TwoPinch | GestureBinding::TwoRotate) => {
            "Pinch / Rotate"
        }
        BindingId::Gesture(GestureBinding::LongPress { .. }) => "Long Press",
        BindingId::Gesture(GestureBinding::Swipe { .. }) => "Swipe",
    }
}

fn action_dto(action: &ShortcutAction, profile: &ShortcutProfile) -> ActionDto {
    match action {
        ShortcutAction::Disabled => ActionDto::Disabled,
        ShortcutAction::HoldKeys(keys) => ActionDto::HoldKey {
            keys: keys.iter().map(|key| key.label().to_string()).collect(),
        },
        ShortcutAction::TriggerChord(keys) => ActionDto::TriggerChord {
            keys: keys.iter().map(|key| key.label().to_string()).collect(),
        },
        ShortcutAction::Advanced(action) => ActionDto::Advanced {
            label: advanced_label(action).to_string(),
            detail: advanced_detail(action, profile),
            radial_inner_enabled: radial_inner_enabled(action, profile),
            radial_inner_slots: radial_inner_slots(action, profile),
            radial_outer_slots: radial_outer_slots(action, profile),
        },
    }
}

fn editable_keys(action: &ShortcutAction) -> Option<Vec<String>> {
    match action {
        ShortcutAction::HoldKeys(keys) => {
            Some(keys.iter().map(|key| key.label().to_string()).collect())
        }
        ShortcutAction::TriggerChord(keys) => {
            Some(keys.iter().map(|key| key.label().to_string()).collect())
        }
        ShortcutAction::Advanced(AdvancedAction::PointerDrag { modifiers, .. })
        | ShortcutAction::Advanced(AdvancedAction::PointerWheel { modifiers })
        | ShortcutAction::Advanced(AdvancedAction::PointerRotate { modifiers }) => Some(
            modifiers
                .iter()
                .map(|key| key.label().to_string())
                .collect(),
        ),
        ShortcutAction::Advanced(AdvancedAction::PointerClick { keys, .. }) => {
            Some(keys.iter().map(|key| key.label().to_string()).collect())
        }
        ShortcutAction::Advanced(AdvancedAction::ReleaseActiveKeys)
        | ShortcutAction::Advanced(AdvancedAction::ReservedRadialMenu)
        | ShortcutAction::Disabled => None,
    }
}

fn advanced_label(action: &AdvancedAction) -> &'static str {
    match action {
        AdvancedAction::PointerDrag { button: None, .. } => "按手势坐标移动",
        AdvancedAction::PointerDrag {
            button: Some(_), ..
        } => "指针拖拽",
        AdvancedAction::PointerWheel { .. } => "滚轮缩放",
        AdvancedAction::PointerRotate { .. } => "旋转控制",
        AdvancedAction::PointerClick { button, .. } => match button {
            MouseButton::Left => "左键单击",
            MouseButton::Right => "右键单击",
        },
        AdvancedAction::ReleaseActiveKeys => "释放KeyDown状态键",
        AdvancedAction::ReservedRadialMenu => "径向菜单",
    }
}

fn advanced_detail(action: &AdvancedAction, profile: &ShortcutProfile) -> String {
    match action {
        AdvancedAction::PointerDrag { modifiers, button } => match button {
            Some(button) => format!("{} + {}", join_keys(modifiers), mouse_button_label(*button)),
            None => format!("{} + 按手势坐标移动", join_keys(modifiers)),
        },
        AdvancedAction::PointerWheel { modifiers } => {
            format!("{} + 鼠标滚轮", join_keys(modifiers))
        }
        AdvancedAction::PointerRotate { modifiers } => {
            format!("{} + 鼠标旋转", join_keys(modifiers))
        }
        AdvancedAction::PointerClick {
            keys,
            button,
            anchor,
        } => {
            format!(
                "{} + {}；锚点：{}",
                join_keys(keys),
                mouse_button_label(*button),
                pointer_anchor_label(*anchor)
            )
        }
        AdvancedAction::ReleaseActiveKeys => "释放所有当前按下或切换保持的按键状态".to_string(),
        AdvancedAction::ReservedRadialMenu => {
            let radial_menu = profile.radial_menu();
            let inner = &radial_menu.inner;
            let inner_status = if radial_menu.inner_enabled {
                "启用"
            } else {
                "关闭"
            };
            format!(
                "内环已{}（上 {} / 右 {} / 下 {} / 左 {}），外环 8 槽可编辑：{}",
                inner_status,
                inner.top.label(),
                inner.right.label(),
                inner.bottom.label(),
                inner.left.label(),
                radial_menu
                    .outer
                    .iter()
                    .map(|slot| slot
                        .keys
                        .iter()
                        .map(|key| key.label())
                        .collect::<Vec<_>>()
                        .join("+"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        }
    }
}

fn radial_inner_enabled(action: &AdvancedAction, profile: &ShortcutProfile) -> bool {
    matches!(action, AdvancedAction::ReservedRadialMenu) && profile.radial_menu().inner_enabled
}

fn radial_inner_slots(
    action: &AdvancedAction,
    profile: &ShortcutProfile,
) -> Vec<RadialInnerSlotDto> {
    match action {
        AdvancedAction::ReservedRadialMenu => profile
            .radial_menu()
            .inner
            .slot_entries()
            .into_iter()
            .map(|(slot, key)| RadialInnerSlotDto {
                slot: radial_inner_slot_id(slot).to_string(),
                label: radial_inner_slot_label(slot).to_string(),
                key: key.label().to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn radial_outer_slots(
    action: &AdvancedAction,
    profile: &ShortcutProfile,
) -> Vec<RadialOuterSlotDto> {
    match action {
        AdvancedAction::ReservedRadialMenu => profile
            .radial_menu()
            .outer
            .iter()
            .enumerate()
            .map(|(index, slot)| RadialOuterSlotDto {
                index,
                label: format!("外环 {}", index + 1),
                angle_label: radial_slot_angle_label(index).to_string(),
                keys: slot
                    .keys
                    .iter()
                    .map(|key| key.label().to_string())
                    .collect(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn radial_slot_angle_label(index: usize) -> &'static str {
    match index {
        0 => "上",
        1 => "右上",
        2 => "右",
        3 => "右下",
        4 => "下",
        5 => "左下",
        6 => "左",
        _ => "左上",
    }
}

fn radial_inner_slot_id(slot: RadialInnerSlot) -> &'static str {
    match slot {
        RadialInnerSlot::Top => "top",
        RadialInnerSlot::Right => "right",
        RadialInnerSlot::Bottom => "bottom",
        RadialInnerSlot::Left => "left",
    }
}

fn radial_inner_slot_label(slot: RadialInnerSlot) -> &'static str {
    match slot {
        RadialInnerSlot::Top => "上",
        RadialInnerSlot::Right => "右",
        RadialInnerSlot::Bottom => "下",
        RadialInnerSlot::Left => "左",
    }
}

fn join_keys(keys: &[KeyCode]) -> String {
    keys.iter()
        .map(|key| key.label().to_string())
        .collect::<Vec<_>>()
        .join(" + ")
}

fn mouse_button_label(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "左键",
        MouseButton::Right => "右键",
    }
}

fn active_special_action(action: &ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::Advanced(AdvancedAction::PointerClick {
            button: MouseButton::Left,
            ..
        }) => "pointerClickLeft",
        ShortcutAction::Advanced(AdvancedAction::PointerClick {
            button: MouseButton::Right,
            ..
        }) => "pointerClickRight",
        ShortcutAction::Advanced(AdvancedAction::PointerDrag { button: None, .. }) => "pointerMove",
        ShortcutAction::Advanced(AdvancedAction::PointerDrag {
            button: Some(MouseButton::Left),
            ..
        }) => "pointerDragLeft",
        ShortcutAction::Advanced(AdvancedAction::PointerDrag {
            button: Some(MouseButton::Right),
            ..
        }) => "pointerDragRight",
        ShortcutAction::Advanced(AdvancedAction::PointerWheel { .. }) => "pointerWheel",
        ShortcutAction::Advanced(AdvancedAction::PointerRotate { .. }) => "pointerRotate",
        ShortcutAction::Advanced(AdvancedAction::ReservedRadialMenu) => "radialMenu",
        _ => "none",
    }
}

fn special_action_options(binding: BindingId) -> Vec<SpecialActionOptionDto> {
    let options: &[(&str, &str)] = match binding {
        BindingId::StylusTrigger(
            StylusTrigger::Squeeze
            | StylusTrigger::DoubleTap
            | StylusTrigger::TwoTap
            | StylusTrigger::ThreeTap,
        ) => &[
            ("none", "无特殊动作"),
            ("pointerClickLeft", "鼠标左键"),
            ("pointerClickRight", "鼠标右键"),
        ],
        BindingId::Gesture(GestureBinding::TwoPan | GestureBinding::ThreePan) => &[
            ("none", "无特殊动作"),
            ("radialMenu", "径向菜单"),
            ("pointerMove", "按手势坐标移动"),
            ("pointerDragLeft", "按住左键移动"),
            ("pointerDragRight", "按住右键移动"),
        ],
        BindingId::Gesture(GestureBinding::TwoPinch) => {
            &[("none", "无特殊动作"), ("pointerWheel", "鼠标滚轮")]
        }
        BindingId::Gesture(GestureBinding::TwoRotate) => {
            &[("none", "无特殊动作"), ("pointerRotate", "按旋转角度移动")]
        }
        _ => &[],
    };
    options
        .iter()
        .map(|(id, label)| SpecialActionOptionDto {
            id: (*id).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

fn pointer_anchor_label(anchor: PointerAnchor) -> &'static str {
    match anchor {
        PointerAnchor::CurrentHoverOrLastInRange => "当前 hover 或最后有效坐标",
    }
}

impl From<crate::config::PressureCurve> for PressureCurveDto {
    fn from(value: crate::config::PressureCurve) -> Self {
        Self {
            control_point1: PressureControlPointDto {
                x: value.control_point1.x,
                y: value.control_point1.y,
            },
            control_point2: PressureControlPointDto {
                x: value.control_point2.x,
                y: value.control_point2.y,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcut::{
        AdvancedAction, BindingId, ShortcutAction, ShortcutProfile, StylusTrigger,
    };

    #[test]
    fn four_tap_binding_is_exposed_as_fixed_release_action() {
        let profile = ShortcutProfile::default();
        let binding = binding_dtos(&profile, false)
            .into_iter()
            .find(|binding| {
                binding.id == BindingId::StylusTrigger(StylusTrigger::FourTap).persisted_key()
            })
            .expect("four tap binding should exist");

        assert!(binding.editable_keys.is_none());
        match binding.current_action {
            ActionDto::Advanced { label, detail, .. } => {
                assert_eq!(label, "释放KeyDown状态键");
                assert_eq!(detail, "释放所有当前按下或切换保持的按键状态");
            }
            other => panic!("expected advanced action, got {other:?}"),
        }
        assert_eq!(
            profile.action_for(BindingId::StylusTrigger(StylusTrigger::FourTap)),
            ShortcutAction::Advanced(AdvancedAction::ReleaseActiveKeys)
        );
    }
}
