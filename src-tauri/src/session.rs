//! Data passed between the preview UI and the per-hotkey rewrite session.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInput {
    pub use_agent: bool,
    pub use_knowledge_base: bool,
    pub use_network: bool,
    pub agent_id: String,
    pub knowledge_base_ids: Vec<String>,
    pub answers: Vec<String>,
    #[serde(default)]
    pub candidate: bool,
    #[serde(default)]
    pub use_reference: bool,
    #[serde(default)]
    pub reference_context_type: String,
    #[serde(default)]
    pub reference_context_note: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClarificationQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClarificationPayload {
    pub questions: Vec<ClarificationQuestion>,
}
