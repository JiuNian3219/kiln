use serde_json::{Value, json};

use super::{endpoint, response_error, system_text};
use crate::settings::ModelProvider;

pub(super) async fn complete(
    client: &reqwest::Client,
    provider: &ModelProvider,
    api_key: &str,
    messages: &[Value],
    tools: Option<&Value>,
    max_tokens: u32,
) -> Result<Value, String> {
    let mut body = json!({
        "model": provider.model,
        "system": system_text(messages),
        "messages": messages_for_api(messages),
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
        Some(json!({"id":block.get("id")?.as_str()?,"type":"function","function":{"name":block.get("name")?.as_str()?,"arguments":serde_json::to_string(block.get("input")?).ok()?}}))
    }).collect::<Vec<_>>();
    if !calls.is_empty() {
        return Ok(json!({"role":"assistant","content":"","tool_calls":calls}));
    }
    let content = blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");
    Ok(json!({"role":"assistant","content":content}))
}

fn messages_for_api(messages: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    for message in messages {
        match message.get("role").and_then(Value::as_str) {
            Some("user") => result.push(json!({"role":"user","content":message.get("content").cloned().unwrap_or(Value::String(String::new()))})),
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
                let block = json!({"type":"tool_result","tool_use_id":message.get("tool_call_id").and_then(Value::as_str).unwrap_or_default(),"content":message.get("content").and_then(Value::as_str).unwrap_or_default()});
                if let Some(last) = result.last_mut().filter(|last| last.get("role").and_then(Value::as_str) == Some("user") && last.get("content").and_then(Value::as_array).is_some()) {
                    last["content"].as_array_mut().expect("tool result content is an array").push(block);
                } else { result.push(json!({"role":"user","content":[block]})); }
            }
            _ => {}
        }
    }
    result
}
