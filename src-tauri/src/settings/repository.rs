use std::fs;
use std::path::PathBuf;

use super::behavior::{
    active_model_provider, deepseek_provider, default_feature_toggles, default_shortcuts,
    feature_enabled,
};
use super::catalog::managed_library_root;
use super::types::{CatalogEntry, Settings, SettingsPayload};
use crate::credential::WindowsCredentialStore;

pub struct SettingsRepository;

impl SettingsRepository {
    fn path() -> Result<PathBuf, String> {
        let app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "Unable to locate the Windows AppData directory.".to_string())?;
        Ok(app_data.join("codex-input-enhancer").join("settings.json"))
    }

    pub fn load() -> Result<Settings, String> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Settings {
                agents_root: managed_library_root("agents")?
                    .to_string_lossy()
                    .into_owned(),
                knowledge_bases_root: managed_library_root("knowledge-bases")?
                    .to_string_lossy()
                    .into_owned(),
                ..Settings::default()
            });
        }
        let mut settings: Settings =
            serde_json::from_str(&fs::read_to_string(path).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        for (key, value) in default_feature_toggles() {
            settings.feature_toggles.entry(key).or_insert(value);
        }
        for (key, value) in default_shortcuts() {
            settings.shortcuts.entry(key).or_insert(value);
        }
        if settings.model_providers.is_empty() {
            settings
                .model_providers
                .push(deepseek_provider(settings.model.clone()));
        }
        if !settings
            .model_providers
            .iter()
            .any(|provider| provider.id == settings.default_model_provider)
        {
            settings.default_model_provider = settings
                .model_providers
                .first()
                .map(|provider| provider.id.clone())
                .unwrap_or_default();
        }
        settings.allow_network = feature_enabled(&settings, "network-search");
        if settings.agents_root.trim().is_empty() {
            settings.agents_root = managed_library_root("agents")?
                .to_string_lossy()
                .into_owned();
        }
        if settings.knowledge_bases_root.trim().is_empty() {
            settings.knowledge_bases_root = managed_library_root("knowledge-bases")?
                .to_string_lossy()
                .into_owned();
        }
        Ok(settings)
    }

    pub fn save(settings: &Settings) -> Result<(), String> {
        let path = Self::path()?;
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| "Invalid settings path.".to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            path,
            serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }

    pub fn payload_from_catalog(
        settings: Settings,
        agents: Vec<CatalogEntry>,
        knowledge_bases: Vec<CatalogEntry>,
    ) -> SettingsPayload {
        SettingsPayload {
            agents,
            knowledge_bases,
            api_key_configured: active_model_provider(&settings)
                .ok()
                .map(|provider| WindowsCredentialStore::configured_for(&provider.id))
                .unwrap_or(false),
            settings,
        }
    }
}
