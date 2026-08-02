use super::state::{AppState, cached_catalog};
use super::window::settings_payload;
use crate::credential::WindowsCredentialStore;
use crate::provider;
use crate::settings::{ModelProvider, ModelProviderInput, SettingsRepository};
use tauri::State;

async fn test_provider_connection(provider: &ModelProvider) -> Result<String, String> {
    let api_key = WindowsCredentialStore::load_for(&provider.id)?;
    let client = provider::client(std::time::Duration::from_secs(20))?;
    let message = provider::complete(
        &client,
        provider,
        &api_key,
        &[
            serde_json::json!({"role":"system","content":"Reply with exactly OK."}),
            serde_json::json!({"role":"user","content":"OK"}),
        ],
        None,
        96,
        false,
    )
    .await?;
    let content = message
        .get("content")
        .and_then(serde_json::Value::as_str)
        .filter(|content| !content.trim().is_empty());
    Ok(content
        .map(str::to_owned)
        .unwrap_or_else(|| "服务已响应，但该模型本次未返回可见文本。".to_string()))
}

fn provider_id_from_name(name: &str, existing: &[ModelProvider]) -> String {
    let cleaned = name
        .chars()
        .filter_map(|character| {
            character
                .is_ascii_alphanumeric()
                .then(|| character.to_ascii_lowercase())
                .or_else(|| matches!(character, ' ' | '-' | '_').then_some('-'))
        })
        .collect::<String>();
    let base = cleaned.trim_matches('-');
    let base = if base.is_empty() { "provider" } else { base };
    let mut index = 1_u32;
    loop {
        let candidate = if index == 1 {
            base.to_string()
        } else {
            format!("{base}-{index}")
        };
        if !existing.iter().any(|provider| provider.id == candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[tauri::command]
pub(crate) fn save_model_provider(
    input: ModelProviderInput,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let catalog = cached_catalog(&state)?;
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    let id = if input.id.trim().is_empty() {
        provider_id_from_name(input.name.trim(), &settings.model_providers)
    } else {
        input.id.trim().to_string()
    };
    let provider = ModelProvider {
        id: id.clone(),
        name: input.name.trim().to_string(),
        protocol: input.protocol.trim().to_string(),
        base_url: input.base_url.trim().trim_end_matches('/').to_string(),
        model: input.model.trim().to_string(),
    };
    provider::validate_provider(&provider)?;
    if let Some(existing) = settings
        .model_providers
        .iter_mut()
        .find(|existing| existing.id == id)
    {
        *existing = provider;
    } else {
        settings.model_providers.push(provider);
    }
    if settings.default_model_provider.is_empty() {
        settings.default_model_provider = id.clone();
    }
    WindowsCredentialStore::save_for(&id, &input.api_key)?;
    SettingsRepository::save(&settings)?;
    Ok(settings_payload(settings.clone(), catalog))
}

#[tauri::command]
pub(crate) fn delete_model_provider(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let catalog = cached_catalog(&state)?;
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    if !settings
        .model_providers
        .iter()
        .any(|provider| provider.id == id)
    {
        return Err("AI 服务不存在。".to_string());
    }
    if settings.model_providers.len() == 1 {
        return Err("请至少保留一个 AI 服务。".to_string());
    }
    settings
        .model_providers
        .retain(|provider| provider.id != id);
    if settings.default_model_provider == id {
        settings.default_model_provider = settings
            .model_providers
            .first()
            .map(|provider| provider.id.clone())
            .unwrap_or_default();
    }
    WindowsCredentialStore::delete_for(&id)?;
    SettingsRepository::save(&settings)?;
    Ok(settings_payload(settings.clone(), catalog))
}

#[tauri::command]
pub(crate) fn set_default_model_provider(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let catalog = cached_catalog(&state)?;
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    if !settings
        .model_providers
        .iter()
        .any(|provider| provider.id == id)
    {
        return Err("AI 服务不存在。".to_string());
    }
    settings.default_model_provider = id;
    SettingsRepository::save(&settings)?;
    Ok(settings_payload(settings.clone(), catalog))
}

#[tauri::command]
pub(crate) async fn test_model_provider(
    id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let provider = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .model_providers
        .iter()
        .find(|provider| provider.id == id)
        .cloned()
        .ok_or_else(|| "AI 服务不存在。".to_string())?;
    test_provider_connection(&provider).await
}
