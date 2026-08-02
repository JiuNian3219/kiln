use std::time::Duration;

use tauri::{Emitter, WebviewWindow};

use crate::agent_protocol::{self, ExpectedLanguage};
use crate::diagnostics::Diagnostics;
use crate::provider;
use crate::session::{ClarificationPayload, ClarificationQuestion};
use crate::settings::ModelProvider;

use super::MAX_AUTOMATIC_RETRIES;

pub(super) fn parse_questions(
    text: &str,
    expected_language: ExpectedLanguage,
) -> Result<ClarificationPayload, String> {
    let text = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let value: serde_json::Value = serde_json::from_str(text)
        .or_else(|_| match (text.find('{'), text.rfind('}')) {
            (Some(start), Some(end)) if start < end => serde_json::from_str(&text[start..=end]),
            _ => serde_json::from_str(text),
        })
        .map_err(|_| "AI 服务未按约定返回澄清结果；请直接重试。".to_string())?;
    match value
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
    {
        "final" => Ok(ClarificationPayload {
            questions: Vec::new(),
            replacement: Some(agent_protocol::parse_final_output(text, expected_language)?),
        }),
        "error" => Err(format!(
            "Agent reported: {}",
            value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .unwrap_or("The Agent could not complete the request.")
        )),
        "questions" => {
            let items = value
                .get("questions")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| "DeepSeek returned questions in an invalid format.".to_string())?;
            if items.is_empty() || items.len() > 3 {
                return Err("AI 服务必须返回一到三个澄清问题。".to_string());
            }
            let mut questions = Vec::new();
            for (index, item) in items.iter().enumerate() {
                let prompt = item
                    .get("prompt")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "A clarification question is missing its prompt.".to_string())?;
                if role_confused(prompt) {
                    return Err("AI asked whether to answer the task directly instead of clarifying the downstream task.".to_string());
                }
                let options = item
                    .get("options")
                    .and_then(serde_json::Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .take(6)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                questions.push(ClarificationQuestion {
                    id: format!("q{}", index + 1),
                    prompt: prompt.to_owned(),
                    options,
                });
            }
            Ok(ClarificationPayload {
                questions,
                replacement: None,
            })
        }
        _ => Err("AI 服务返回了未知的澄清结果。".to_string()),
    }
}

fn role_confused(prompt: &str) -> bool {
    let normalized = prompt.to_lowercase();
    let asks_for_delivery = [
        "直接给",
        "直接回答",
        "直接生成",
        "直接提供",
        "帮您写一个提示词",
        "写一个提示词",
        "直接给你",
        "directly answer",
        "direct answer",
        "give you ideas",
        "write a prompt",
        "generate a prompt",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase));
    asks_for_delivery
        && (normalized.contains("还是")
            || normalized.contains("或者")
            || normalized.contains("or would you")
            || normalized.contains("or do you"))
}

fn retry_reason(error: &str) -> Option<&'static str> {
    if error.contains("HTTP 401")
        || error.contains("HTTP 403")
        || error.contains("HTTP 400")
        || error.contains("HTTP 422")
        || error.starts_with("Agent reported:")
    {
        None
    } else if error.contains("HTTP 429") {
        Some("rate_limited")
    } else if error.contains("HTTP 5") || error.contains("无法连接") {
        Some("network")
    } else if error.contains("invalid response") || error.contains("no response text") {
        Some("response")
    } else {
        Some("output_contract")
    }
}

fn retry_messages(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut repaired = Vec::with_capacity(messages.len() + 1);
    repaired.push(serde_json::json!({"role":"system","content":"The previous response failed host validation. Return only the exact required JSON object, use the selected draft language, and never include prose, Markdown fences, or textual tool syntax outside the required fields."}));
    repaired.extend_from_slice(messages);
    repaired
}

async fn wait_for_retry(
    stage: &str,
    retry: u8,
    reason: &str,
    window: &WebviewWindow,
    diagnostics: &Diagnostics,
    session_id: &str,
) -> Result<(), String> {
    diagnostics.info(
        &format!("agent.{stage}_retry"),
        Some(session_id),
        serde_json::json!({"retryAttempt":retry,"reason":reason}),
    );
    window
        .emit(
            "agent-status",
            format!("输出异常，正在自动重试（{retry}/{MAX_AUTOMATIC_RETRIES}）…"),
        )
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(500 * (1_u64 << (retry - 1))))
    })
    .await
    .map_err(|error| format!("Retry delay failed: {error}"))?;
    Ok(())
}

async fn model_text(
    client: &reqwest::Client,
    api_key: &str,
    provider: &ModelProvider,
    messages: Vec<serde_json::Value>,
    max_tokens: u32,
) -> Result<String, String> {
    provider::text(client, provider, api_key, &messages, max_tokens, true).await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn questions(
    client: &reqwest::Client,
    api_key: &str,
    provider: &ModelProvider,
    messages: &[serde_json::Value],
    expected_language: ExpectedLanguage,
    window: &WebviewWindow,
    diagnostics: &Diagnostics,
    session_id: &str,
) -> Result<ClarificationPayload, String> {
    for retry in 0..=MAX_AUTOMATIC_RETRIES {
        let result = model_text(
            client,
            api_key,
            provider,
            if retry == 0 {
                messages.to_vec()
            } else {
                retry_messages(messages)
            },
            700,
        )
        .await
        .and_then(|text| parse_questions(&text, expected_language));
        match result {
            Ok(payload) => return Ok(payload),
            Err(error) => {
                let Some(reason) = retry_reason(&error) else {
                    return Err(error
                        .strip_prefix("Agent reported: ")
                        .unwrap_or(&error)
                        .to_string());
                };
                if retry == MAX_AUTOMATIC_RETRIES {
                    return Err("分析结果格式异常，已自动重试 3 次，请稍后手动重试。".to_string());
                }
                wait_for_retry(
                    "analysis",
                    retry + 1,
                    reason,
                    window,
                    diagnostics,
                    session_id,
                )
                .await?;
            }
        }
    }
    unreachable!("retry loop always returns")
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn final_output(
    client: &reqwest::Client,
    api_key: &str,
    provider: &ModelProvider,
    messages: &[serde_json::Value],
    expected_language: ExpectedLanguage,
    window: &WebviewWindow,
    diagnostics: &Diagnostics,
    session_id: &str,
) -> Result<String, String> {
    for retry in 0..=MAX_AUTOMATIC_RETRIES {
        let result = model_text(
            client,
            api_key,
            provider,
            if retry == 0 {
                messages.to_vec()
            } else {
                retry_messages(messages)
            },
            1200,
        )
        .await
        .and_then(|text| agent_protocol::parse_final_output(&text, expected_language));
        match result {
            Ok(prompt) => return Ok(prompt),
            Err(error) => {
                let Some(reason) = retry_reason(&error) else {
                    return Err(error);
                };
                if retry == MAX_AUTOMATIC_RETRIES {
                    return Err(
                        "生成结果格式异常，已自动重试 3 次，请稍后手动重新生成。".to_string()
                    );
                }
                wait_for_retry(
                    "generation",
                    retry + 1,
                    reason,
                    window,
                    diagnostics,
                    session_id,
                )
                .await?;
            }
        }
    }
    unreachable!("retry loop always returns")
}
