use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnownDeviceConfig {
    pub name: String,
    pub sku: Option<String>,
    pub model_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub last_connected_device_id: Option<String>,
    pub last_connected_mac_address: Option<String>,
    #[serde(default)]
    pub known_devices: HashMap<String, KnownDeviceConfig>,
}

impl AppConfig {
    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    pub fn load() -> Result<Self, String> {
        let path = Self::path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let data = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read config {}: {}", path.display(), error))?;

        serde_json::from_str(&data)
            .map_err(|error| format!("failed to parse config {}: {}", path.display(), error))
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path()?;
        let parent = path
            .parent()
            .ok_or_else(|| format!("config path has no parent: {}", path.display()))?;

        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create config dir {}: {}",
                parent.display(),
                error
            )
        })?;

        let data = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize config: {}", error))?;

        fs::write(&path, data)
            .map_err(|error| format!("failed to write config {}: {}", path.display(), error))
    }

    pub fn remember_device_name(&mut self, id: &str, name: &str) -> bool {
        let entry = self.known_devices.entry(id.to_string()).or_default();
        if entry.name == name {
            return false;
        }

        entry.name = name.to_string();
        true
    }

    pub fn remember_connected_device(&mut self, id: &str) -> bool {
        let changed = self.last_connected_device_id.as_deref() != Some(id)
            || self.last_connected_mac_address.as_deref() != Some(id);

        self.last_connected_device_id = Some(id.to_string());
        self.last_connected_mac_address = Some(id.to_string());
        changed
    }

    pub fn remember_device_metadata(
        &mut self,
        id: &str,
        name: Option<&str>,
        model_key: Option<&str>,
        sku: Option<&str>,
    ) -> bool {
        let entry = self.known_devices.entry(id.to_string()).or_default();
        let before = (
            entry.name.clone(),
            entry.model_key.clone(),
            entry.sku.clone(),
        );

        if let Some(name) = name {
            entry.name = name.to_string();
        }
        if let Some(model_key) = model_key {
            entry.model_key = Some(model_key.to_string());
        }
        if let Some(sku) = sku {
            entry.sku = Some(sku.to_string());
        }

        before
            != (
                entry.name.clone(),
                entry.model_key.clone(),
                entry.sku.clone(),
            )
    }

    pub fn known_model_key(&self, id: &str) -> Option<&str> {
        self.known_devices
            .get(id)
            .and_then(|device| device.model_key.as_deref())
    }

    fn path() -> Result<PathBuf, String> {
        let project_dirs = ProjectDirs::from("it", "Purpl3", "ear-native")
            .ok_or_else(|| "failed to resolve config directory".to_string())?;

        Ok(project_dirs.config_dir().join("config.json"))
    }
}
