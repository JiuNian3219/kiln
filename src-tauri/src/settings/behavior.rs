use std::collections::HashMap;

use super::types::{ModelProvider, Settings};

pub(super) fn default_deepseek_model() -> String {
    "deepseek-v4-flash".to_string()
}
pub(super) fn default_reference_shortcut() -> String {
    "Ctrl+Shift+T".to_string()
}
pub(super) fn default_reference_capture_mode() -> String {
    "selection".to_string()
}
pub const MIN_KNOWLEDGE_BASE_INLINE_TOKEN_LIMIT: u32 = 500;
pub const MAX_KNOWLEDGE_BASE_INLINE_TOKEN_LIMIT: u32 = 8_000;
pub fn default_knowledge_base_inline_token_limit() -> u32 {
    2_500
}
pub fn valid_knowledge_base_inline_token_limit(value: u32) -> bool {
    (MIN_KNOWLEDGE_BASE_INLINE_TOKEN_LIMIT..=MAX_KNOWLEDGE_BASE_INLINE_TOKEN_LIMIT).contains(&value)
}

pub fn deepseek_provider(model: String) -> ModelProvider {
    ModelProvider {
        id: "deepseek".to_string(),
        name: "DeepSeek".to_string(),
        protocol: "openai-chat-completions".to_string(),
        base_url: "https://api.deepseek.com".to_string(),
        model,
    }
}

pub fn supported_provider_protocol(protocol: &str) -> bool {
    matches!(
        protocol,
        "openai-chat-completions"
            | "openai-responses"
            | "anthropic-messages"
            | "gemini-generate-content"
    )
}

pub fn active_model_provider(settings: &Settings) -> Result<ModelProvider, String> {
    let id = settings.default_model_provider.trim();
    settings
        .model_providers
        .iter()
        .find(|provider| provider.id == id)
        .or_else(|| settings.model_providers.first())
        .cloned()
        .ok_or_else(|| "尚未配置 AI 服务。请在控制面板中添加服务。".to_string())
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
