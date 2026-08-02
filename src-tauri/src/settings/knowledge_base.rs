use std::fs;
use std::path::{Path, PathBuf};

use super::types::{KnowledgeBaseIndex, KnowledgeBaseTextDocument, Settings};
use crate::workspace;

fn index_storage_path(id: &str) -> Result<PathBuf, String> {
    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Unable to locate the Windows AppData directory.".to_string())?;
    Ok(app_data
        .join("codex-input-enhancer")
        .join("indexes")
        .join(format!("{id}.md")))
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
    let path = match settings.knowledge_base_indexes.get(id) {
        Some(index) if index.mode == "generated" => index_storage_path(id)?,
        Some(index) if index.mode == "manual" => {
            knowledge_base_file(settings, id, &index.manual_path)?
        }
        _ => knowledge_base_file(settings, id, "INDEX.md")?,
    };
    read_bounded_text(&path, "knowledge base index")
}

pub fn read_small_knowledge_base_documents(
    settings: &Settings,
    id: &str,
    max_files: usize,
    max_total_bytes: u64,
) -> Result<Option<Vec<KnowledgeBaseTextDocument>>, String> {
    let candidates = knowledge_base_index_candidates(settings, id)?;
    if candidates.len() > max_files {
        return Ok(None);
    }
    let mut total_bytes = 0_u64;
    let mut paths = Vec::with_capacity(candidates.len());
    for relative_path in candidates {
        let path = knowledge_base_file(settings, id, &relative_path)?;
        total_bytes = total_bytes.saturating_add(
            fs::metadata(&path)
                .map_err(|error| error.to_string())?
                .len(),
        );
        if total_bytes > max_total_bytes {
            return Ok(None);
        }
        paths.push((relative_path, path));
    }
    paths
        .into_iter()
        .map(|(relative_path, path)| {
            fs::read_to_string(path)
                .map(|content| KnowledgeBaseTextDocument {
                    relative_path,
                    content,
                })
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
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
    if fs::metadata(path)
        .map_err(|_| format!("{kind} 不存在。"))?
        .len()
        > 32_000
    {
        return Err(format!("{kind} 超过 32 KB 限制。"));
    }
    fs::read_to_string(path).map_err(|error| error.to_string())
}
