use std::collections::HashMap;
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
    pub combinations: Vec<Combination>,
    #[serde(default)]
    pub default_combination: String,
    #[serde(default)]
    pub knowledge_base_indexes: HashMap<String, KnowledgeBaseIndex>,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default = "default_reference_shortcut")]
    pub reference_shortcut: String,
    #[serde(default = "default_reference_capture_mode")]
    pub reference_capture_mode: String,
    #[serde(default = "default_feature_toggles")]
    pub feature_toggles: HashMap<String, bool>,
    #[serde(default = "default_shortcuts")]
    pub shortcuts: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Combination {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub knowledge_base_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeBaseIndex {
    pub mode: String,
    #[serde(default)]
    pub manual_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureAndShortcutSettings {
    #[serde(default = "default_feature_toggles")]
    pub feature_toggles: HashMap<String, bool>,
    #[serde(default = "default_shortcuts")]
    pub shortcuts: HashMap<String, String>,
    #[serde(default = "default_reference_shortcut")]
    pub reference_shortcut: String,
    #[serde(default = "default_reference_capture_mode")]
    pub reference_capture_mode: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureAndShortcutSaveResult {
    pub success: bool,
    pub field_errors: HashMap<String, String>,
    pub settings: FeatureAndShortcutSettings,
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
    #[serde(default = "default_reference_shortcut")]
    pub reference_shortcut: String,
    #[serde(default = "default_reference_capture_mode")]
    pub reference_capture_mode: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombinationInput {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub knowledge_base_ids: Vec<String>,
}

fn default_reference_shortcut() -> String {
    "Ctrl+Shift+T".to_string()
}

fn default_reference_capture_mode() -> String {
    "selection".to_string()
}

pub fn default_feature_toggles() -> HashMap<String, bool> {
    HashMap::from([
        ("network-search".to_string(), true),
        ("reference-context".to_string(), true),
    ])
}

pub fn default_shortcuts() -> HashMap<String, String> {
    HashMap::from([
        ("read-selection".to_string(), "Ctrl+Alt+E".to_string()),
        (
            "open-control-panel".to_string(),
            "Ctrl+Shift+Alt+S".to_string(),
        ),
        ("quit-app".to_string(), "Ctrl+Alt+Q".to_string()),
    ])
}

pub fn feature_enabled(settings: &Settings, key: &str) -> bool {
    settings
        .feature_toggles
        .get(key)
        .copied()
        .unwrap_or_else(|| default_feature_toggles().get(key).copied().unwrap_or(false))
}

pub fn shortcut_for(settings: &Settings, key: &str) -> String {
    settings
        .shortcuts
        .get(key)
        .cloned()
        .or_else(|| default_shortcuts().get(key).cloned())
        .unwrap_or_default()
}

pub fn validate_shortcut(value: &str) -> Result<(), String> {
    let parts = value.split('+').map(str::trim).collect::<Vec<_>>();
    if parts.len() < 2 || parts.iter().any(|part| part.is_empty()) {
        return Err("快捷键必须包含修饰键和一个字母或数字。".to_string());
    }
    let (key, modifiers) = parts.split_last().expect("non-empty shortcut parts");
    if key.chars().count() != 1
        || !key
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err("快捷键的主键只能是一个字母或数字。".to_string());
    }
    let mut seen = HashMap::new();
    for modifier in modifiers {
        if !matches!(*modifier, "Ctrl" | "Alt" | "Shift") || seen.insert(*modifier, true).is_some()
        {
            return Err("修饰键只能使用一次 Ctrl、Alt 或 Shift。".to_string());
        }
    }
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_status: Option<String>,
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
            combinations: Vec::new(),
            default_combination: String::new(),
            knowledge_base_indexes: HashMap::new(),
            allow_network: false,
            reference_shortcut: default_reference_shortcut(),
            reference_capture_mode: default_reference_capture_mode(),
            feature_toggles: default_feature_toggles(),
            shortcuts: default_shortcuts(),
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
            let settings = Settings {
                agents_root: managed_library_root("agents")?
                    .to_string_lossy()
                    .into_owned(),
                knowledge_bases_root: managed_library_root("knowledge-bases")?
                    .to_string_lossy()
                    .into_owned(),
                ..Settings::default()
            };
            return Ok(settings);
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
    pub fn payload(settings: Settings) -> SettingsPayload {
        SettingsPayload {
            agents: discover_catalog(&settings.agents_root, "AGENT.md"),
            knowledge_bases: discover_knowledge_bases(&settings),
            api_key_configured: WindowsCredentialStore::configured(),
            settings,
        }
    }
}

pub fn managed_library_root(kind: &str) -> Result<PathBuf, String> {
    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Unable to locate the Windows AppData directory.".to_string())?;
    Ok(app_data
        .join("codex-input-enhancer")
        .join("library")
        .join(kind))
}

fn index_storage_path(id: &str) -> Result<PathBuf, String> {
    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Unable to locate the Windows AppData directory.".to_string())?;
    Ok(app_data
        .join("codex-input-enhancer")
        .join("indexes")
        .join(format!("{id}.md")))
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
                    index_status: None,
                }
            })
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

pub fn discover_knowledge_bases(settings: &Settings) -> Vec<CatalogEntry> {
    let Ok(entries) = fs::read_dir(&settings.knowledge_bases_root) else {
        return Vec::new();
    };
    let mut result = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            (path.is_dir()).then(|| {
                let id = entry.file_name().to_string_lossy().into_owned();
                CatalogEntry {
                    name: id.clone(),
                    path: path.to_string_lossy().into_owned(),
                    index_status: Some(knowledge_base_index_status(settings, &id)),
                    id,
                }
            })
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

#[allow(clippy::obfuscated_if_else)]
pub fn knowledge_base_index_status(settings: &Settings, id: &str) -> String {
    match settings.knowledge_base_indexes.get(id) {
        Some(index) if index.mode == "generated" => index_storage_path(id)
            .map(|path| path.is_file())
            .unwrap_or(false)
            .then_some("已由 AI 生成")
            .unwrap_or("缺少 INDEX")
            .to_string(),
        Some(index) if index.mode == "manual" => {
            knowledge_base_file(settings, id, &index.manual_path)
                .is_ok()
                .then_some("已手动指定")
                .unwrap_or("缺少 INDEX")
                .to_string()
        }
        _ => knowledge_base_file(settings, id, "INDEX.md")
            .is_ok()
            .then_some("使用 INDEX.md")
            .unwrap_or("缺少 INDEX")
            .to_string(),
    }
}

pub fn read_knowledge_base_index(settings: &Settings, id: &str) -> Result<String, String> {
    let configured = settings.knowledge_base_indexes.get(id);
    let path = match configured {
        Some(index) if index.mode == "generated" => index_storage_path(id)?,
        Some(index) if index.mode == "manual" => {
            knowledge_base_file(settings, id, &index.manual_path)?
        }
        _ => knowledge_base_file(settings, id, "INDEX.md")?,
    };
    read_bounded_text(&path, "knowledge base index")
}

pub fn knowledge_base_index_candidates(
    settings: &Settings,
    id: &str,
) -> Result<Vec<String>, String> {
    let root = knowledge_base_file(settings, id, ".")?;
    let mut candidates = Vec::new();
    collect_text_paths(&root, &root, &mut candidates)?;
    Ok(candidates)
}

pub fn knowledge_base_index_material(settings: &Settings, id: &str) -> Result<String, String> {
    let root = knowledge_base_file(settings, id, ".")?;
    let mut material = String::new();
    for relative in knowledge_base_index_candidates(settings, id)?
        .into_iter()
        .take(18)
    {
        let content = fs::read_to_string(root.join(&relative)).unwrap_or_default();
        material.push_str(&format!(
            "\n\n## {relative}\n{}",
            content.chars().take(1400).collect::<String>()
        ));
    }
    Ok(material)
}

pub fn save_generated_knowledge_base_index(
    settings: &mut Settings,
    id: String,
    content: String,
) -> Result<(), String> {
    let path = index_storage_path(&id)?;
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| "无效的索引目录。".to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())?;
    settings.knowledge_base_indexes.insert(
        id,
        KnowledgeBaseIndex {
            mode: "generated".to_string(),
            manual_path: String::new(),
        },
    );
    Ok(())
}

fn knowledge_base_file(settings: &Settings, id: &str, relative: &str) -> Result<PathBuf, String> {
    let root = Path::new(&settings.knowledge_bases_root)
        .canonicalize()
        .map_err(|_| "知识库目录不可用。".to_string())?;
    if Path::new(id).file_name().and_then(|part| part.to_str()) != Some(id) {
        return Err("无效的知识库。".to_string());
    }
    let base = root
        .join(id)
        .canonicalize()
        .map_err(|_| "知识库不存在。".to_string())?;
    let path = if relative == "." {
        base.clone()
    } else {
        base.join(relative)
            .canonicalize()
            .map_err(|_| "索引文件不存在。".to_string())?
    };
    if !path.starts_with(&base) {
        return Err("索引文件必须位于知识库内。".to_string());
    }
    Ok(path)
}

fn collect_text_paths(root: &Path, current: &Path, output: &mut Vec<String>) -> Result<(), String> {
    for entry in fs::read_dir(current).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if fs::symlink_metadata(&path)
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
        {
            continue;
        }
        if path.is_dir() {
            collect_text_paths(root, &path, output)?;
        } else if workspace::is_readable_text(&path) {
            output.push(
                path.strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    output.sort();
    output.truncate(120);
    Ok(())
}

fn read_bounded_text(path: &Path, kind: &str) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|_| format!("{kind} 不存在。"))?;
    if metadata.len() > 32_000 {
        return Err(format!("{kind} 超过 32 KB 限制。"));
    }
    fs::read_to_string(path).map_err(|error| error.to_string())
}

#[allow(dead_code)]
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
    pub fn delete_directory(root: &str, id: &str, kind: &str) -> Result<(), String> {
        let id = checked_catalog_id(id, kind)?;
        let root = Path::new(root)
            .canonicalize()
            .map_err(|_| format!("{kind} 资料库不可用。"))?;
        let directory = root
            .join(id)
            .canonicalize()
            .map_err(|_| format!("{kind} 不存在。"))?;
        if !directory.starts_with(&root)
            || directory.parent() != Some(root.as_path())
            || !directory.is_dir()
        {
            return Err(format!("无效的 {kind}。"));
        }
        fs::remove_dir_all(directory).map_err(|error| error.to_string())
    }
    pub fn import_agent_file(root: &str, source: &str) -> Result<(), String> {
        let root = Path::new(root);
        fs::create_dir_all(root).map_err(|error| error.to_string())?;
        let source = Path::new(source)
            .canonicalize()
            .map_err(|_| "无法访问 Agent 文件。".to_string())?;
        if !source.is_file() || source.extension().and_then(|part| part.to_str()) != Some("md") {
            return Err("Agent 必须是 UTF-8 Markdown 文件。".to_string());
        }
        let metadata = fs::metadata(&source).map_err(|error| error.to_string())?;
        if metadata.len() > 32_000 {
            return Err("Agent 文件不能超过 32 KB。".to_string());
        }
        fs::read_to_string(&source).map_err(|_| "Agent 必须是 UTF-8 文本。".to_string())?;
        let name = source
            .file_stem()
            .and_then(|part| part.to_str())
            .ok_or_else(|| "无效的 Agent 文件名。".to_string())?;
        let destination = unique_catalog_directory(root, name)?;
        fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        fs::copy(&source, destination.join("AGENT.md")).map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn import_knowledge_bases(root: &str, sources: &[String]) -> Result<(), String> {
        let root = Path::new(root);
        fs::create_dir_all(root).map_err(|error| error.to_string())?;
        for source in sources {
            let source = Path::new(source)
                .canonicalize()
                .map_err(|_| "无法访问知识库文件夹。".to_string())?;
            if !source.is_dir() {
                return Err("知识库必须是文件夹。".to_string());
            }
            let name = source
                .file_name()
                .and_then(|part| part.to_str())
                .ok_or_else(|| "无效的知识库文件夹。".to_string())?;
            let destination = unique_catalog_directory(root, name)?;
            let mut copied = 0;
            copy_text_directory(&source, &destination, &mut copied).inspect_err(|_| {
                let _ = fs::remove_dir_all(&destination);
            })?;
            if copied == 0 {
                let _ = fs::remove_dir_all(&destination);
                return Err("知识库中没有可读取的 .md 或 .txt 文件。".to_string());
            }
        }
        Ok(())
    }
    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn delete(root: &str, id: &str, marker: &str, kind: &str) -> Result<(), String> {
        let directory = catalog_directory(root, id, marker, kind)?;
        fs::remove_dir_all(directory).map_err(|error| format!("Unable to delete {kind}: {error}"))
    }
}

fn unique_catalog_directory(root: &Path, base: &str) -> Result<PathBuf, String> {
    let base = checked_catalog_id(base, "library item")?;
    for number in 1..=999 {
        let suffix = if number == 1 {
            String::new()
        } else {
            format!("-{number}")
        };
        let path = root.join(format!("{base}{suffix}"));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err("同名资料过多，无法导入。".to_string())
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
