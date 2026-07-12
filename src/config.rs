use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    error::AppError,
    shortcut::{ShortcutPresetLibrary, ShortcutProfile},
};

const CONFIG_VERSION: u32 = 7;
const APP_DIR: &str = "AirSlatePcServer";
const CONFIG_FILE: &str = "config.toml";

fn default_show_launch_at_startup_on_main_page() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PressureCurveControlPoint {
    pub x: f32,
    pub y: f32,
}

impl Default for PressureCurveControlPoint {
    fn default() -> Self {
        Self { x: 0.33, y: 0.33 }
    }
}

impl PressureCurveControlPoint {
    pub fn normalize(&mut self) {
        self.x = self.x.clamp(0.0, 1.0);
        self.y = self.y.clamp(0.0, 1.0);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PressureCurve {
    pub control_point1: PressureCurveControlPoint,
    pub control_point2: PressureCurveControlPoint,
}

impl Default for PressureCurve {
    fn default() -> Self {
        Self {
            control_point1: PressureCurveControlPoint { x: 0.33, y: 0.33 },
            control_point2: PressureCurveControlPoint { x: 0.66, y: 0.66 },
        }
    }
}

impl PressureCurve {
    pub fn normalize(&mut self) {
        self.control_point1.normalize();
        self.control_point2.normalize();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub config_version: u32,
    pub app_name: String,
    pub launch_at_startup: bool,
    #[serde(default = "default_show_launch_at_startup_on_main_page")]
    pub show_launch_at_startup_on_main_page: bool,
    pub selected_monitor_id: Option<String>,
    pub pressure_curve: PressureCurve,
    pub shortcut_profile: ShortcutProfile,
    pub shortcut_presets: ShortcutPresetLibrary,
}

impl Default for Config {
    fn default() -> Self {
        let default_profile = ShortcutProfile::default();
        let mut shortcut_presets = ShortcutPresetLibrary::default();
        shortcut_presets.normalize_with_legacy(&default_profile);

        Self {
            config_version: CONFIG_VERSION,
            app_name: "airslate_pc_server".to_string(),
            launch_at_startup: false,
            show_launch_at_startup_on_main_page: true,
            selected_monitor_id: None,
            pressure_curve: PressureCurve::default(),
            shortcut_profile: default_profile,
            shortcut_presets,
        }
    }
}

impl Config {
    pub fn load_or_create(path: &Path) -> Result<Self, AppError> {
        if path.exists() {
            return Self::load(path);
        }

        let config = Self::default();
        config.save(path)?;
        info!(path = %path.display(), "created default config");
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<Self, AppError> {
        let raw = fs::read_to_string(path)?;
        let mut config = toml::from_str::<Self>(&raw).map_err(|source| AppError::ConfigParse {
            path: path.to_path_buf(),
            source,
        })?;
        config.normalize();
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let raw = toml::to_string_pretty(self)?;
        fs::write(path, raw)?;
        Ok(())
    }

    pub fn normalize(&mut self) {
        self.config_version = CONFIG_VERSION;
        self.pressure_curve.normalize();
        self.shortcut_profile.remove_disabled_overrides();
        self.shortcut_profile.remove_fixed_binding_overrides();
        self.shortcut_presets
            .normalize_with_legacy(&self.shortcut_profile);
        for preset in &mut self.shortcut_presets.presets {
            preset.profile.remove_disabled_overrides();
            preset.profile.remove_fixed_binding_overrides();
        }
        if let Some(active) = self.shortcut_presets.active() {
            self.shortcut_profile = active.profile.clone();
        }
    }
}

pub fn config_path() -> Result<PathBuf, AppError> {
    Ok(config_base_dir()?.join(APP_DIR).join(CONFIG_FILE))
}

#[cfg(windows)]
fn config_base_dir() -> Result<PathBuf, AppError> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(AppError::MissingConfigBase("LOCALAPPDATA"))
}

#[cfg(target_os = "macos")]
fn config_base_dir() -> Result<PathBuf, AppError> {
    let home = env::var_os("HOME").ok_or(AppError::MissingConfigBase("HOME"))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcut::{BindingId, GestureBinding, KeyCode, ShortcutAction, StylusTrigger};

    #[test]
    fn config_serializes_custom_bindings_with_string_keys() {
        let mut config = Config::default();
        config.shortcut_profile.set_custom_binding(
            BindingId::StylusTrigger(StylusTrigger::TwoTap),
            ShortcutAction::TriggerChord(vec![KeyCode::Control, KeyCode::Y]),
        );
        config
            .shortcut_presets
            .active_mut()
            .expect("active preset")
            .profile = config.shortcut_profile.clone();

        let raw = toml::to_string_pretty(&config).expect("config should serialize");

        assert!(raw.contains("stylus:two_tap"));
        assert!(raw.contains("TriggerChord"));
    }

    #[test]
    fn config_round_trips_custom_bindings() {
        let mut config = Config::default();
        config.shortcut_profile.set_custom_binding(
            BindingId::Gesture(GestureBinding::LongPress { fingers: 1 }),
            ShortcutAction::HoldKeys(vec![KeyCode::Z]),
        );
        config
            .shortcut_presets
            .active_mut()
            .expect("active preset")
            .profile = config.shortcut_profile.clone();

        let raw = toml::to_string_pretty(&config).expect("config should serialize");
        let restored = toml::from_str::<Config>(&raw).expect("config should deserialize");

        assert_eq!(
            restored
                .shortcut_profile
                .custom_bindings
                .get(&BindingId::Gesture(GestureBinding::LongPress {
                    fingers: 1
                })),
            Some(&ShortcutAction::HoldKeys(vec![KeyCode::Z]))
        );
    }

    #[test]
    fn config_defaults_to_showing_launch_at_startup_on_main_page() {
        let config = Config::default();
        let raw = toml::to_string_pretty(&config).expect("config should serialize");
        let mut table = toml::from_str::<toml::Table>(&raw).expect("config should be a table");
        table.remove("show_launch_at_startup_on_main_page");
        let old_raw = toml::to_string(&table).expect("old config should serialize");
        let restored = toml::from_str::<Config>(&old_raw).expect("old config should deserialize");

        assert!(restored.show_launch_at_startup_on_main_page);
    }

    #[test]
    fn normalize_removes_four_tap_overrides_from_all_profiles() {
        let mut config = Config::default();
        let four_tap = BindingId::StylusTrigger(StylusTrigger::FourTap);
        config.shortcut_profile.set_custom_binding(
            four_tap,
            ShortcutAction::TriggerChord(vec![KeyCode::Delete]),
        );

        config.shortcut_presets.create("测试预设".to_string());
        config
            .shortcut_presets
            .active_mut()
            .expect("created preset should be active")
            .profile
            .set_custom_binding(
                four_tap,
                ShortcutAction::TriggerChord(vec![KeyCode::Delete]),
            );

        config.normalize();

        assert!(
            !config
                .shortcut_profile
                .custom_bindings
                .contains_key(&four_tap)
        );
        assert!(
            config
                .shortcut_presets
                .presets
                .iter()
                .all(|preset| !preset.profile.custom_bindings.contains_key(&four_tap))
        );
    }
}
