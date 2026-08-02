use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::behavior::{
    deepseek_provider, default_deepseek_model, default_feature_toggles,
    default_knowledge_base_inline_token_limit, default_reference_capture_mode,
    default_reference_shortcut, default_shortcuts,
};

pub const GENERAL_ENHANCEMENT_AGENT_ID: &str = "__general_enhancement__";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_deepseek_model")]
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
    #[serde(default = "default_knowledge_base_inline_token_limit")]
    pub knowledge_base_inline_token_limit: u32,
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
    #[serde(default)]
    pub model_providers: Vec<ModelProvider>,
    #[serde(default)]
    pub default_model_provider: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProvider {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProviderInput {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Combination {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub knowledge_base_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombinationInput {
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
    #[serde(default = "default_knowledge_base_inline_token_limit")]
    pub knowledge_base_inline_token_limit: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureAndShortcutSaveResult {
    pub success: bool,
    pub field_errors: HashMap<String, String>,
    pub settings: FeatureAndShortcutSettings,
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

#[derive(Clone, Debug)]
pub struct KnowledgeBaseTextDocument {
    pub relative_path: String,
    pub content: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: default_deepseek_model(),
            agents_root: String::new(),
            knowledge_bases_root: String::new(),
            default_agent: String::new(),
            default_knowledge_base: String::new(),
            combinations: Vec::new(),
            default_combination: String::new(),
            knowledge_base_indexes: HashMap::new(),
            knowledge_base_inline_token_limit: default_knowledge_base_inline_token_limit(),
            allow_network: false,
            reference_shortcut: default_reference_shortcut(),
            reference_capture_mode: default_reference_capture_mode(),
            feature_toggles: default_feature_toggles(),
            shortcuts: default_shortcuts(),
            model_providers: vec![deepseek_provider(default_deepseek_model())],
            default_model_provider: "deepseek".to_string(),
        }
    }
}
