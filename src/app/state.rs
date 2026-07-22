use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
};

use crate::{
    config::{Config, HoverMovePolicy, PressureCurve, UsbInterface},
    error::AppError,
    session::SharedSessionService,
    shortcut::{
        AdvancedAction, BindingId, GestureBinding, KeyCode, MouseButton, PointerAnchor,
        RadialInnerBindings, SharedShortcutProfile, ShortcutAction, ShortcutPreset,
        ShortcutPresetLibrary, ShortcutProfile, SpecialAction, StylusTrigger,
    },
    workspace::WorkspaceService,
};

pub type SharedConfigState = Arc<Mutex<Config>>;
pub type SharedPressureSettings = Arc<RwLock<PressureSettings>>;
pub type SharedInputProcessingSettings = Arc<InputProcessingSettings>;

#[derive(Default)]
pub struct InputProcessingSettings {
    pub latest_contact_move_only: AtomicBool,
    pub latest_contact_move_tolerance_ms: AtomicU64,
    pub hover_move_policy: AtomicU8,
    pub preempt_previous_stroke: AtomicBool,
}

impl InputProcessingSettings {
    fn from_config(config: &Config) -> Self {
        Self {
            latest_contact_move_only: AtomicBool::new(config.latest_contact_move_only),
            latest_contact_move_tolerance_ms: AtomicU64::new(u64::from(
                config.latest_contact_move_tolerance_ms,
            )),
            hover_move_policy: AtomicU8::new(config.hover_move_policy.level()),
            preempt_previous_stroke: AtomicBool::new(config.preempt_previous_stroke),
        }
    }
}

pub const PRESSURE_LUT_SIZE: usize = 1025;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressureControlPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PressureSettings {
    pub control_point1: PressureControlPoint,
    pub control_point2: PressureControlPoint,
    pub lut: [u16; PRESSURE_LUT_SIZE],
}

impl PressureSettings {
    pub fn new(control_point1: PressureControlPoint, control_point2: PressureControlPoint) -> Self {
        let mut settings = Self {
            control_point1,
            control_point2,
            lut: [0; PRESSURE_LUT_SIZE],
        };
        settings.normalize();
        settings.rebuild_lut();
        settings
    }

    pub fn from_curve(curve: PressureCurve) -> Self {
        Self::new(
            PressureControlPoint {
                x: curve.control_point1.x,
                y: curve.control_point1.y,
            },
            PressureControlPoint {
                x: curve.control_point2.x,
                y: curve.control_point2.y,
            },
        )
    }

    pub fn from_config(config: &Config) -> Self {
        Self::from_curve(config.pressure_curve)
    }

    pub fn update_curve(&mut self, curve: PressureCurve) {
        self.control_point1 = PressureControlPoint {
            x: curve.control_point1.x,
            y: curve.control_point1.y,
        };
        self.control_point2 = PressureControlPoint {
            x: curve.control_point2.x,
            y: curve.control_point2.y,
        };
        self.normalize();
        self.rebuild_lut();
    }

    pub fn map_pressure(&self, value: f32) -> u16 {
        let normalized = value.clamp(0.0, 1.0);
        let index = (normalized * (PRESSURE_LUT_SIZE as f32 - 1.0)).round() as usize;
        self.lut[index.min(PRESSURE_LUT_SIZE - 1)]
    }

    fn normalize(&mut self) {
        self.control_point1.x = self.control_point1.x.clamp(0.0, 1.0);
        self.control_point1.y = self.control_point1.y.clamp(0.0, 1.0);
        self.control_point2.x = self.control_point2.x.clamp(0.0, 1.0);
        self.control_point2.y = self.control_point2.y.clamp(0.0, 1.0);
    }

    fn rebuild_lut(&mut self) {
        for index in 0..PRESSURE_LUT_SIZE {
            let x = index as f32 / (PRESSURE_LUT_SIZE as f32 - 1.0);
            let y = cubic_bezier_y_for_x(x, self.control_point1, self.control_point2);
            self.lut[index] = (y.clamp(0.0, 1.0) * 1024.0).round() as u16;
        }
    }
}

fn cubic_bezier_y_for_x(
    x: f32,
    control_point1: PressureControlPoint,
    control_point2: PressureControlPoint,
) -> f32 {
    let mut low = 0.0;
    let mut high = 1.0;

    for _ in 0..20 {
        let t = (low + high) * 0.5;
        let current_x = cubic_bezier(t, 0.0, control_point1.x, control_point2.x, 1.0);
        if current_x < x {
            low = t;
        } else {
            high = t;
        }
    }

    let t = (low + high) * 0.5;
    cubic_bezier(t, 0.0, control_point1.y, control_point2.y, 1.0)
}

fn cubic_bezier(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let one_minus_t = 1.0 - t;
    one_minus_t.powi(3) * p0
        + 3.0 * one_minus_t.powi(2) * t * p1
        + 3.0 * one_minus_t * t.powi(2) * p2
        + t.powi(3) * p3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        protocol::StylusFlags,
        session::SessionService,
        shortcut::{ShortcutRuntime, StylusTrigger},
        workspace::{ActiveWorkspace, MonitorId, MonitorInfo, WorkspaceSnapshot},
    };
    use std::{
        env, fs,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Default)]
    struct RecordingShortcutExecutor {
        commands: Mutex<Vec<crate::shortcut::ShortcutCommand>>,
    }

    impl crate::shortcut::ShortcutExecutor for RecordingShortcutExecutor {
        fn execute(&self, command: crate::shortcut::ShortcutCommand) -> Result<(), AppError> {
            self.commands
                .lock()
                .map_err(|_| AppError::StatePoisoned("recording_shortcut_executor"))?
                .push(command);
            Ok(())
        }
    }

    fn test_workspace() -> WorkspaceService {
        WorkspaceService::from_snapshot(WorkspaceSnapshot {
            monitors: vec![MonitorInfo {
                id: MonitorId::new("monitor-1".to_string()),
                device_name: "DISPLAY1".to_string(),
                is_primary: true,
                pixel_width: 1920,
                pixel_height: 1080,
                virtual_left: 0,
                virtual_top: 0,
                virtual_right: 1920,
                virtual_bottom: 1080,
            }],
            active_monitor_id: Some(MonitorId::new("monitor-1".to_string())),
            active_workspace: Some(ActiveWorkspace {
                monitor: MonitorInfo {
                    id: MonitorId::new("monitor-1".to_string()),
                    device_name: "DISPLAY1".to_string(),
                    is_primary: true,
                    pixel_width: 1920,
                    pixel_height: 1080,
                    virtual_left: 0,
                    virtual_top: 0,
                    virtual_right: 1920,
                    virtual_bottom: 1080,
                },
            }),
        })
    }

    fn test_config_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        env::temp_dir().join(format!("airslate_pc_server_{name}_{unique}.toml"))
    }

    #[test]
    fn default_curve_generates_near_linear_lut() {
        let settings = PressureSettings::from_curve(PressureCurve::default());

        assert_eq!(settings.lut[0], 0);
        assert_eq!(settings.lut[PRESSURE_LUT_SIZE - 1], 1024);
        assert!((i32::from(settings.lut[PRESSURE_LUT_SIZE / 2]) - 512).abs() <= 1);
    }

    #[test]
    fn set_usb_interface_persists_and_updates_runtime_snapshot() {
        let config_path = test_config_path("usb_interface_round_trip");
        let runtime = AppRuntime::new(
            config_path.clone(),
            Config::default(),
            test_workspace(),
            SessionService::shared(),
        );
        let configured = UsbInterface::new(0x12, 0x34, 0x56);

        runtime
            .set_usb_interface(configured)
            .expect("USB interface should persist");

        assert_eq!(
            runtime.usb_interface().expect("USB interface snapshot"),
            configured
        );
        assert_eq!(
            Config::load(&config_path)
                .expect("saved config should load")
                .usb_interface,
            configured
        );

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn set_binding_keys_persists_and_reloads_without_serialization_failure() {
        let config_path = test_config_path("binding_keys_round_trip");
        let config = Config::default();
        let runtime = AppRuntime::new(
            config_path.clone(),
            config,
            test_workspace(),
            SessionService::shared(),
        );

        runtime
            .set_binding_keys(
                BindingId::StylusTrigger(StylusTrigger::TwoTap),
                vec![KeyCode::Control, KeyCode::Y],
            )
            .expect("binding keys should persist");

        let restored = Config::load(&config_path).expect("saved config should load");
        let active = restored
            .shortcut_presets
            .active()
            .expect("active preset should exist");

        assert_eq!(
            active
                .profile
                .action_for(BindingId::StylusTrigger(StylusTrigger::TwoTap)),
            ShortcutAction::TriggerChord(vec![KeyCode::Control, KeyCode::Y])
        );

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn set_radial_inner_bindings_persists_and_updates_runtime_profile() {
        let config_path = test_config_path("radial_inner_round_trip");
        let config = Config::default();
        let runtime = AppRuntime::new(
            config_path.clone(),
            config,
            test_workspace(),
            SessionService::shared(),
        );
        let inner = RadialInnerBindings {
            top: KeyCode::Alt,
            right: KeyCode::Shift,
            bottom: KeyCode::Space,
            left: KeyCode::Control,
        };

        runtime
            .set_radial_inner_bindings(inner.clone())
            .expect("radial inner bindings should persist");

        let restored = Config::load(&config_path).expect("saved config should load");
        let active = restored
            .shortcut_presets
            .active()
            .expect("active preset should exist");
        assert_eq!(active.profile.radial_menu.inner, inner);
        assert_eq!(
            runtime
                .shortcut_profile()
                .read()
                .expect("shortcut profile lock should succeed")
                .radial_menu
                .inner,
            inner
        );

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn set_radial_inner_bindings_rejects_duplicate_keys() {
        let config_path = test_config_path("radial_inner_duplicate");
        let config = Config::default();
        let runtime = AppRuntime::new(
            config_path.clone(),
            config,
            test_workspace(),
            SessionService::shared(),
        );

        let result = runtime.set_radial_inner_bindings(RadialInnerBindings {
            top: KeyCode::Alt,
            right: KeyCode::Alt,
            bottom: KeyCode::Space,
            left: KeyCode::Control,
        });

        assert!(result.is_err());
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn set_radial_inner_enabled_persists_and_updates_runtime_profile() {
        let config_path = test_config_path("radial_inner_enabled_round_trip");
        let config = Config::default();
        let runtime = AppRuntime::new(
            config_path.clone(),
            config,
            test_workspace(),
            SessionService::shared(),
        );

        runtime
            .set_radial_inner_enabled(false)
            .expect("radial inner enabled flag should persist");

        let restored = Config::load(&config_path).expect("saved config should load");
        let active = restored
            .shortcut_presets
            .active()
            .expect("active preset should exist");
        assert!(!active.profile.radial_menu.inner_enabled);
        assert!(
            !runtime
                .shortcut_profile()
                .read()
                .expect("shortcut profile lock should succeed")
                .radial_menu
                .inner_enabled
        );

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn edited_binding_is_used_by_runtime_shortcut_executor() {
        let config_path = test_config_path("runtime_uses_edited_keys");
        let config = Config::default();
        let runtime = AppRuntime::new(
            config_path.clone(),
            config,
            test_workspace(),
            SessionService::shared(),
        );

        runtime
            .set_binding_keys(
                BindingId::StylusTrigger(StylusTrigger::TwoTap),
                vec![KeyCode::Control, KeyCode::Y],
            )
            .expect("binding keys should persist");

        let restored = Config::load(&config_path).expect("config should load");
        let profile = restored.shortcut_profile;
        let executor = Arc::new(RecordingShortcutExecutor::default());
        let runtime = ShortcutRuntime::new(
            executor.clone(),
            profile.clone().shared(),
            crate::shortcut::null_radial_menu_overlay(),
            test_workspace(),
        );

        runtime.handle_stylus_flags("session-1", 1, StylusFlags(0b0001_0000));

        assert_eq!(
            profile.action_for(BindingId::StylusTrigger(StylusTrigger::TwoTap)),
            ShortcutAction::TriggerChord(vec![KeyCode::Control, KeyCode::Y])
        );
        assert_eq!(
            executor
                .commands
                .lock()
                .expect("commands lock should succeed")
                .clone(),
            vec![crate::shortcut::ShortcutCommand::PressChord(vec![
                KeyCode::Control,
                KeyCode::Y
            ])]
        );

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn four_tap_binding_rejects_key_editing() {
        let config_path = test_config_path("four_tap_locked_binding");
        let config = Config::default();
        let runtime = AppRuntime::new(
            config_path.clone(),
            config,
            test_workspace(),
            SessionService::shared(),
        );

        let result = runtime.set_binding_keys(
            BindingId::StylusTrigger(StylusTrigger::FourTap),
            vec![KeyCode::Delete],
        );

        assert!(result.is_err());
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn keyboard_keys_and_special_action_are_updated_independently() {
        let config_path = test_config_path("combined_keyboard_and_special_action");
        let runtime = AppRuntime::new(
            config_path.clone(),
            Config::default(),
            test_workspace(),
            SessionService::shared(),
        );
        let binding = BindingId::StylusTrigger(StylusTrigger::TwoTap);

        runtime
            .set_binding_keys(binding, vec![KeyCode::Control, KeyCode::K])
            .expect("keyboard keys should save");
        runtime
            .set_binding_special_action(binding, SpecialAction::PointerClickLeft)
            .expect("special action should save");
        assert_eq!(
            runtime
                .config_snapshot()
                .expect("config snapshot")
                .shortcut_profile
                .action_for(binding),
            ShortcutAction::Advanced(AdvancedAction::PointerClick {
                keys: vec![KeyCode::Control, KeyCode::K],
                button: MouseButton::Left,
                anchor: PointerAnchor::CurrentHoverOrLastInRange,
            })
        );

        runtime
            .set_binding_keys(binding, Vec::new())
            .expect("keyboard keys may be cleared");
        assert_eq!(
            runtime
                .config_snapshot()
                .expect("config snapshot")
                .shortcut_profile
                .action_for(binding),
            ShortcutAction::Advanced(AdvancedAction::PointerClick {
                keys: Vec::new(),
                button: MouseButton::Left,
                anchor: PointerAnchor::CurrentHoverOrLastInRange,
            })
        );

        let _ = fs::remove_file(config_path);
    }
}

#[derive(Clone)]
pub struct AppRuntime {
    config_path: PathBuf,
    config: SharedConfigState,
    workspace: WorkspaceService,
    pressure_settings: SharedPressureSettings,
    shortcut_profile: SharedShortcutProfile,
    input_processing_settings: SharedInputProcessingSettings,
    session: SharedSessionService,
}

impl AppRuntime {
    pub fn new(
        config_path: PathBuf,
        config: Config,
        workspace: WorkspaceService,
        session: SharedSessionService,
    ) -> Self {
        let pressure_settings = Arc::new(RwLock::new(PressureSettings::from_config(&config)));
        let shortcut_profile = config.shortcut_profile.clone().shared();

        let input_processing_settings = Arc::new(InputProcessingSettings::from_config(&config));
        Self {
            config_path,
            config: Arc::new(Mutex::new(config)),
            workspace,
            pressure_settings,
            shortcut_profile,
            input_processing_settings,
            session,
        }
    }

    pub fn workspace(&self) -> WorkspaceService {
        self.workspace.clone()
    }

    pub fn pressure_settings(&self) -> SharedPressureSettings {
        self.pressure_settings.clone()
    }

    pub fn shortcut_profile(&self) -> SharedShortcutProfile {
        self.shortcut_profile.clone()
    }

    pub fn input_processing_settings(&self) -> SharedInputProcessingSettings {
        self.input_processing_settings.clone()
    }

    pub fn has_active_session(&self) -> Result<bool, AppError> {
        self.session
            .lock()
            .map_err(|_| AppError::StatePoisoned("session"))
            .map(|session| session.has_active_session())
    }

    pub fn config_snapshot(&self) -> Result<Config, AppError> {
        self.config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))
            .map(|config| config.clone())
    }

    pub fn usb_interface(&self) -> Result<UsbInterface, AppError> {
        self.config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))
            .map(|config| config.usb_interface)
    }

    pub fn shortcut_presets_snapshot(&self) -> Result<ShortcutPresetLibrary, AppError> {
        self.config_snapshot().map(|config| config.shortcut_presets)
    }

    pub fn set_selected_monitor(&self, monitor_id: String) -> Result<(), AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.selected_monitor_id = Some(monitor_id);
        config.normalize();
        config.save(&self.config_path)?;
        self.workspace.refresh(&config)
    }

    pub fn set_pressure_curve(&self, curve: PressureCurve) -> Result<(), AppError> {
        {
            let mut settings = self
                .pressure_settings
                .write()
                .map_err(|_| AppError::StatePoisoned("pressure_settings"))?;
            settings.update_curve(curve);
        }

        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.pressure_curve = curve;
        config.normalize();
        config.save(&self.config_path)
    }

    pub fn set_launch_at_startup(&self, enabled: bool) -> Result<(), AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.launch_at_startup = enabled;
        config.normalize();
        config.save(&self.config_path)
    }

    pub fn set_show_launch_at_startup_on_main_page(&self, enabled: bool) -> Result<(), AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.show_launch_at_startup_on_main_page = enabled;
        config.normalize();
        config.save(&self.config_path)
    }

    pub fn set_latest_contact_move_only(&self, enabled: bool) -> Result<(), AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.latest_contact_move_only = enabled;
        config.normalize();
        config.save(&self.config_path)?;
        self.input_processing_settings
            .latest_contact_move_only
            .store(enabled, Ordering::Release);
        Ok(())
    }

    pub fn set_latest_contact_move_tolerance_ms(&self, tolerance_ms: u32) -> Result<(), AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.latest_contact_move_tolerance_ms = tolerance_ms;
        config.normalize();
        config.save(&self.config_path)?;
        self.input_processing_settings
            .latest_contact_move_tolerance_ms
            .store(
                u64::from(config.latest_contact_move_tolerance_ms),
                Ordering::Release,
            );
        Ok(())
    }

    pub fn set_hover_move_policy(&self, policy: HoverMovePolicy) -> Result<(), AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.hover_move_policy = policy;
        config.normalize();
        config.save(&self.config_path)?;
        self.input_processing_settings
            .hover_move_policy
            .store(policy.level(), Ordering::Release);
        Ok(())
    }

    pub fn set_preempt_previous_stroke(&self, enabled: bool) -> Result<(), AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.preempt_previous_stroke = enabled;
        config.normalize();
        config.save(&self.config_path)?;
        self.input_processing_settings
            .preempt_previous_stroke
            .store(enabled, Ordering::Release);
        Ok(())
    }

    pub fn set_usb_interface(&self, interface: UsbInterface) -> Result<(), AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        config.usb_interface = interface;
        config.normalize();
        config.save(&self.config_path)
    }

    pub fn select_shortcut_preset(&self, preset_id: &str) -> Result<(), AppError> {
        self.mutate_presets(|presets| {
            presets
                .select(preset_id)
                .then_some(())
                .ok_or_else(|| AppError::ShortcutPreset(format!("unknown preset id: {preset_id}")))
        })
    }

    pub fn create_shortcut_preset(&self, name: String) -> Result<ShortcutPreset, AppError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AppError::ShortcutPreset(
                "preset name cannot be empty".to_string(),
            ));
        }

        self.mutate_presets(|presets| Ok(presets.create(trimmed.to_string()).clone()))
    }

    pub fn rename_shortcut_preset(&self, preset_id: &str, name: String) -> Result<(), AppError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AppError::ShortcutPreset(
                "preset name cannot be empty".to_string(),
            ));
        }

        self.mutate_presets(|presets| {
            presets
                .rename(preset_id, trimmed.to_string())
                .then_some(())
                .ok_or_else(|| AppError::ShortcutPreset(format!("unknown preset id: {preset_id}")))
        })
    }

    pub fn delete_shortcut_preset(&self, preset_id: &str) -> Result<(), AppError> {
        if preset_id == "default" {
            return Err(AppError::ShortcutPreset(
                "default preset cannot be deleted".to_string(),
            ));
        }

        self.mutate_presets(|presets| {
            presets
                .remove(preset_id)
                .then_some(())
                .ok_or_else(|| AppError::ShortcutPreset(format!("unknown preset id: {preset_id}")))
        })
    }

    pub fn reset_shortcut_preset(&self, preset_id: &str) -> Result<(), AppError> {
        self.mutate_presets(|presets| {
            presets
                .reset(preset_id)
                .then_some(())
                .ok_or_else(|| AppError::ShortcutPreset(format!("unknown preset id: {preset_id}")))
        })
    }

    pub fn set_binding_keys(&self, binding: BindingId, keys: Vec<KeyCode>) -> Result<(), AppError> {
        let keys = normalize_binding_keys(keys);
        self.mutate_active_profile(|profile| {
            let action = keyboard_action_from_keys(profile, binding, keys)?;
            profile.set_custom_binding(binding, action);
            Ok(())
        })
    }

    pub fn set_binding_special_action(
        &self,
        binding: BindingId,
        special_action: SpecialAction,
    ) -> Result<(), AppError> {
        self.mutate_active_profile(|profile| {
            let action = special_action_for_binding(profile, binding, special_action)?;
            profile.set_custom_binding(binding, action);
            Ok(())
        })
    }

    pub fn set_radial_outer_binding(
        &self,
        index: usize,
        keys: Vec<KeyCode>,
    ) -> Result<(), AppError> {
        let keys = normalize_keys(keys)?;
        self.mutate_active_profile(|profile| {
            let Some(slot) = profile.radial_menu.outer.get_mut(index) else {
                return Err(AppError::DesktopShell(format!(
                    "radial outer slot index out of range: {index}"
                )));
            };
            slot.keys = keys;
            Ok(())
        })
    }

    pub fn set_radial_inner_bindings(&self, inner: RadialInnerBindings) -> Result<(), AppError> {
        validate_radial_inner_bindings(&inner)?;
        self.mutate_active_profile(|profile| {
            profile.radial_menu.inner = inner;
            Ok(())
        })
    }

    pub fn set_radial_inner_enabled(&self, inner_enabled: bool) -> Result<(), AppError> {
        self.mutate_active_profile(|profile| {
            profile.radial_menu.inner_enabled = inner_enabled;
            Ok(())
        })
    }

    fn mutate_active_profile<T>(
        &self,
        mutate: impl FnOnce(&mut ShortcutProfile) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;

        let Some(active) = config.shortcut_presets.active_mut() else {
            return Err(AppError::ShortcutPreset(
                "active preset is missing from preset library".to_string(),
            ));
        };

        let result = mutate(&mut active.profile)?;
        config.shortcut_profile = active.profile.clone();
        config.normalize();
        config.save(&self.config_path)?;
        self.replace_shortcut_profile(config.shortcut_profile.clone())?;
        Ok(result)
    }

    fn mutate_presets<T>(
        &self,
        mutate: impl FnOnce(&mut ShortcutPresetLibrary) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AppError::StatePoisoned("config"))?;
        let result = mutate(&mut config.shortcut_presets)?;
        if let Some(active) = config.shortcut_presets.active() {
            config.shortcut_profile = active.profile.clone();
        }
        config.normalize();
        config.save(&self.config_path)?;
        self.replace_shortcut_profile(config.shortcut_profile.clone())?;
        Ok(result)
    }

    fn replace_shortcut_profile(&self, profile: ShortcutProfile) -> Result<(), AppError> {
        let mut shared = self
            .shortcut_profile
            .write()
            .map_err(|_| AppError::StatePoisoned("shortcut_profile"))?;
        *shared = profile;
        Ok(())
    }
}

fn normalize_keys(mut keys: Vec<KeyCode>) -> Result<Vec<KeyCode>, AppError> {
    if keys.is_empty() {
        return Err(AppError::DesktopShell(
            "shortcut keys cannot be empty".to_string(),
        ));
    }
    keys.sort_by_key(|key| key.sort_rank());
    keys.dedup();
    Ok(keys)
}

fn normalize_binding_keys(mut keys: Vec<KeyCode>) -> Vec<KeyCode> {
    keys.sort_by_key(|key| key.sort_rank());
    keys.dedup();
    keys
}

fn validate_radial_inner_bindings(inner: &RadialInnerBindings) -> Result<(), AppError> {
    let mut keys = vec![inner.top, inner.right, inner.bottom, inner.left];
    keys.sort_by_key(|key| key.sort_rank());
    keys.dedup();
    if keys.len() != 4 {
        return Err(AppError::DesktopShell(
            "radial inner bindings must contain four distinct keys".to_string(),
        ));
    }
    Ok(())
}

fn keyboard_action_from_keys(
    profile: &ShortcutProfile,
    binding: BindingId,
    keys: Vec<KeyCode>,
) -> Result<ShortcutAction, AppError> {
    match profile.action_for(binding) {
        ShortcutAction::Advanced(AdvancedAction::PointerClick { button, anchor, .. }) => {
            return Ok(ShortcutAction::Advanced(AdvancedAction::PointerClick {
                keys,
                button,
                anchor,
            }));
        }
        ShortcutAction::Advanced(AdvancedAction::PointerDrag { button, .. }) => {
            return Ok(ShortcutAction::Advanced(AdvancedAction::PointerDrag {
                modifiers: keys,
                button,
            }));
        }
        ShortcutAction::Advanced(AdvancedAction::PointerWheel { .. }) => {
            return Ok(ShortcutAction::Advanced(AdvancedAction::PointerWheel {
                modifiers: keys,
            }));
        }
        ShortcutAction::Advanced(AdvancedAction::PointerRotate { .. }) => {
            return Ok(ShortcutAction::Advanced(AdvancedAction::PointerRotate {
                modifiers: keys,
            }));
        }
        _ => {}
    }
    match binding {
        BindingId::StylusTrigger(StylusTrigger::FourTap) => Err(AppError::DesktopShell(
            "this binding has a fixed system action".to_string(),
        )),
        BindingId::Gesture(GestureBinding::LongPress { .. })
        | BindingId::Gesture(GestureBinding::ThreePan)
        | BindingId::Gesture(GestureBinding::TwoPinch)
        | BindingId::Gesture(GestureBinding::TwoRotate) => Ok(ShortcutAction::HoldKeys(keys)),
        BindingId::StylusTrigger(_) | BindingId::Gesture(GestureBinding::Swipe { .. }) => {
            Ok(ShortcutAction::TriggerChord(keys))
        }
    }
}

fn special_action_for_binding(
    profile: &ShortcutProfile,
    binding: BindingId,
    special_action: SpecialAction,
) -> Result<ShortcutAction, AppError> {
    let keys = current_keys(profile, binding);
    if special_action == SpecialAction::None {
        return keyboard_action_without_special(binding, keys);
    }
    let action = match (binding, special_action) {
        (
            BindingId::StylusTrigger(
                StylusTrigger::Squeeze
                | StylusTrigger::DoubleTap
                | StylusTrigger::TwoTap
                | StylusTrigger::ThreeTap,
            ),
            SpecialAction::PointerClickLeft,
        ) => AdvancedAction::PointerClick {
            keys,
            button: MouseButton::Left,
            anchor: PointerAnchor::CurrentHoverOrLastInRange,
        },
        (
            BindingId::StylusTrigger(
                StylusTrigger::Squeeze
                | StylusTrigger::DoubleTap
                | StylusTrigger::TwoTap
                | StylusTrigger::ThreeTap,
            ),
            SpecialAction::PointerClickRight,
        ) => AdvancedAction::PointerClick {
            keys,
            button: MouseButton::Right,
            anchor: PointerAnchor::CurrentHoverOrLastInRange,
        },
        (
            BindingId::Gesture(GestureBinding::TwoPan | GestureBinding::ThreePan),
            SpecialAction::RadialMenu,
        ) => AdvancedAction::ReservedRadialMenu,
        (
            BindingId::Gesture(GestureBinding::TwoPan | GestureBinding::ThreePan),
            SpecialAction::PointerMove,
        ) => {
            AdvancedAction::PointerDrag {
                modifiers: keys,
                button: None,
            }
        }
        (
            BindingId::Gesture(GestureBinding::TwoPan | GestureBinding::ThreePan),
            SpecialAction::PointerDragLeft,
        ) => {
            AdvancedAction::PointerDrag {
                modifiers: keys,
                button: Some(MouseButton::Left),
            }
        }
        (
            BindingId::Gesture(GestureBinding::TwoPan | GestureBinding::ThreePan),
            SpecialAction::PointerDragRight,
        ) => {
            AdvancedAction::PointerDrag {
                modifiers: keys,
                button: Some(MouseButton::Right),
            }
        }
        (BindingId::Gesture(GestureBinding::TwoPinch), SpecialAction::PointerWheel) => {
            AdvancedAction::PointerWheel { modifiers: keys }
        }
        (BindingId::Gesture(GestureBinding::TwoRotate), SpecialAction::PointerRotate) => {
            AdvancedAction::PointerRotate { modifiers: keys }
        }
        _ => {
            return Err(AppError::DesktopShell(format!(
                "special action {special_action:?} is not supported by {binding:?}"
            )));
        }
    };
    Ok(ShortcutAction::Advanced(action))
}

fn current_keys(profile: &ShortcutProfile, binding: BindingId) -> Vec<KeyCode> {
    match profile.action_for(binding) {
        ShortcutAction::HoldKeys(keys) | ShortcutAction::TriggerChord(keys) => keys,
        ShortcutAction::Advanced(AdvancedAction::PointerClick { keys, .. }) => keys,
        ShortcutAction::Advanced(AdvancedAction::PointerDrag { modifiers, .. })
        | ShortcutAction::Advanced(AdvancedAction::PointerWheel { modifiers })
        | ShortcutAction::Advanced(AdvancedAction::PointerRotate { modifiers }) => modifiers,
        _ => Vec::new(),
    }
}

fn keyboard_action_without_special(
    binding: BindingId,
    keys: Vec<KeyCode>,
) -> Result<ShortcutAction, AppError> {
    match binding {
        BindingId::StylusTrigger(StylusTrigger::FourTap) => Err(AppError::DesktopShell(
            "this binding has a fixed system action".to_string(),
        )),
        BindingId::Gesture(GestureBinding::LongPress { .. })
        | BindingId::Gesture(GestureBinding::ThreePan)
        | BindingId::Gesture(GestureBinding::TwoPinch)
        | BindingId::Gesture(GestureBinding::TwoRotate) => Ok(ShortcutAction::HoldKeys(keys)),
        BindingId::StylusTrigger(_) | BindingId::Gesture(GestureBinding::Swipe { .. }) => {
            Ok(ShortcutAction::TriggerChord(keys))
        }
    }
}
