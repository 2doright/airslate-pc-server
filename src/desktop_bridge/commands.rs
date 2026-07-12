use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

use super::dto::{
    AppBootstrapDto, PressureControlPointDto, PressureCurvePayload, app_bootstrap, parse_binding_id,
};
use super::local_ip::lan_ipv4_values;
use crate::{
    app::AppContext,
    config::{PressureCurve, PressureCurveControlPoint},
    error::AppError,
    shortcut::{KeyCode, RadialInnerBindings, SpecialAction},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingKeysPayload {
    pub binding_id: String,
    pub keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingSpecialActionPayload {
    pub binding_id: String,
    pub special_action: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadialOuterSlotPayload {
    pub index: usize,
    pub keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadialInnerBindingsPayload {
    pub top: String,
    pub right: String,
    pub bottom: String,
    pub left: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadialInnerEnabledPayload {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSelectionPayload {
    pub preset_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetNamePayload {
    pub preset_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePresetPayload {
    pub name: String,
}

#[tauri::command]
pub fn get_app_bootstrap(state: State<'_, AppContext>) -> Result<AppBootstrapDto, String> {
    app_bootstrap(&state.runtime, &state.config_path.display().to_string()).map_err(error_message)
}

#[tauri::command]
pub fn disconnect_active_session(
    state: State<'_, AppContext>,
) -> Result<crate::app::lifecycle::SessionStatusEvent, String> {
    state
        .session_lifecycle
        .disconnect_locally()
        .map_err(error_message)
}

#[tauri::command]
pub fn get_lan_ipv4_values() -> Vec<String> {
    lan_ipv4_values()
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("仅支持打开 HTTPS 外部链接".to_string());
    }

    app.opener()
        .open_url(&url, None::<String>)
        .map_err(|error| format!("打开链接失败: {error}"))
}

#[tauri::command]
pub fn set_selected_monitor(
    state: State<'_, AppContext>,
    monitor_id: String,
) -> Result<(), String> {
    state
        .runtime
        .set_selected_monitor(monitor_id)
        .map_err(error_message)
}

#[tauri::command]
pub fn set_pressure_curve(
    state: State<'_, AppContext>,
    payload: PressureCurvePayload,
) -> Result<(), String> {
    state
        .runtime
        .set_pressure_curve(PressureCurve {
            control_point1: to_pressure_curve_point(payload.control_point1),
            control_point2: to_pressure_curve_point(payload.control_point2),
        })
        .map_err(error_message)
}

#[tauri::command]
pub fn set_launch_at_startup(
    app: AppHandle,
    state: State<'_, AppContext>,
    enabled: bool,
) -> Result<(), String> {
    set_autostart_enabled(&app, enabled)?;

    if let Err(error) = state.runtime.set_launch_at_startup(enabled) {
        return match set_autostart_enabled(&app, !enabled) {
            Ok(()) => Err(error_message(error)),
            Err(rollback_error) => Err(format!(
                "{}; autostart rollback failed: {rollback_error}",
                error_message(error)
            )),
        };
    }

    Ok(())
}

#[tauri::command]
pub fn set_show_launch_at_startup_on_main_page(
    state: State<'_, AppContext>,
    enabled: bool,
) -> Result<(), String> {
    state
        .runtime
        .set_show_launch_at_startup_on_main_page(enabled)
        .map_err(error_message)
}

#[tauri::command]
pub fn set_latest_contact_move_only(
    state: State<'_, AppContext>,
    enabled: bool,
) -> Result<(), String> {
    state
        .runtime
        .set_latest_contact_move_only(enabled)
        .map_err(error_message)
}

#[tauri::command]
pub fn set_latest_contact_move_tolerance_ms(
    state: State<'_, AppContext>,
    tolerance_ms: u32,
) -> Result<(), String> {
    state
        .runtime
        .set_latest_contact_move_tolerance_ms(tolerance_ms)
        .map_err(error_message)
}

#[tauri::command]
pub fn set_preempt_previous_stroke(
    state: State<'_, AppContext>,
    enabled: bool,
) -> Result<(), String> {
    state
        .runtime
        .set_preempt_previous_stroke(enabled)
        .map_err(error_message)
}

#[tauri::command]
pub fn select_shortcut_preset(
    state: State<'_, AppContext>,
    payload: PresetSelectionPayload,
) -> Result<(), String> {
    state
        .runtime
        .select_shortcut_preset(&payload.preset_id)
        .map_err(error_message)
}

#[tauri::command]
pub fn create_shortcut_preset(
    state: State<'_, AppContext>,
    payload: CreatePresetPayload,
) -> Result<(), String> {
    state
        .runtime
        .create_shortcut_preset(payload.name)
        .map(|_| ())
        .map_err(error_message)
}

#[tauri::command]
pub fn rename_shortcut_preset(
    state: State<'_, AppContext>,
    payload: PresetNamePayload,
) -> Result<(), String> {
    state
        .runtime
        .rename_shortcut_preset(&payload.preset_id, payload.name)
        .map_err(error_message)
}

#[tauri::command]
pub fn delete_shortcut_preset(
    state: State<'_, AppContext>,
    payload: PresetSelectionPayload,
) -> Result<(), String> {
    state
        .runtime
        .delete_shortcut_preset(&payload.preset_id)
        .map_err(error_message)
}

#[tauri::command]
pub fn reset_shortcut_preset(
    state: State<'_, AppContext>,
    payload: PresetSelectionPayload,
) -> Result<(), String> {
    state
        .runtime
        .reset_shortcut_preset(&payload.preset_id)
        .map_err(error_message)
}

#[tauri::command]
pub fn set_binding_keys(
    state: State<'_, AppContext>,
    payload: BindingKeysPayload,
) -> Result<(), String> {
    let binding = parse_binding_id(&payload.binding_id).map_err(error_message)?;
    let keys = payload
        .keys
        .iter()
        .map(|key| parse_key_code(key))
        .collect::<Result<Vec<_>, _>>()?;
    state
        .runtime
        .set_binding_keys(binding, keys)
        .map_err(error_message)
}

#[tauri::command]
pub fn set_binding_special_action(
    state: State<'_, AppContext>,
    payload: BindingSpecialActionPayload,
) -> Result<(), String> {
    let binding = parse_binding_id(&payload.binding_id).map_err(error_message)?;
    let special_action = SpecialAction::parse(&payload.special_action)
        .ok_or_else(|| format!("unknown special action: {}", payload.special_action))?;
    state
        .runtime
        .set_binding_special_action(binding, special_action)
        .map_err(error_message)
}

#[tauri::command]
pub fn set_radial_outer_slot(
    state: State<'_, AppContext>,
    payload: RadialOuterSlotPayload,
) -> Result<(), String> {
    let keys = payload
        .keys
        .iter()
        .map(|key| parse_key_code(key))
        .collect::<Result<Vec<_>, _>>()?;
    state
        .runtime
        .set_radial_outer_binding(payload.index, keys)
        .map_err(error_message)
}

#[tauri::command]
pub fn set_radial_inner_bindings(
    state: State<'_, AppContext>,
    payload: RadialInnerBindingsPayload,
) -> Result<(), String> {
    state
        .runtime
        .set_radial_inner_bindings(RadialInnerBindings {
            top: parse_key_code(&payload.top)?,
            right: parse_key_code(&payload.right)?,
            bottom: parse_key_code(&payload.bottom)?,
            left: parse_key_code(&payload.left)?,
        })
        .map_err(error_message)
}

#[tauri::command]
pub fn set_radial_inner_enabled(
    state: State<'_, AppContext>,
    payload: RadialInnerEnabledPayload,
) -> Result<(), String> {
    state
        .runtime
        .set_radial_inner_enabled(payload.enabled)
        .map_err(error_message)
}

fn to_pressure_curve_point(value: PressureControlPointDto) -> PressureCurveControlPoint {
    PressureCurveControlPoint {
        x: value.x,
        y: value.y,
    }
}

fn parse_key_code(value: &str) -> Result<KeyCode, String> {
    KeyCode::parse(value).ok_or_else(|| format!("unknown key code: {value}"))
}

fn set_autostart_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable().map_err(|error| error.to_string())
    } else {
        autostart.disable().map_err(|error| error.to_string())
    }
}

fn error_message(error: AppError) -> String {
    error.to_string()
}
