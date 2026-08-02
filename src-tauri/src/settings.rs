mod behavior;
mod catalog;
mod knowledge_base;
mod repository;
mod types;

pub use behavior::{
    active_model_provider, feature_enabled, shortcut_for, supported_provider_protocol,
    valid_knowledge_base_inline_token_limit, validate_shortcut,
};
pub use catalog::{CatalogRepository, discover_catalog, discover_knowledge_bases};
pub use knowledge_base::{
    knowledge_base_index_candidates, knowledge_base_index_material, read_knowledge_base_index,
    read_small_knowledge_base_documents, save_generated_knowledge_base_index,
};
pub use repository::SettingsRepository;
pub use types::{
    CatalogEntry, Combination, CombinationInput, FeatureAndShortcutSaveResult,
    FeatureAndShortcutSettings, GENERAL_ENHANCEMENT_AGENT_ID, KnowledgeBaseIndex, ModelProvider,
    ModelProviderInput, Settings, SettingsPayload,
};

pub fn is_general_enhancement_agent(id: &str) -> bool {
    id == GENERAL_ENHANCEMENT_AGENT_ID
}

pub fn combination_agent_available(agent_id: &str, agents: &[CatalogEntry]) -> bool {
    is_general_enhancement_agent(agent_id) || agents.iter().any(|agent| agent.id == agent_id)
}

#[cfg(test)]
mod tests {
    use super::{CatalogEntry, GENERAL_ENHANCEMENT_AGENT_ID, combination_agent_available};

    #[test]
    fn accepts_the_builtin_general_agent_for_combinations() {
        let agents = [CatalogEntry {
            id: "writing".to_string(),
            name: "写作".to_string(),
            path: String::new(),
            index_status: None,
        }];
        assert!(combination_agent_available(
            GENERAL_ENHANCEMENT_AGENT_ID,
            &agents
        ));
        assert!(combination_agent_available("writing", &agents));
        assert!(!combination_agent_available("missing", &agents));
    }
}
