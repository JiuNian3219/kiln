use serde_json::{Value, json};

use super::{endpoint, response_error};
use crate::settings::ModelProvider;

enum UpstreamPayload {
    Json(Value),
    Events(Vec<Value>),
}

pub(super) async fn complete(
    client: &reqwest::Client,
    provider: &ModelProvider,
    api_key: &str,
    messages: &[Value],
    tools: Option<&Value>,
    max_tokens: u32,
    json_mode: bool,
) -> Result<Value, String> {
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
    match receive_payload(response, "OpenAI 兼容服务").await? {
        UpstreamPayload::Json(payload) => message(&payload),
        UpstreamPayload::Events(events) => Ok(stream_message(&events)),
    }
}

async fn receive_payload(
    response: reqwest::Response,
    service_name: &str,
) -> Result<UpstreamPayload, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("无法读取 {service_name} 的响应：{error}"))?;
    if !status.is_success() {
        return Err(response_error(service_name, status, body));
    }
    if let Ok(payload) = serde_json::from_str::<Value>(&body) {
        return Ok(UpstreamPayload::Json(payload));
    }
    let events = parse_sse_events(&body)?;
    if events.is_empty() {
        return Err(format!("{service_name} 返回了无法识别的响应格式。"));
    }
    Ok(UpstreamPayload::Events(events))
}

fn parse_sse_events(body: &str) -> Result<Vec<Value>, String> {
    let mut events = Vec::new();
    let mut data_lines = Vec::new();
    let mut saw_data = false;
    let flush = |data_lines: &mut Vec<&str>, events: &mut Vec<Value>| -> Result<(), String> {
        if data_lines.is_empty() {
            return Ok(());
        }
        let data = data_lines.join("\n");
        data_lines.clear();
        if data.trim() != "[DONE]" {
            events.push(
                serde_json::from_str(&data)
                    .map_err(|_| "AI 服务返回了无法解析的流式事件。".to_string())?,
            );
        }
        Ok(())
    };
    for line in body.lines() {
        if line.trim().is_empty() {
            flush(&mut data_lines, &mut events)?;
        } else if let Some(data) = line.strip_prefix("data:") {
            saw_data = true;
            data_lines.push(data.trim_start());
        }
    }
    flush(&mut data_lines, &mut events)?;
    saw_data
        .then_some(events)
        .ok_or_else(|| "AI 服务返回了无效响应。".to_string())
}

fn content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.to_string(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.get("content").and_then(Value::as_str))
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn message(payload: &Value) -> Result<Value, String> {
    let message = payload
        .pointer("/choices/0/message")
        .ok_or_else(|| "OpenAI 兼容服务未返回助手消息。".to_string())?;
    let mut normalized =
        json!({"role":"assistant", "content":content_text(message.get("content"))});
    if let Some(calls) = message.get("tool_calls").filter(|calls| calls.is_array()) {
        normalized["tool_calls"] = calls.clone();
    }
    Ok(normalized)
}

#[derive(Default)]
struct StreamToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn stream_message(events: &[Value]) -> Value {
    let mut content = String::new();
    let mut calls: Vec<StreamToolCall> = Vec::new();
    for choice in events.iter().flat_map(|event| {
        event
            .get("choices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
    }) {
        let Some(delta) = choice.get("delta") else {
            continue;
        };
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            content.push_str(text);
        }
        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            while calls.len() <= index {
                calls.push(StreamToolCall::default());
            }
            let target = &mut calls[index];
            if let Some(id) = call.get("id").and_then(Value::as_str) {
                target.id = id.to_string();
            }
            if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                target.name = name.to_string();
            }
            if let Some(arguments) = call.pointer("/function/arguments").and_then(Value::as_str) {
                target.arguments.push_str(arguments);
            }
        }
    }
    let tool_calls = calls
        .into_iter()
        .enumerate()
        .map(|(index, call)| {
            json!({
                "id": if call.id.is_empty() { format!("stream-call-{index}") } else { call.id },
                "type":"function", "function":{"name":call.name,"arguments":call.arguments},
            })
        })
        .collect::<Vec<_>>();
    let mut message = json!({"role":"assistant", "content":content});
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_openai_compatible_sse_without_exposing_transport_to_callers() {
        let events = parse_sse_events("data: {\"choices\":[{\"delta\":{\"content\":\"O\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"K\"}}]}\n\ndata: [DONE]\n\n").expect("SSE events should parse");
        assert_eq!(
            stream_message(&events)
                .get("content")
                .and_then(Value::as_str),
            Some("OK")
        );
    }

    #[test]
    fn normalizes_openai_content_parts() {
        let message = message(&json!({"choices":[{"message":{"content":[{"text":"OK"}]}}]}))
            .expect("message should normalize");
        assert_eq!(message.get("content").and_then(Value::as_str), Some("OK"));
    }
}
