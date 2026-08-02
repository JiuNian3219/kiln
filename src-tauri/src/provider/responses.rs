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
    let mut body = json!({"model":provider.model,"instructions":system_text(messages),"input":input,"max_output_tokens":max_tokens});
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
