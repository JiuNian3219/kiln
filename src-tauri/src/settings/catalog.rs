use std::fs;
use std::path::{Path, PathBuf};

use super::knowledge_base::knowledge_base_index_status;
use super::types::{CatalogEntry, Settings};
use crate::workspace;

pub fn managed_library_root(kind: &str) -> Result<PathBuf, String> {
    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Unable to locate the Windows AppData directory.".to_string())?;
    Ok(app_data
        .join("codex-input-enhancer")
        .join("library")
        .join(kind))
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
            path.is_dir().then(|| {
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

#[allow(dead_code)]
pub fn catalog_directory(
    root: &str,
    id: &str,
    marker: &str,
    kind: &str,
) -> Result<PathBuf, String> {
    let id = checked_catalog_id(id, kind)?;
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
        if fs::metadata(&source)
            .map_err(|error| error.to_string())?
            .len()
            > 32_000
        {
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
