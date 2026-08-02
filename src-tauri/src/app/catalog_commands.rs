use super::state::{AppState, cached_catalog, refresh_catalog};
use super::window::settings_payload;
use crate::settings::{
    CatalogRepository, Combination, CombinationInput, SettingsRepository,
    combination_agent_available,
};
use tauri::State;

#[tauri::command]
pub(crate) fn import_agent(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    CatalogRepository::import_agent_file(&settings.agents_root, &source_path)?;
    Ok(settings_payload(
        settings.clone(),
        refresh_catalog(&state, &settings)?,
    ))
}

#[tauri::command]
pub(crate) fn import_knowledge_bases(
    source_paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?
        .clone();
    CatalogRepository::import_knowledge_bases(&settings.knowledge_bases_root, &source_paths)?;
    Ok(settings_payload(
        settings.clone(),
        refresh_catalog(&state, &settings)?,
    ))
}

#[tauri::command]
pub(crate) fn delete_agent(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let settings = {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "Settings lock failed.".to_string())?;
        CatalogRepository::delete_directory(&settings.agents_root, &id, "Agent")?;
        settings.combinations.retain(|item| item.agent_id != id);
        if !settings
            .combinations
            .iter()
            .any(|item| item.id == settings.default_combination)
        {
            settings.default_combination.clear();
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
pub(crate) fn delete_knowledge_base(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let settings = {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "Settings lock failed.".to_string())?;
        CatalogRepository::delete_directory(&settings.knowledge_bases_root, &id, "知识库")?;
        settings.knowledge_base_indexes.remove(&id);
        settings
            .combinations
            .retain(|item| !item.knowledge_base_ids.iter().any(|base| base == &id));
        if !settings
            .combinations
            .iter()
            .any(|item| item.id == settings.default_combination)
        {
            settings.default_combination.clear();
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
pub(crate) fn save_combination(
    input: CombinationInput,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let catalog = cached_catalog(&state)?;
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    let name = input.name.trim();
    if name.is_empty() || input.agent_id.trim().is_empty() || input.knowledge_base_ids.is_empty() {
        return Err("组合必须包含名称、一个 Agent 和至少一个知识库。".to_string());
    }
    if !combination_agent_available(&input.agent_id, &catalog.agents)
        || input.knowledge_base_ids.iter().any(|id| {
            !catalog
                .knowledge_bases
                .iter()
                .any(|item| item.id == *id && item.index_status.as_deref() != Some("缺少 INDEX"))
        })
    {
        return Err("组合引用了不可用或缺少 INDEX 的资料。".to_string());
    }
    let id = if input.id.trim().is_empty() {
        format!(
            "combo-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        )
    } else {
        input.id
    };
    let combination = Combination {
        id: id.clone(),
        name: name.to_string(),
        agent_id: input.agent_id,
        knowledge_base_ids: input.knowledge_base_ids,
    };
    if let Some(existing) = settings.combinations.iter_mut().find(|item| item.id == id) {
        *existing = combination;
    } else {
        settings.combinations.push(combination);
    }
    SettingsRepository::save(&settings)?;
    Ok(settings_payload(settings.clone(), catalog))
}

#[tauri::command]
pub(crate) fn delete_combination(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let catalog = cached_catalog(&state)?;
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    settings.combinations.retain(|item| item.id != id);
    if settings.default_combination == id {
        settings.default_combination.clear();
    }
    SettingsRepository::save(&settings)?;
    Ok(settings_payload(settings.clone(), catalog))
}

#[tauri::command]
pub(crate) fn set_default_combination(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsPayload, String> {
    let catalog = cached_catalog(&state)?;
    let mut settings = state
        .settings
        .lock()
        .map_err(|_| "Settings lock failed.".to_string())?;
    if !id.is_empty() && !settings.combinations.iter().any(|item| item.id == id) {
        return Err("组合不存在。".to_string());
    }
    settings.default_combination = id;
    SettingsRepository::save(&settings)?;
    Ok(settings_payload(settings.clone(), catalog))
}
