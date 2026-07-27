//! Provider-neutral model transport.
//!
//! The Agent uses OpenAI-shaped messages internally. This module is the only
//! place that translates those messages to a remote API protocol, keeping API
//! keys, endpoint details and protocol differences out of orchestration code.

use std::time::Duration;

use reqwest::Url;
use serde_json::{Value, json};

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

fn endpoint(provider: &ModelProvider, suffix: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(provider.base_url.trim()).map_err(|_| "服务地址不是有效 URL。".to_string())?;
    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}/{suffix}"));
    Ok(url)
}

fn response_error(protocol: &str, status: reqwest::StatusCode, body: String) -> String {
    format!(
        "{protocol} 返回 HTTP {status}: {}",
        body.chars().take(320).collect::<String>()
    )
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
            let mut body = json!({
                "model": provider.model,
                "messages": messages,
                "stream": false,
                "max_tokens": max_tokens,
            });
            if let Some(tools) = tools {
                body["tools"] = tools.clone();
            } else {
                body["tool_choice"] = json!("none");
            }
            if json_mode {
                body["response_format"] = json!({"type":"json_object"});
            }
            if provider.id == "deepseek" {
                body["thinking"] = json!({"type":"disabled"});
            }
            let response = client
                .post(endpoint(provider, "chat/completions")?)
                .bearer_auth(api_key)
                .json(&body)
                .send()
                .await
                .map_err(|error| format!("无法连接 AI 服务：{error}"))?;
            let status = response.status();
            let payload: Value = response
                .json()
                .await
                .map_err(|error| format!("OpenAI 兼容服务返回了无效响应：{error}"))?;
            if !status.is_success() {
                return Err(response_error(
                    "OpenAI 兼容服务",
                    status,
                    payload.to_string(),
                ));
            }
            payload
                .pointer("/choices/0/message")
                .cloned()
                .ok_or_else(|| "OpenAI 兼容服务未返回助手消息。".to_string())
        }
        "anthropic-messages" => {
            anthropic_complete(client, provider, api_key, messages, tools, max_tokens).await
        }
        "gemini-generate-content" => {
            gemini_complete(
                client, provider, api_key, messages, tools, max_tokens, json_mode,
            )
            .await
        }
        "openai-responses" => {
            responses_complete(
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
    let message = complete(
        client,
        provider,
        api_key,
        messages,
        None,
        max_tokens,
        require_json,
    )
    .await?;
    message
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "AI 服务未返回文本内容。".to_string())
}

async fn anthropic_complete(
    client: &reqwest::Client,
    provider: &ModelProvider,
    api_key: &str,
    messages: &[Value],
    tools: Option<&Value>,
    max_tokens: u32,
) -> Result<Value, String> {
    let system = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|message| message.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut body = json!({
        "model": provider.model,
        "system": system,
        "messages": anthropic_messages(messages),
        "max_tokens": max_tokens,
    });
    if let Some(tools) = tools {
        body["tools"] = Value::Array(
            tools.as_array().cloned().unwrap_or_default().into_iter().filter_map(|tool| {
                Some(json!({
                    "name": tool.pointer("/function/name")?.as_str()?,
                    "description": tool.pointer("/function/description").and_then(Value::as_str).unwrap_or_default(),
                    "input_schema": tool.pointer("/function/parameters")?.clone(),
                }))
            }).collect(),
        );
    }
    let response = client
        .post(endpoint(provider, "v1/messages")?)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("无法连接 AI 服务：{error}"))?;
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("Anthropic 服务返回了无效响应：{error}"))?;
    if !status.is_success() {
        return Err(response_error(
            "Anthropic 服务",
            status,
            payload.to_string(),
        ));
    }
    let blocks = payload
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| "Anthropic 服务未返回内容。".to_string())?;
    let calls = blocks.iter().filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use")).filter_map(|block| {
        Some(json!({"id": block.get("id")?.as_str()?, "type":"function", "function":{"name":block.get("name")?.as_str()?, "arguments":serde_json::to_string(block.get("input")?).ok()?}}))
    }).collect::<Vec<_>>();
    if !calls.is_empty() {
        return Ok(json!({"role":"assistant", "content":"", "tool_calls": calls}));
    }
    let content = blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");
    Ok(json!({"role":"assistant", "content":content}))
}

fn anthropic_messages(messages: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    for message in messages {
        match message.get("role").and_then(Value::as_str) {
            Some("user") => result.push(json!({"role":"user", "content":message.get("content").cloned().unwrap_or(Value::String(String::new()))})),
            Some("assistant") => {
                let mut content = Vec::new();
                if let Some(text) = message.get("content").and_then(Value::as_str).filter(|text| !text.is_empty()) { content.push(json!({"type":"text","text":text})); }
                for call in message.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
                    if let (Some(id), Some(name), Some(arguments)) = (call.get("id").and_then(Value::as_str), call.pointer("/function/name").and_then(Value::as_str), call.pointer("/function/arguments").and_then(Value::as_str)) {
                        content.push(json!({"type":"tool_use","id":id,"name":name,"input":serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}))}));
                    }
                }
                result.push(json!({"role":"assistant","content":content}));
            }
            Some("tool") => {
                let result_block = json!({"type":"tool_result","tool_use_id":message.get("tool_call_id").and_then(Value::as_str).unwrap_or_default(),"content":message.get("content").and_then(Value::as_str).unwrap_or_default()});
                if let Some(last) = result.last_mut().filter(|last| {
                    last.get("role").and_then(Value::as_str) == Some("user")
                        && last.get("content").and_then(Value::as_array).is_some()
                }) {
                    last["content"].as_array_mut().expect("tool result content is an array").push(result_block);
                } else {
                    result.push(json!({"role":"user","content":[result_block]}));
                }
            }
            _ => {}
        }
    }
    result
}

async fn gemini_complete(
    client: &reqwest::Client,
    provider: &ModelProvider,
    api_key: &str,
    messages: &[Value],
    tools: Option<&Value>,
    max_tokens: u32,
    json_mode: bool,
) -> Result<Value, String> {
    let system = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|message| message.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut body = json!({"systemInstruction":{"parts":[{"text":system}]}, "contents":gemini_messages(messages), "generationConfig":{"maxOutputTokens":max_tokens}});
    if json_mode {
        body["generationConfig"]["responseMimeType"] = json!("application/json");
    }
    if let Some(tools) = tools {
        body["tools"] = json!([{"functionDeclarations": tools.as_array().cloned().unwrap_or_default().into_iter().filter_map(|tool| Some(json!({"name":tool.pointer("/function/name")?.as_str()?, "description":tool.pointer("/function/description").and_then(Value::as_str).unwrap_or_default(), "parameters":tool.pointer("/function/parameters")?.clone()}))).collect::<Vec<_>>() }]);
    }
    let response = client
        .post(endpoint(
            provider,
            &format!("v1beta/models/{}:generateContent", provider.model),
        )?)
        .query(&[("key", api_key)])
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("无法连接 AI 服务：{error}"))?;
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("Gemini 服务返回了无效响应：{error}"))?;
    if !status.is_success() {
        return Err(response_error("Gemini 服务", status, payload.to_string()));
    }
    let parts = payload
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .ok_or_else(|| "Gemini 服务未返回内容。".to_string())?;
    let calls = parts.iter().filter_map(|part| part.get("functionCall")).filter_map(|call| {
        let name = call.get("name")?.as_str()?;
        let id = call.get("id").and_then(Value::as_str).unwrap_or(name);
        let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
        Some(json!({"id":id,"type":"function","function":{"name":name,"arguments":serde_json::to_string(&args).ok()?}}))
    }).collect::<Vec<_>>();
    if !calls.is_empty() {
        return Ok(json!({"role":"assistant","content":"","tool_calls":calls}));
    }
    let content = parts
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");
    Ok(json!({"role":"assistant","content":content}))
}

fn gemini_messages(messages: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    for message in messages {
        match message.get("role").and_then(Value::as_str) {
        Some("user") => result.push(json!({"role":"user","parts":[{"text":message.get("content").and_then(Value::as_str).unwrap_or_default()}]})),
        Some("assistant") => {
            let mut parts = Vec::new();
            if let Some(text) = message.get("content").and_then(Value::as_str).filter(|text| !text.is_empty()) { parts.push(json!({"text":text})); }
            for call in message.get("tool_calls").and_then(Value::as_array).into_iter().flatten() { if let (Some(name), Some(arguments)) = (call.pointer("/function/name").and_then(Value::as_str), call.pointer("/function/arguments").and_then(Value::as_str)) { parts.push(json!({"functionCall":{"name":name,"args":serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}))}})); } }
            result.push(json!({"role":"model","parts":parts}));
        }
        Some("tool") => {
            let part = json!({"functionResponse":{"name":message.get("tool_name").and_then(Value::as_str).unwrap_or_default(),"response":{"content":message.get("content").and_then(Value::as_str).unwrap_or_default()}}});
            if let Some(last) = result.last_mut().filter(|last| {
                last.get("role").and_then(Value::as_str) == Some("user")
                    && last.get("parts").and_then(Value::as_array).is_some_and(|parts| {
                        parts.iter().all(|part| part.get("functionResponse").is_some())
                    })
            }) {
                last["parts"].as_array_mut().expect("function response parts are an array").push(part);
            } else {
                result.push(json!({"role":"user","parts":[part]}));
            }
        }
        _ => {}
    }
    }
    result
}

async fn responses_complete(
    client: &reqwest::Client,
    provider: &ModelProvider,
    api_key: &str,
    messages: &[Value],
    tools: Option<&Value>,
    max_tokens: u32,
    json_mode: bool,
) -> Result<Value, String> {
    let instructions = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|message| message.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut input = Vec::new();
    for message in messages {
        match message.get("role").and_then(Value::as_str) {
            Some("user") => input.push(json!({"role":"user","content":[{"type":"input_text","text":message.get("content").and_then(Value::as_str).unwrap_or_default()}]})),
            Some("assistant") => {
                for call in message.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
                    if let (Some(id), Some(name), Some(arguments)) = (call.get("id").and_then(Value::as_str), call.pointer("/function/name").and_then(Value::as_str), call.pointer("/function/arguments").and_then(Value::as_str)) {
                        input.push(json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments}));
                    }
                }
            }
            Some("tool") => input.push(json!({"type":"function_call_output","call_id":message.get("tool_call_id").and_then(Value::as_str).unwrap_or_default(),"output":message.get("content").and_then(Value::as_str).unwrap_or_default()})),
            _ => {}
        }
    }
    let mut body = json!({"model":provider.model,"instructions":instructions,"input":input,"max_output_tokens":max_tokens});
    if json_mode {
        body["text"] = json!({"format":{"type":"json_object"}});
    }
    if let Some(tools) = tools {
        body["tools"] = Value::Array(tools.as_array().cloned().unwrap_or_default().into_iter().filter_map(|tool| Some(json!({"type":"function","name":tool.pointer("/function/name")?.as_str()?,"description":tool.pointer("/function/description").and_then(Value::as_str).unwrap_or_default(),"parameters":tool.pointer("/function/parameters")?.clone()}))).collect());
    }
    let response = client
        .post(endpoint(provider, "responses")?)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("无法连接 AI 服务：{error}"))?;
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("OpenAI Responses 服务返回了无效响应：{error}"))?;
    if !status.is_success() {
        return Err(response_error(
            "OpenAI Responses 服务",
            status,
            payload.to_string(),
        ));
    }
    let output = payload
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| "OpenAI Responses 服务未返回内容。".to_string())?;
    let calls = output.iter().filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call")).filter_map(|item| Some(json!({"id":item.get("call_id")?.as_str()?,"type":"function","function":{"name":item.get("name")?.as_str()?,"arguments":item.get("arguments")?.as_str()?}}))).collect::<Vec<_>>();
    if !calls.is_empty() {
        return Ok(json!({"role":"assistant","content":"","tool_calls":calls}));
    }
    let content = output
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");
    Ok(json!({"role":"assistant","content":content}))
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
