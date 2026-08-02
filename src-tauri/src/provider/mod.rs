//! Provider-neutral model transport.
//!
//! The Agent uses OpenAI-shaped messages internally. Protocol adapters live in
//! sibling modules so orchestration never needs endpoint-specific details.

mod anthropic;
mod gemini;
mod openai;
mod responses;

use std::time::Duration;

use reqwest::Url;
use serde_json::Value;

use crate::settings::{ModelProvider, supported_provider_protocol};

pub fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| format!("无法创建网络客户端：{error}"))
}

pub fn validate_provider(provider: &ModelProvider) -> Result<(), String> {
    if provider.name.trim().is_empty() || provider.name.chars().count() > 48 {
        return Err("服务名称不能为空，且不能超过 48 个字符。".to_string());
    }
    if !supported_provider_protocol(&provider.protocol) {
        return Err("不支持的 API 协议。".to_string());
    }
    if provider.model.trim().is_empty() || provider.model.chars().count() > 160 {
        return Err("模型名称不能为空。".to_string());
    }
    let url =
        Url::parse(provider.base_url.trim()).map_err(|_| "服务地址不是有效 URL。".to_string())?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err("服务地址必须是公开 HTTPS 地址。".to_string());
    }
    Ok(())
}

pub async fn complete(
    client: &reqwest::Client,
    provider: &ModelProvider,
    api_key: &str,
    messages: &[Value],
    tools: Option<&Value>,
    max_tokens: u32,
    json_mode: bool,
) -> Result<Value, String> {
    validate_provider(provider)?;
    match provider.protocol.as_str() {
        "openai-chat-completions" => {
            openai::complete(
                client, provider, api_key, messages, tools, max_tokens, json_mode,
            )
            .await
        }
        "anthropic-messages" => {
            anthropic::complete(client, provider, api_key, messages, tools, max_tokens).await
        }
        "gemini-generate-content" => {
            gemini::complete(
                client, provider, api_key, messages, tools, max_tokens, json_mode,
            )
            .await
        }
        "openai-responses" => {
            responses::complete(
                client, provider, api_key, messages, tools, max_tokens, json_mode,
            )
            .await
        }
        _ => Err("不支持的 API 协议。".to_string()),
    }
}

pub async fn text(
    client: &reqwest::Client,
    provider: &ModelProvider,
    api_key: &str,
    messages: &[Value],
    max_tokens: u32,
    require_json: bool,
) -> Result<String, String> {
    complete(
        client,
        provider,
        api_key,
        messages,
        None,
        max_tokens,
        require_json,
    )
    .await?
    .get("content")
    .and_then(Value::as_str)
    .map(str::to_owned)
    .filter(|text| !text.trim().is_empty())
    .ok_or_else(|| "AI 服务未返回文本内容。".to_string())
}

pub(super) fn endpoint(provider: &ModelProvider, suffix: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(provider.base_url.trim()).map_err(|_| "服务地址不是有效 URL。".to_string())?;
    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}/{suffix}"));
    Ok(url)
}

pub(super) fn response_error(protocol: &str, status: reqwest::StatusCode, body: String) -> String {
    format!(
        "{protocol} 返回 HTTP {status}: {}",
        body.chars().take(320).collect::<String>()
    )
}

pub(super) fn system_text(messages: &[Value]) -> String {
    messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|message| message.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_explicit_protocols_and_https_endpoints() {
        let provider = ModelProvider {
            id: "x".into(),
            name: "测试".into(),
            protocol: "anthropic-messages".into(),
            base_url: "https://api.example.com".into(),
            model: "model".into(),
        };
        assert!(validate_provider(&provider).is_ok());
        assert!(
            validate_provider(&ModelProvider {
                base_url: "http://localhost".into(),
                ..provider
            })
            .is_err()
        );
    }
}
