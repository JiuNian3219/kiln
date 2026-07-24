use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::credential::WindowsCredentialStore;
use crate::workspace;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub model: String,
    pub agents_root: String,
    pub knowledge_bases_root: String,
    pub default_agent: String,
    pub default_knowledge_base: String,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default = "default_reference_shortcut")]
    pub reference_shortcut: String,
    #[serde(default = "default_reference_capture_mode")]
    pub reference_capture_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub model: String,
    pub agents_root: String,
    pub knowledge_bases_root: String,
    pub default_agent: String,
    pub default_knowledge_base: String,
    pub api_key: String,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default = "default_reference_shortcut")]
    pub reference_shortcut: String,
    #[serde(default = "default_reference_capture_mode")]
    pub reference_capture_mode: String,
}

fn default_reference_shortcut() -> String {
    "Ctrl+Shift+T".to_string()
}

fn default_reference_capture_mode() -> String {
    "selection".to_string()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPayload {
    pub settings: Settings,
    pub api_key_configured: bool,
    pub agents: Vec<CatalogEntry>,
    pub knowledge_bases: Vec<CatalogEntry>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: "deepseek-v4-flash".into(),
            agents_root: String::new(),
            knowledge_bases_root: String::new(),
            default_agent: String::new(),
            default_knowledge_base: String::new(),
            allow_network: false,
            reference_shortcut: default_reference_shortcut(),
            reference_capture_mode: default_reference_capture_mode(),
        }
    }
}

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
            return Ok(Settings::default());
        }
        serde_json::from_str(&fs::read_to_string(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())
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
    pub fn payload(settings: Settings) -> SettingsPayload {
        SettingsPayload {
            agents: discover_catalog(&settings.agents_root, "AGENT.md"),
            knowledge_bases: discover_catalog(&settings.knowledge_bases_root, "INDEX.md"),
            api_key_configured: WindowsCredentialStore::configured(),
            settings,
        }
    }
}

pub fn discover_catalog(root: &str, marker: &str) -> Vec<CatalogEntry> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut result = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            (path.is_dir() && path.join(marker).is_file()).then(|| {
                let name = entry.file_name().to_string_lossy().into_owned();
                CatalogEntry {
                    id: name.clone(),
                    name,
                    path: path.to_string_lossy().into_owned(),
                }
            })
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

pub fn catalog_directory(
    root: &str,
    id: &str,
    marker: &str,
    kind: &str,
) -> Result<PathBuf, String> {
    let id = id.trim();
    if id.is_empty() || Path::new(id).file_name().and_then(|part| part.to_str()) != Some(id) {
        return Err(format!("Invalid {kind} name."));
    }
    let root = Path::new(root)
        .canonicalize()
        .map_err(|_| format!("Configure a valid {kind} root first."))?;
    let directory = root
        .join(id)
        .canonicalize()
        .map_err(|_| format!("The selected {kind} no longer exists."))?;
    if !directory.is_dir()
        || directory.parent() != Some(root.as_path())
        || !directory.join(marker).is_file()
    {
        return Err(format!("Invalid {kind} directory."));
    }
    Ok(directory)
}

/// Manages imported catalogs. Imported content is copied into the configured
/// root after rejecting symlinks and non-text files, so later model reads stay
/// within the user's explicitly configured directories.
pub struct CatalogRepository;

impl CatalogRepository {
    pub fn import(root: &str, source: &str, marker: &str, kind: &str) -> Result<(), String> {
        let root = Path::new(root)
            .canonicalize()
            .map_err(|_| format!("Configure a valid {kind} root first."))?;
        let source = Path::new(source)
            .canonicalize()
            .map_err(|_| format!("Unable to access the selected {kind} folder."))?;
        if !source.is_dir() || !source.join(marker).is_file() {
            return Err(format!("The selected folder must contain {marker}."));
        }
        if source.starts_with(&root) {
            return Err("This folder is already inside the configured root.".to_string());
        }
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "Invalid folder name.".to_string())?;
        let destination = root.join(checked_catalog_id(name, kind)?);
        if destination.exists() {
            return Err(format!("A {kind} with this name already exists."));
        }
        let mut copied = 0;
        copy_text_directory(&source, &destination, &mut copied).inspect_err(|_| {
            let _ = fs::remove_dir_all(&destination);
        })?;
        if !destination.join(marker).is_file() {
            let _ = fs::remove_dir_all(&destination);
            return Err(format!("The imported {kind} did not include {marker}."));
        }
        Ok(())
    }

    pub fn delete(root: &str, id: &str, marker: &str, kind: &str) -> Result<(), String> {
        let directory = catalog_directory(root, id, marker, kind)?;
        fs::remove_dir_all(directory).map_err(|error| format!("Unable to delete {kind}: {error}"))
    }
}

fn checked_catalog_id(id: &str, kind: &str) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty() || Path::new(id).file_name().and_then(|name| name.to_str()) != Some(id) {
        return Err(format!("Invalid {kind} name."));
    }
    Ok(id.to_string())
}

fn copy_text_directory(
    source: &Path,
    destination: &Path,
    copied: &mut usize,
) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Imported folders cannot contain symbolic links.".to_string());
        }
        if metadata.is_dir() {
            copy_text_directory(&source_path, &destination_path, copied)?;
        } else if workspace::is_readable_text(&source_path) {
            *copied += 1;
            if *copied > 120 {
                return Err("An imported folder may contain at most 120 text files.".to_string());
            }
            if metadata.len() > 32_000 {
                return Err("Each imported text file must be at most 32 KB.".to_string());
            }
            fs::copy(&source_path, &destination_path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
