use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};
use uuid::Uuid;

use super::{
    domain::{
        BindingId, GestureBinding, RadialMenuConfig, ShortcutAction, StylusTrigger, SwipeAxis,
    },
    preset::DefaultPreset,
};

pub type SharedShortcutProfile = Arc<RwLock<ShortcutProfile>>;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct ShortcutProfile {
    #[serde(default, with = "binding_map_serde")]
    pub custom_bindings: HashMap<BindingId, ShortcutAction>,
    pub radial_menu: RadialMenuConfig,
}

mod binding_map_serde {
    use super::*;

    pub fn serialize<S>(
        bindings: &HashMap<BindingId, ShortcutAction>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries = bindings
            .iter()
            .map(|(binding, action)| (binding.persisted_key(), action))
            .collect::<BTreeMap<_, _>>();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<BindingId, ShortcutAction>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries = BTreeMap::<String, ShortcutAction>::deserialize(deserializer)?;
        entries
            .into_iter()
            .map(|(binding, action)| {
                BindingId::parse_persisted_key(&binding)
                    .map(|binding| (binding, action))
                    .ok_or_else(|| D::Error::custom(format!("unknown binding id: {binding}")))
            })
            .collect()
    }
}

impl ShortcutProfile {
    pub fn shared(self) -> SharedShortcutProfile {
        Arc::new(RwLock::new(self))
    }

    pub fn preset_action_for(&self, binding: BindingId) -> ShortcutAction {
        DefaultPreset.action_for(binding)
    }

    pub fn action_for(&self, binding: BindingId) -> ShortcutAction {
        self.custom_bindings
            .get(&binding)
            .cloned()
            .unwrap_or_else(|| self.preset_action_for(binding))
    }

    pub fn reset_to_default(&mut self) {
        *self = Self::default();
    }

    pub fn set_custom_binding(&mut self, binding: BindingId, action: ShortcutAction) {
        self.custom_bindings.insert(binding, action);
    }

    pub fn remove_disabled_overrides(&mut self) {
        self.custom_bindings
            .retain(|_, action| !matches!(action, ShortcutAction::Disabled));
    }

    pub fn remove_fixed_binding_overrides(&mut self) {
        self.custom_bindings
            .remove(&BindingId::StylusTrigger(StylusTrigger::FourTap));
    }

    pub fn radial_menu(&self) -> &RadialMenuConfig {
        &self.radial_menu
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ShortcutPreset {
    pub id: String,
    pub name: String,
    pub profile: ShortcutProfile,
}

impl Default for ShortcutPreset {
    fn default() -> Self {
        Self::default_preset()
    }
}

impl ShortcutPreset {
    pub fn default_preset() -> Self {
        Self {
            id: "default".to_string(),
            name: "默认预设".to_string(),
            profile: ShortcutProfile::default(),
        }
    }

    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            profile: ShortcutProfile::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ShortcutPresetLibrary {
    pub active_preset_id: String,
    pub presets: Vec<ShortcutPreset>,
}

impl Default for ShortcutPresetLibrary {
    fn default() -> Self {
        let preset = ShortcutPreset::default_preset();
        Self {
            active_preset_id: preset.id.clone(),
            presets: vec![preset],
        }
    }
}

impl ShortcutPresetLibrary {
    pub fn normalize_with_legacy(&mut self, legacy_profile: &ShortcutProfile) {
        if self.presets.is_empty() {
            self.presets.push(ShortcutPreset {
                id: "default".to_string(),
                name: "默认预设".to_string(),
                profile: legacy_profile.clone(),
            });
        }

        if self.active_preset_id.is_empty()
            || !self
                .presets
                .iter()
                .any(|preset| preset.id == self.active_preset_id)
        {
            self.active_preset_id = self
                .presets
                .first()
                .map(|preset| preset.id.clone())
                .unwrap_or_else(|| "default".to_string());
        }
    }

    pub fn active(&self) -> Option<&ShortcutPreset> {
        self.presets
            .iter()
            .find(|preset| preset.id == self.active_preset_id)
    }

    pub fn active_mut(&mut self) -> Option<&mut ShortcutPreset> {
        self.presets
            .iter_mut()
            .find(|preset| preset.id == self.active_preset_id)
    }

    pub fn select(&mut self, preset_id: &str) -> bool {
        if self.presets.iter().any(|preset| preset.id == preset_id) {
            self.active_preset_id = preset_id.to_string();
            true
        } else {
            false
        }
    }

    pub fn create(&mut self, name: String) -> &ShortcutPreset {
        let preset = ShortcutPreset::new(name);
        let preset_id = preset.id.clone();
        self.presets.push(preset);
        self.active_preset_id = preset_id;
        self.active().expect("created preset should be active")
    }

    pub fn rename(&mut self, preset_id: &str, name: String) -> bool {
        let Some(preset) = self
            .presets
            .iter_mut()
            .find(|preset| preset.id == preset_id)
        else {
            return false;
        };
        preset.name = name;
        true
    }

    pub fn reset(&mut self, preset_id: &str) -> bool {
        let Some(preset) = self
            .presets
            .iter_mut()
            .find(|preset| preset.id == preset_id)
        else {
            return false;
        };
        preset.profile.reset_to_default();
        true
    }

    pub fn remove(&mut self, preset_id: &str) -> bool {
        if preset_id == "default" {
            return false;
        }

        let Some(index) = self
            .presets
            .iter()
            .position(|preset| preset.id == preset_id)
        else {
            return false;
        };

        self.presets.remove(index);
        if self.active_preset_id == preset_id {
            self.active_preset_id = "default".to_string();
        }
        true
    }
}

pub fn all_bindings() -> Vec<BindingId> {
    vec![
        BindingId::Gesture(GestureBinding::TwoPan),
        BindingId::StylusTrigger(StylusTrigger::Squeeze),
        BindingId::StylusTrigger(StylusTrigger::DoubleTap),
        BindingId::StylusTrigger(StylusTrigger::TwoTap),
        BindingId::StylusTrigger(StylusTrigger::ThreeTap),
        BindingId::StylusTrigger(StylusTrigger::FourTap),
        BindingId::Gesture(GestureBinding::ThreePan),
        BindingId::Gesture(GestureBinding::TwoPinch),
        BindingId::Gesture(GestureBinding::TwoRotate),
        BindingId::Gesture(GestureBinding::LongPress { fingers: 1 }),
        BindingId::Gesture(GestureBinding::LongPress { fingers: 2 }),
        BindingId::Gesture(GestureBinding::LongPress { fingers: 3 }),
        BindingId::Gesture(GestureBinding::LongPress { fingers: 4 }),
        BindingId::Gesture(GestureBinding::Swipe {
            fingers: 1,
            axis: SwipeAxis::Horizontal,
        }),
        BindingId::Gesture(GestureBinding::Swipe {
            fingers: 1,
            axis: SwipeAxis::Vertical,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcut::domain::{AdvancedAction, PointerAnchor};

    #[test]
    fn profile_defaults_to_preset_mapping() {
        let profile = ShortcutProfile::default();
        assert_eq!(
            profile.action_for(BindingId::StylusTrigger(StylusTrigger::DoubleTap)),
            ShortcutAction::Advanced(AdvancedAction::SecondaryClick {
                anchor: PointerAnchor::CurrentHoverOrLastInRange,
            })
        );
    }

    #[test]
    fn preset_library_defaults_to_single_default_preset() {
        let library = ShortcutPresetLibrary::default();
        assert_eq!(library.presets.len(), 1);
        assert_eq!(library.active_preset_id, "default");
        assert_eq!(library.presets[0].name, "默认预设");
    }

    #[test]
    fn removing_active_custom_preset_falls_back_to_default() {
        let mut library = ShortcutPresetLibrary::default();
        let preset = library.create("测试预设".to_string()).clone();

        assert!(library.remove(&preset.id));
        assert_eq!(library.active_preset_id, "default");
        assert_eq!(library.presets.len(), 1);
    }

    #[test]
    fn default_preset_cannot_be_removed() {
        let mut library = ShortcutPresetLibrary::default();

        assert!(!library.remove("default"));
        assert_eq!(library.presets.len(), 1);
        assert_eq!(library.active_preset_id, "default");
    }
}
