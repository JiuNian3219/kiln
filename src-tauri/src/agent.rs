//! Per-request Agent orchestration. It only receives the settings and context
//! explicitly selected for a single shortcut session.

mod context;
mod network;
mod output;
mod prompt;
mod tools;

use std::time::Duration;

use tauri::{Emitter, WebviewWindow};

use crate::agent_protocol;
use crate::credential::WindowsCredentialStore;
use crate::diagnostics::Diagnostics;
use crate::provider;
use crate::session::{ClarificationPayload, SessionInput};
use crate::settings::{ModelProvider, Settings, active_model_provider};
use crate::workspace::ToolScope;

const MAX_AUTOMATIC_RETRIES: u8 = 3;
const MAX_AGENT_TOOL_ROUNDS: u8 = 8;
const MAX_KNOWLEDGE_BASE_LOCAL_TOOL_ROUNDS: u8 = 2;
const MAX_INLINE_KNOWLEDGE_BASE_FILES: usize = 6;
const MAX_INLINE_KNOWLEDGE_BASE_BYTES: u64 = 12 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KnowledgeBaseDelivery {
    None,
    Inline { estimated_tokens: u32 },
    Retrieval,
}

struct SessionContext {
    agent: Option<String>,
    knowledge_bases: String,
    local_scopes: Vec<ToolScope>,
    knowledge_base_delivery: KnowledgeBaseDelivery,
}

struct ToolLoopOutput {
    messages: Vec<serde_json::Value>,
    immediate_content: Option<String>,
}
struct PreparedMessages {
    client: reqwest::Client,
    provider: ModelProvider,
    api_key: String,
    messages: Vec<serde_json::Value>,
    immediate_content: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn prepare(
    settings: Settings,
    system_prompt: String,
    user_message: String,
    local_scopes: &[ToolScope],
    allow_network: bool,
    max_local_tool_rounds: u8,
    window: &WebviewWindow,
    diagnostics: &Diagnostics,
    session_id: &str,
) -> Result<PreparedMessages, String> {
    let provider = active_model_provider(&settings)?;
    let api_key = WindowsCredentialStore::load_for(&provider.id)?;
    let client = provider::client(Duration::from_secs(60))?;
    let loop_output = if local_scopes.is_empty() && !allow_network {
        ToolLoopOutput {
            messages: vec![
                serde_json::json!({"role":"system","content":system_prompt}),
                serde_json::json!({"role":"user","content":user_message}),
            ],
            immediate_content: None,
        }
    } else {
        tools::run(
            &client,
            &api_key,
            &provider,
            &system_prompt,
            &user_message,
            local_scopes,
            allow_network,
            max_local_tool_rounds,
            window,
            diagnostics,
            session_id,
        )
        .await?
    };
    Ok(PreparedMessages {
        client,
        provider,
        api_key,
        messages: loop_output.messages,
        immediate_content: loop_output.immediate_content,
    })
}

pub async fn analyze(
    settings: Settings,
    original: String,
    input: &SessionInput,
    reference: Option<&str>,
    window: &WebviewWindow,
    diagnostics: &Diagnostics,
    session_id: &str,
) -> Result<ClarificationPayload, String> {
    context::validate(input)?;
    let context = context::documents(&settings, input)?;
    emit_knowledge_base_status(window, context.knowledge_base_delivery);
    let prepared = prepare(
        settings.clone(),
        agent_protocol::with_reference_context(
            prompt::planning(context.agent.as_deref(), &context.knowledge_bases),
            reference,
            &input.reference_context_type,
            &input.reference_context_note,
        ),
        agent_protocol::wrap_selected_draft(&original),
        &context.local_scopes,
        settings.allow_network && input.use_network,
        local_tool_round_limit(context.knowledge_base_delivery),
        window,
        diagnostics,
        session_id,
    )
    .await?;
    let expected_language = agent_protocol::expected_language(&original);
    if let Some(content) = prepared.immediate_content
        && let Ok(payload) = output::parse_questions(&content, expected_language)
    {
        return Ok(payload);
    }
    output::questions(
        &prepared.client,
        &prepared.api_key,
        &prepared.provider,
        &prepared.messages,
        expected_language,
        window,
        diagnostics,
        session_id,
    )
    .await
}

pub async fn generate(
    settings: Settings,
    original: String,
    input: &SessionInput,
    reference: Option<&str>,
    window: &WebviewWindow,
    diagnostics: &Diagnostics,
    session_id: &str,
) -> Result<String, String> {
    context::validate(input)?;
    let context = context::documents(&settings, input)?;
    emit_knowledge_base_status(window, context.knowledge_base_delivery);
    let prepared = prepare(
        settings.clone(),
        agent_protocol::with_reference_context(
            prompt::generation(context.agent.as_deref(), &context.knowledge_bases),
            reference,
            &input.reference_context_type,
            &input.reference_context_note,
        ),
        agent_protocol::wrap_draft_with_answers(&original, &input.answers),
        &context.local_scopes,
        settings.allow_network && input.use_network,
        local_tool_round_limit(context.knowledge_base_delivery),
        window,
        diagnostics,
        session_id,
    )
    .await?;
    let expected_language = agent_protocol::expected_language(&original);
    if let Some(content) = prepared.immediate_content
        && let Ok(prompt) = agent_protocol::parse_final_output(&content, expected_language)
    {
        return Ok(prompt);
    }
    output::final_output(
        &prepared.client,
        &prepared.api_key,
        &prepared.provider,
        &prepared.messages,
        expected_language,
        window,
        diagnostics,
        session_id,
    )
    .await
}

fn local_tool_round_limit(delivery: KnowledgeBaseDelivery) -> u8 {
    match delivery {
        KnowledgeBaseDelivery::Retrieval => MAX_KNOWLEDGE_BASE_LOCAL_TOOL_ROUNDS,
        KnowledgeBaseDelivery::None | KnowledgeBaseDelivery::Inline { .. } => MAX_AGENT_TOOL_ROUNDS,
    }
}

fn emit_knowledge_base_status(window: &WebviewWindow, delivery: KnowledgeBaseDelivery) {
    let status = match delivery {
        KnowledgeBaseDelivery::Inline { estimated_tokens } => {
            format!("已直接加载知识库上下文 · {estimated_tokens} tokens")
        }
        KnowledgeBaseDelivery::Retrieval => "知识库较大，按需检索中".to_string(),
        KnowledgeBaseDelivery::None => return,
    };
    let _ = window.emit("agent-status", status);
}

#[cfg(test)]
mod tests {
    use super::{output, prompt};
    use crate::agent_protocol::ExpectedLanguage;

    #[test]
    fn general_enhancement_keeps_the_draft_domain_and_tone() {
        let prompt = prompt::generation(None, "(No knowledge base selected.)");
        assert!(prompt.contains("smallest rewrite"));
        assert!(prompt.contains("Do not turn an informal thought"));
    }

    #[test]
    fn planning_prompt_preserves_reference_boundaries() {
        let prompt = prompt::planning(None, "context");
        assert!(prompt.contains("Reference context may supply facts"));
        assert!(prompt.contains("Resource references"));
    }

    #[test]
    fn parses_a_final_result_without_clarifications() {
        let payload = output::parse_questions(
            r#"{"kind":"final","prompt":"请完成这个任务"}"#,
            ExpectedLanguage::Chinese,
        )
        .expect("final payload should parse");
        assert_eq!(payload.questions.len(), 0);
        assert_eq!(payload.replacement.as_deref(), Some("请完成这个任务"));
    }

    #[test]
    fn rejects_delivery_choice_as_a_clarification() {
        let error = output::parse_questions(
            r#"{"kind":"questions","questions":[{"prompt":"你想直接回答还是写一个提示词？","options":[]}] }"#,
            ExpectedLanguage::Chinese,
        )
        .expect_err("delivery choice must be rejected");
        assert!(error.contains("directly"));
    }
}
