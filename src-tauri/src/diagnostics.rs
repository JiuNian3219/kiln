//! Local-only, privacy-preserving diagnostics for troubleshooting.
//!
//! Entries are newline-delimited JSON under AppData and never leave the
//! machine unless the user explicitly copies them from the control panel.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const RETENTION_DAYS: u64 = 14;
const MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024;
const MAX_RECENT_EVENTS: usize = 80;
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct Diagnostics {
    directory: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEvent {
    pub timestamp_ms: u64,
    pub level: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsPayload {
    pub retention_days: u64,
    pub max_total_bytes: u64,
    pub recent_events: Vec<DiagnosticEvent>,
    pub report: String,
}

impl Diagnostics {
    pub fn new() -> Self {
        let directory = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("codex-input-enhancer")
            .join("logs");
        let diagnostics = Self { directory };
        diagnostics.prune();
        diagnostics.record(
            "info",
            "app.started",
            None,
            None,
            json!({ "platform": "windows" }),
        );
        diagnostics
    }

    pub fn new_session_id(&self) -> String {
        format!(
            "s-{:x}-{:x}",
            now_millis(),
            SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    pub fn info(&self, event: &str, session_id: Option<&str>, metadata: Value) {
        self.record("info", event, session_id, None, metadata);
    }

    pub fn error(&self, event: &str, session_id: Option<&str>, error_code: &str, metadata: Value) {
        self.record("error", event, session_id, Some(error_code), metadata);
    }

    pub fn payload(&self) -> DiagnosticsPayload {
        let recent_events = self.recent_events();
        let report = diagnostic_report(&recent_events);
        DiagnosticsPayload {
            retention_days: RETENTION_DAYS,
            max_total_bytes: MAX_TOTAL_BYTES,
            recent_events,
            report,
        }
    }

    pub fn clear(&self) {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_log_file(&path) {
                let _ = fs::remove_file(path);
            }
        }
        self.info("diagnostics.cleared", None, json!({}));
    }

    fn record(
        &self,
        level: &str,
        event: &str,
        session_id: Option<&str>,
        error_code: Option<&str>,
        metadata: Value,
    ) {
        if fs::create_dir_all(&self.directory).is_err() {
            return;
        }
        let entry = DiagnosticEvent {
            timestamp_ms: now_millis(),
            level: level.to_string(),
            event: event.to_string(),
            session_id: session_id.map(str::to_owned),
            error_code: error_code.map(str::to_owned),
            metadata: sanitize_metadata(metadata),
        };
        let Ok(line) = serde_json::to_string(&entry) else {
            return;
        };
        let path = self
            .directory
            .join(format!("app-{}.jsonl", now_millis() / 86_400_000));
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }

    fn recent_events(&self) -> Vec<DiagnosticEvent> {
        let mut files = log_files(&self.directory);
        files.sort();
        let mut events = Vec::new();
        for file in files {
            let Ok(contents) = fs::read_to_string(file) else {
                continue;
            };
            for line in contents.lines() {
                if let Ok(event) = serde_json::from_str::<DiagnosticEvent>(line) {
                    events.push(event);
                }
            }
        }
        events.sort_by_key(|event| event.timestamp_ms);
        let start = events.len().saturating_sub(MAX_RECENT_EVENTS);
        events[start..].to_vec()
    }

    fn prune(&self) {
        let _ = fs::create_dir_all(&self.directory);
        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(RETENTION_DAYS * 86_400))
            .unwrap_or(UNIX_EPOCH);
        let mut files = Vec::new();
        for path in log_files(&self.directory) {
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if metadata
                .modified()
                .map(|modified| modified < cutoff)
                .unwrap_or(false)
            {
                let _ = fs::remove_file(path);
                continue;
            }
            files.push((path, metadata.len()));
        }
        files.sort_by_key(|(path, _)| path.clone());
        let mut total: u64 = files.iter().map(|(_, size)| size).sum();
        for (path, size) in files {
            if total <= MAX_TOTAL_BYTES {
                break;
            }
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn log_files(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_log_file(path.as_path()))
        .collect()
}

fn is_log_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("app-") && name.ends_with(".jsonl"))
}

fn sanitize_metadata(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .filter(|(key, _)| !is_sensitive_key(key))
                .map(|(key, value)| (key, sanitize_metadata(value)))
                .collect::<serde_json::Map<_, _>>(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(sanitize_metadata).collect()),
        Value::String(value) => Value::String(sanitize_text(&value)),
        value => value,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "key",
        "token",
        "secret",
        "password",
        "path",
        "text",
        "content",
        "query",
        "draft",
        "reference",
        "response",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn sanitize_text(value: &str) -> String {
    if value.len() > 120
        || value.contains(['\\', '/', '\n', '\r'])
        || value.to_ascii_lowercase().contains("sk-")
    {
        "[redacted]".to_string()
    } else {
        value.to_string()
    }
}

fn diagnostic_report(events: &[DiagnosticEvent]) -> String {
    let mut report = format!(
        "Codex Input Enhancer 本地诊断摘要\n保留策略：{RETENTION_DAYS} 天 / {} MB\n最近事件：{}\n",
        MAX_TOTAL_BYTES / 1024 / 1024,
        events.len()
    );
    for event in events {
        let code = event.error_code.as_deref().unwrap_or("-");
        let session = event.session_id.as_deref().unwrap_or("-");
        report.push_str(&format!(
            "{} | {} | {} | {} | {}\n",
            event.timestamp_ms, event.level, event.event, code, session
        ));
    }
    report
}
