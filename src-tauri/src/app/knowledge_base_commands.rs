use super::state::{AppState, refresh_catalog};
use super::window::settings_payload;
use crate::credential::WindowsCredentialStore;
use crate::provider;
use crate::settings::{
    KnowledgeBaseIndex, SettingsRepository, active_model_provider, knowledge_base_index_candidates,
    knowledge_base_index_material, save_generated_knowledge_base_index,
};
use tauri::State;

#[tauri::command]
pub(crate) fn get_knowledge_base_index_candidates(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    knowledge_base_index_candidates(&settings, &id)
}

#[tauri::command]
pub(crate) fn set_knowledge_base_index(
    id: String,
    mode: String,
    manual_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let settings = {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "Settings lock failed.".to_string())?;
        if mode == "manual" {
            let path = manual_path.unwrap_or_default();
            if !knowledge_base_index_candidates(&settings, &id)?
                .iter()
                .any(|item| item == &path)
            {
                return Err("请选择知识库内的 Markdown 或文本文件作为索引。".to_string());
            }
            settings.knowledge_base_indexes.insert(
                id,
                KnowledgeBaseIndex {
                    mode,
                    manual_path: path,
                },
            );
        } else {
            settings.knowledge_base_indexes.remove(&id);
        }
        SettingsRepository::save(&settings)?;
        settings.clone()
    };
    Ok(settings_payload(
        settings.clone(),
        refresh_catalog(&state, &settings)?,
    ))
}

#[tauri::command]
pub(crate) async fn generate_knowledge_base_index(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let (material, provider) = {
        let settings = state
            .settings
            .lock()
            .map_err(|_| "Settings lock failed.".to_string())?;
        (
            knowledge_base_index_material(&settings, &id)?,
            active_model_provider(&settings)?,
        )
    };
    let api_key = WindowsCredentialStore::load_for(&provider.id)?;
    let client = provider::client(std::time::Duration::from_secs(60))?;
    let content = provider::text(&client, &provider, &api_key, &[
        serde_json::json!({"role":"system","content":"根据给出的资料文件清单和有限摘录，生成简洁的中文知识库索引。说明主题、适用范围，并列出每个文件的用途。不要编造文件中没有的信息。仅输出 Markdown。"}),
        serde_json::json!({"role":"user","content":material}),
    ], 1200, false).await?;
    let settings = {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "Settings lock failed.".to_string())?;
        save_generated_knowledge_base_index(&mut settings, id, content)?;
        SettingsRepository::save(&settings)?;
        settings.clone()
    };
    Ok(settings_payload(
        settings.clone(),
        refresh_catalog(&state, &settings)?,
    ))
}
