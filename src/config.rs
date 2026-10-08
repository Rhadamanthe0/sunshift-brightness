//! SSB Config file definition
use crate::common::config_directory;
use anyhow::Context;
use enum_iterator::Sequence;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use validator::{Validate, ValidationError};

const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Deserialize, Serialize, Validate, Clone, Copy, PartialEq)]
pub struct Location {
    #[validate(range(min = -90.0, max = 90.0), custom(function = "validate_finite"))]
    pub latitude: f64,
    #[validate(range(min = -180.0, max = 180.0), custom(function = "validate_finite"))]
    pub longitude: f64,
}

fn validate_finite(value: f64) -> Result<(), ValidationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ValidationError::new("finite"))
    }
}

#[derive(Debug, Deserialize, Serialize, Validate, Clone)]
pub struct SsbConfig {
    #[validate(range(max = 100))]
    pub brightness_day: u32,
    #[validate(range(max = 100))]
    pub brightness_night: u32,
    #[validate(range(max = 360))]
    pub transition_mins: u32,
    /// Minutes pour avancer (positif) ou retarder (négatif) la transition du
    /// matin par rapport au lever du soleil réel.
    #[serde(default)]
    #[validate(range(min = -180, max = 180))]
    pub sunrise_offset_mins: i32,
    /// Minutes pour avancer (positif) ou retarder (négatif) la transition du
    /// soir par rapport au coucher du soleil réel.
    #[serde(default)]
    #[validate(range(min = -180, max = 180))]
    pub sunset_offset_mins: i32,
    #[validate(nested)]
    pub location: Option<Location>,
    #[serde(default)]
    #[validate(nested)]
    pub overrides: Vec<MonitorOverride>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, Hash, PartialEq, Sequence)]
#[serde(rename_all = "snake_case")]
pub enum MonitorProperty {
    DeviceName,
    #[cfg(windows)]
    DeviceDescription,
    #[cfg(windows)]
    DeviceKey,
    #[cfg(windows)]
    DevicePath,
}

impl MonitorProperty {
    pub fn as_str(&self) -> &'static str {
        match self {
            MonitorProperty::DeviceName => "Name",
            #[cfg(windows)]
            MonitorProperty::DeviceDescription => "Description",
            #[cfg(windows)]
            MonitorProperty::DeviceKey => "Key",
            #[cfg(windows)]
            MonitorProperty::DevicePath => "Path",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Validate, Clone)]
pub struct MonitorOverride {
    pub pattern: String,
    pub key: MonitorProperty,
    #[validate(nested)]
    pub brightness: Option<BrightnessValues>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Copy, Clone)]
pub struct BrightnessValues {
    #[validate(range(max = 100))]
    pub brightness_day: u32,
    #[validate(range(max = 100))]
    pub brightness_night: u32,
}

impl SsbConfig {
    pub fn load(path_override: Option<PathBuf>) -> anyhow::Result<Option<Self>> {
        let path = path_override.unwrap_or_else(get_default_config_path);
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).context(format!("Unable to read file '{}'", path.display())),
        };
        let config = serde_json::from_str::<SsbConfig>(&contents).context(format!(
            "Unable to deserialize config file '{}'",
            path.display()
        ))?;
        config
            .validate()
            .context(format!("Invalid config file '{}'", path.display()))?;
        Ok(Some(config))
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.validate().context("Invalid configuration")?;
        self.persist_to(&get_default_config_path())
    }

    fn persist_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let serialised = serde_json::to_string_pretty(&self)?;
        let parent = path.parent().expect("config path must have parent");
        fs::create_dir_all(parent).context("Unable to create configuration directory")?;
        let mut temp_file = NamedTempFile::new_in(parent)?;
        temp_file.write_all(serialised.as_bytes())?;
        temp_file.flush()?;
        temp_file.as_file().sync_all()?;
        temp_file.persist(path)?;
        log::debug!("Successfully saved config to {}", path.display());
        Ok(())
    }
}

pub fn get_default_config_path() -> PathBuf {
    config_directory().join(CONFIG_FILE_NAME)
}

impl Default for SsbConfig {
    fn default() -> Self {
        SsbConfig {
            brightness_day: 100,
            brightness_night: 60,
            transition_mins: 40,
            sunrise_offset_mins: 0,
            sunset_offset_mins: 0,
            location: None,
            overrides: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_directory_creation_is_fallible() {
        let dir = tempfile::tempdir().unwrap();
        let config = SsbConfig::default();
        let missing_parent = dir.path().join("new/config.json");
        config.persist_to(&missing_parent).unwrap();
        assert!(SsbConfig::load(Some(missing_parent)).unwrap().is_some());
        let blocked = dir.path().join("blocked");
        fs::write(&blocked, "keep").unwrap();
        assert!(config.persist_to(&blocked.join("config.json")).is_err());
        assert_eq!(fs::read_to_string(blocked).unwrap(), "keep");
    }

    #[test]
    fn saves_and_replaces_configuration_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut config = SsbConfig::default();
        config.persist_to(&path).unwrap();
        config.brightness_night = 0;
        config.sunrise_offset_mins = -180;
        config.persist_to(&path).unwrap();
        let loaded = SsbConfig::load(Some(path)).unwrap().unwrap();
        assert_eq!(loaded.brightness_night, 0);
        assert_eq!(loaded.sunrise_offset_mins, -180);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn old_configuration_loads_with_zero_offsets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"brightness_day":100,"brightness_night":0,"transition_mins":0,"location":{"latitude":48,"longitude":2}}"#).unwrap();
        let config = SsbConfig::load(Some(path)).unwrap().unwrap();
        assert_eq!(config.sunrise_offset_mins, 0);
        assert_eq!(config.sunset_offset_mins, 0);
        assert!(config.overrides.is_empty());
    }

    #[test]
    fn rejects_non_finite_locations_and_invalid_nested_values() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 91.0] {
            assert!(Location {
                latitude: value,
                longitude: 0.0
            }
            .validate()
            .is_err());
        }
        let config = SsbConfig {
            overrides: vec![MonitorOverride {
                pattern: "*".into(),
                key: MonitorProperty::DeviceName,
                brightness: Some(BrightnessValues {
                    brightness_day: 101,
                    brightness_night: 0,
                }),
            }],
            ..Default::default()
        };
        assert!(config.validate().is_err());
        assert!(config.save().is_err());
    }
}
