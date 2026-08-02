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
    json_mode: bool,
) -> Result<Value, String> {
    let mut body = json!({"systemInstruction":{"parts":[{"text":system_text(messages)}]},"contents":messages_for_api(messages),"generationConfig":{"maxOutputTokens":max_tokens}});
    if json_mode {
        body["generationConfig"]["responseMimeType"] = json!("application/json");
    }
    if let Some(tools) = tools {
        body["tools"] = json!([{"functionDeclarations":tools.as_array().cloned().unwrap_or_default().into_iter().filter_map(|tool| Some(json!({"name":tool.pointer("/function/name")?.as_str()?,"description":tool.pointer("/function/description").and_then(Value::as_str).unwrap_or_default(),"parameters":tool.pointer("/function/parameters")?.clone()}))).collect::<Vec<_>>()}]);
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
        Some(json!({"id":call.get("id").and_then(Value::as_str).unwrap_or(name),"type":"function","function":{"name":name,"arguments":serde_json::to_string(&call.get("args").cloned().unwrap_or_else(|| json!({}))).ok()?}}))
    }).collect::<Vec<_>>();
    if !calls.is_empty() {
        return Ok(json!({"role":"assistant","content":"","tool_calls":calls}));
    }
    Ok(
        json!({"role":"assistant","content":parts.iter().filter_map(|part| part.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("")}),
    )
}

fn messages_for_api(messages: &[Value]) -> Vec<Value> {
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
                if let Some(last) = result.last_mut().filter(|last| last.get("role").and_then(Value::as_str) == Some("user") && last.get("parts").and_then(Value::as_array).is_some_and(|parts| parts.iter().all(|part| part.get("functionResponse").is_some()))) { last["parts"].as_array_mut().expect("function response parts are an array").push(part); } else { result.push(json!({"role":"user","parts":[part]})); }
            }
            _ => {}
        }
    }
    result
}
