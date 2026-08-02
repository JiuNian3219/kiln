use crate::session::SessionInput;
use crate::settings::{
    Settings, is_general_enhancement_agent, read_knowledge_base_index,
    read_small_knowledge_base_documents,
};
use crate::workspace::{self, ToolScope};

use super::{
    KnowledgeBaseDelivery, MAX_INLINE_KNOWLEDGE_BASE_BYTES, MAX_INLINE_KNOWLEDGE_BASE_FILES,
    SessionContext,
};

fn agent_tool_scope(settings: &Settings, input: &SessionInput) -> Result<Vec<ToolScope>, String> {
    if input.use_agent
        && !input.agent_id.trim().is_empty()
        && !is_general_enhancement_agent(&input.agent_id)
    {
        return Ok(vec![ToolScope {
            id: "agent".to_string(),
            root: workspace::configured_scope_root(
                &settings.agents_root,
                &input.agent_id,
                "Agent",
            )?,
        }]);
    }
    Ok(Vec::new())
}

fn knowledge_base_tool_scopes(
    settings: &Settings,
    input: &SessionInput,
) -> Result<Vec<ToolScope>, String> {
    input
        .knowledge_base_ids
        .iter()
        .filter(|id| !id.trim().is_empty())
        .enumerate()
        .map(|(index, id)| {
            Ok(ToolScope {
                id: format!("knowledge_base_{}", index + 1),
                root: workspace::configured_scope_root(
                    &settings.knowledge_bases_root,
                    id,
                    "knowledge base",
                )?,
            })
        })
        .collect()
}

fn estimate_tokens(text: &str) -> u32 {
    let mut estimate = 0_u32;
    let mut ascii_run = 0_u32;
    for character in text.chars() {
        if character.is_ascii() {
            ascii_run = ascii_run.saturating_add(1);
        } else {
            estimate = estimate.saturating_add(ascii_run.div_ceil(4));
            ascii_run = 0;
            if !character.is_whitespace() {
                estimate = estimate.saturating_add(1);
            }
        }
    }
    estimate.saturating_add(ascii_run.div_ceil(4))
}

fn inline_material(settings: &Settings, id: &str) -> Result<Option<String>, String> {
    let Some(documents) = read_small_knowledge_base_documents(
        settings,
        id,
        MAX_INLINE_KNOWLEDGE_BASE_FILES,
        MAX_INLINE_KNOWLEDGE_BASE_BYTES,
    )?
    else {
        return Ok(None);
    };
    let documents = documents
        .into_iter()
        .map(|document| {
            format!(
                "<document path=\"{}\">\n{}\n</document>",
                document.relative_path, document.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(Some(format!(
        "<knowledge-base id=\"{id}\">\n{documents}\n</knowledge-base>"
    )))
}

pub(super) fn documents(
    settings: &Settings,
    input: &SessionInput,
) -> Result<SessionContext, String> {
    let agent = if input.use_agent
        && !input.agent_id.trim().is_empty()
        && !is_general_enhancement_agent(&input.agent_id)
    {
        workspace::read_configured_document(
            &settings.agents_root,
            &input.agent_id,
            "AGENT.md",
            "Agent",
        )?
    } else {
        None
    };
    let selected = if input.use_knowledge_base {
        input
            .knowledge_base_ids
            .iter()
            .filter(|id| !id.trim().is_empty())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut inline = Vec::new();
    let mut can_inline = !selected.is_empty();
    for id in &selected {
        match inline_material(settings, id)? {
            Some(material) => inline.push(material),
            None => {
                can_inline = false;
                break;
            }
        }
    }
    let inline_text = inline.join("\n\n");
    let inline_tokens = estimate_tokens(&inline_text);
    if inline_text.len() as u64 > MAX_INLINE_KNOWLEDGE_BASE_BYTES
        || inline_tokens > settings.knowledge_base_inline_token_limit
    {
        can_inline = false;
    }
    let (knowledge_bases, knowledge_base_delivery, knowledge_scopes) = if selected.is_empty() {
        (
            "(No knowledge base selected.)".to_string(),
            KnowledgeBaseDelivery::None,
            Vec::new(),
        )
    } else if can_inline {
        (
            inline_text,
            KnowledgeBaseDelivery::Inline {
                estimated_tokens: inline_tokens,
            },
            Vec::new(),
        )
    } else {
        let indexes = selected
            .iter()
            .map(|id| {
                read_knowledge_base_index(settings, id).map(|index| {
                    format!("<knowledge-base id=\"{id}\">\n{index}\n</knowledge-base>")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        (
            indexes.join("\n\n"),
            KnowledgeBaseDelivery::Retrieval,
            knowledge_base_tool_scopes(settings, input)?,
        )
    };
    let mut local_scopes = agent_tool_scope(settings, input)?;
    local_scopes.extend(knowledge_scopes);
    Ok(SessionContext {
        agent,
        knowledge_bases,
        local_scopes,
        knowledge_base_delivery,
    })
}

pub(super) fn validate(input: &SessionInput) -> Result<(), String> {
    if input.use_agent && input.agent_id.trim().is_empty() {
        return Err("请选择本次要使用的 Agent，或取消 Agent 上下文。".to_string());
    }
    if input.use_knowledge_base
        && !input
            .knowledge_base_ids
            .iter()
            .any(|id| !id.trim().is_empty())
    {
        return Err("请至少选择一个知识库，或取消知识库上下文。".to_string());
    }
    if input.knowledge_base_ids.len() > 6 {
        return Err("一次最多可选择 6 个知识库。".to_string());
    }
    Ok(())
}
