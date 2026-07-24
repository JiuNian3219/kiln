//! Read-only, bounded access to user-selected Agent and knowledge-base folders.
//!
//! Paths are canonicalized before every read. This is the sole filesystem tool
//! surface exposed to the model; it never writes, follows an escaped path, or
//! reaches a root that was not selected for the current request.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ToolScope {
    pub id: String,
    pub root: PathBuf,
}

pub fn configured_scope_root(root: &str, selected_id: &str, kind: &str) -> Result<PathBuf, String> {
    let root = Path::new(root)
        .canonicalize()
        .map_err(|error| format!("Unable to access the {kind} root: {error}"))?;
    let scope = root
        .join(selected_id)
        .canonicalize()
        .map_err(|_| format!("The selected {kind} directory is unavailable."))?;
    if !scope.starts_with(&root) || !scope.is_dir() {
        return Err(format!(
            "The selected {kind} is outside its configured root."
        ));
    }
    Ok(scope)
}

pub fn read_configured_document(
    root: &str,
    selected_id: &str,
    marker: &str,
    kind: &str,
) -> Result<Option<String>, String> {
    if selected_id.trim().is_empty() {
        return Ok(None);
    }
    if Path::new(selected_id)
        .file_name()
        .and_then(|name| name.to_str())
        != Some(selected_id)
    {
        return Err(format!("Invalid {kind} selection."));
    }
    let root = Path::new(root)
        .canonicalize()
        .map_err(|error| format!("Unable to access the {kind} root: {error}"))?;
    let document = root
        .join(selected_id)
        .join(marker)
        .canonicalize()
        .map_err(|_| format!("The selected {kind} no longer contains {marker}."))?;
    if !document.starts_with(&root) {
        return Err(format!(
            "The selected {kind} is outside its configured root."
        ));
    }
    let metadata = fs::metadata(&document).map_err(|error| error.to_string())?;
    if metadata.len() > 32_000 {
        return Err(format!("{kind} {marker} is larger than the 32 KB limit."));
    }
    fs::read_to_string(document)
        .map(Some)
        .map_err(|error| format!("Unable to read {kind} {marker}: {error}"))
}

pub fn is_readable_text(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("md" | "txt")
    )
}

fn resolve_tool_path(scopes: &[ToolScope], scope_id: &str, path: &str) -> Result<PathBuf, String> {
    let scope_id = if scope_id == "knowledge_base" {
        scopes
            .iter()
            .find(|scope| scope.id.starts_with("knowledge_base_"))
            .map(|scope| scope.id.as_str())
            .unwrap_or(scope_id)
    } else {
        scope_id
    };
    let scope = scopes
        .iter()
        .find(|scope| scope.id == scope_id)
        .ok_or_else(|| "That tool scope is not enabled for this request.".to_string())?;
    let relative = Path::new(path);
    if relative.is_absolute() {
        return Err("Absolute paths are not allowed.".to_string());
    }
    let resolved =
        scope.root.join(relative).canonicalize().map_err(|_| {
            "The requested path does not exist inside the enabled scope.".to_string()
        })?;
    if !resolved.starts_with(&scope.root) {
        return Err("The requested path is outside the enabled scope.".to_string());
    }
    Ok(resolved)
}

fn collect_paths(root: &Path, current: &Path, depth: usize, output: &mut Vec<String>) {
    if depth > 4 || output.len() >= 120 {
        return;
    }
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    let mut entries = entries.filter_map(|entry| entry.ok()).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if output.len() >= 120 {
            break;
        }
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if path.is_dir() {
            output.push(format!("{}/", relative.to_string_lossy()));
            collect_paths(root, &path, depth + 1, output);
        } else if is_readable_text(&path) {
            output.push(relative.to_string_lossy().into_owned());
        }
    }
}

/// Executes only the local, read-only tools declared for the current scopes.
pub fn execute_read_only_tool(
    name: &str,
    arguments: &serde_json::Value,
    scopes: &[ToolScope],
) -> Result<String, String> {
    let name = if name == "read_text" {
        "read_file"
    } else {
        name
    };
    let scope_id = arguments
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Tool call is missing scope.".to_string())?;
    let path = arguments
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(".");
    let resolved = resolve_tool_path(scopes, scope_id, path)?;
    match name {
        "list_files" => {
            if !resolved.is_dir() {
                return Err("list_files requires a directory path.".to_string());
            }
            let mut paths = Vec::new();
            collect_paths(&resolved, &resolved, 0, &mut paths);
            Ok(if paths.is_empty() {
                "No readable files found.".to_string()
            } else {
                paths.join("\n")
            })
        }
        "read_file" => {
            if !resolved.is_file() || !is_readable_text(&resolved) {
                return Err("read_file only permits .md and .txt files.".to_string());
            }
            let metadata = fs::metadata(&resolved).map_err(|error| error.to_string())?;
            if metadata.len() > 32_000 {
                return Err("The requested file exceeds the 32 KB read limit.".to_string());
            }
            fs::read_to_string(resolved).map_err(|error| error.to_string())
        }
        "search_files" => {
            let query = arguments
                .get("query")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|query| !query.is_empty())
                .ok_or_else(|| "search_files requires a non-empty query.".to_string())?
                .to_lowercase();
            if !resolved.is_dir() {
                return Err("search_files requires a directory path.".to_string());
            }
            let mut paths = Vec::new();
            collect_paths(&resolved, &resolved, 0, &mut paths);
            let mut matches = Vec::new();
            for relative in paths.into_iter().filter(|path| !path.ends_with('/')) {
                if matches.len() >= 24 {
                    break;
                }
                let path = resolved.join(&relative);
                let Ok(content) = fs::read_to_string(&path) else {
                    continue;
                };
                for (line_number, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&query) {
                        matches.push(format!("{relative}:{}: {}", line_number + 1, line.trim()));
                        if matches.len() >= 24 {
                            break;
                        }
                    }
                }
            }
            Ok(if matches.is_empty() {
                "No matches found.".to_string()
            } else {
                matches.join("\n")
            })
        }
        _ => Err("Unknown local read-only tool.".to_string()),
    }
}
