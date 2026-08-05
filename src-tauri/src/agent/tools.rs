use std::collections::HashSet;

use tauri::{Emitter, WebviewWindow};

use crate::diagnostics::Diagnostics;
use crate::provider;
use crate::settings::ModelProvider;
use crate::workspace::{self, ToolScope};

use super::{MAX_AGENT_TOOL_ROUNDS, ToolLoopOutput, network};

fn definitions(local_tools_enabled: bool, allow_network: bool) -> serde_json::Value {
    let mut tools = Vec::new();
    if local_tools_enabled {
        tools.push(serde_json::json!({"type":"function","function":{"name":"search_files","description":"Search text in .md and .txt files inside an enabled local scope. Use this to find relevant local context before drafting.","parameters":{"type":"object","properties":{"scope":{"type":"string"},"path":{"type":"string"},"query":{"type":"string"}},"required":["scope","query"],"additionalProperties":false}}}));
        tools.push(serde_json::json!({"type":"function","function":{"name":"read_file","description":"Read one .md or .txt file inside an enabled local scope. The path must be relative to that scope.","parameters":{"type":"object","properties":{"scope":{"type":"string"},"path":{"type":"string"}},"required":["scope","path"],"additionalProperties":false}}}));
    }
    if allow_network {
        tools.extend(network::definitions());
    }
    serde_json::Value::Array(tools)
}

struct ToolExecution {
    content: Result<String, String>,
    error_kind: Option<&'static str>,
}

impl ToolExecution {
    fn success(content: String) -> Self {
        Self {
            content: Ok(content),
            error_kind: None,
        }
    }

    fn content(content: Result<String, String>) -> Self {
        Self {
            content,
            error_kind: None,
        }
    }

    fn failure(error: String, error_kind: Option<&'static str>) -> Self {
        Self {
            content: Err(error),
            error_kind,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    client: &reqwest::Client,
    api_key: &str,
    provider: &ModelProvider,
    system_prompt: &str,
    original: &str,
    scopes: &[ToolScope],
    allow_network: bool,
    max_local_tool_rounds: u8,
    window: &WebviewWindow,
    diagnostics: &Diagnostics,
    session_id: &str,
) -> Result<ToolLoopOutput, String> {
    let mut messages = vec![
        serde_json::json!({"role":"system","content":system_prompt}),
        serde_json::json!({"role":"user","content":original}),
    ];
    let mut local_tool_rounds = 0_u8;
    let mut completed_web_operations = HashSet::new();
    for _ in 0..MAX_AGENT_TOOL_ROUNDS {
        let local_tools_enabled = !scopes.is_empty() && local_tool_rounds < max_local_tool_rounds;
        let tools = definitions(local_tools_enabled, allow_network);
        let tool_definitions = tools
            .as_array()
            .filter(|items| !items.is_empty())
            .map(|_| &tools);
        let message = provider::complete(
            client,
            provider,
            api_key,
            &messages,
            tool_definitions,
            900,
            false,
        )
        .await?;
        let calls = message
            .get("tool_calls")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            let content = message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if content.contains("DSML") || content.contains("tool_calls") {
                messages.push(serde_json::json!({"role":"user","content":"Do not write DSML/XML/text tool syntax. If a tool is needed, call one of the supplied functions through the native tool_calls field only. Otherwise continue without a tool call."}));
                continue;
            }
            return Ok(ToolLoopOutput {
                messages,
                immediate_content: Some(content.to_string()),
            });
        }
        if calls.iter().any(|call| {
            !matches!(
                call.pointer("/function/name")
                    .and_then(serde_json::Value::as_str),
                Some("web_search" | "web_fetch")
            )
        }) {
            local_tool_rounds = local_tool_rounds.saturating_add(1);
        }
        messages.push(message);
        for call in calls {
            let id = call
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let name = call
                .pointer("/function/name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let arguments = call
                .pointer("/function/arguments")
                .and_then(serde_json::Value::as_str)
                .and_then(|text| serde_json::from_str(text).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            let status = match name {
                "search_files" => "正在检索已选知识库…",
                "read_file" => "正在阅读相关资料…",
                "web_search" => "正在检索公开资料…",
                "web_fetch" => "正在阅读公开资料…",
                _ => "正在处理上下文…",
            };
            let _ = window.emit("agent-status", status);
            let started = std::time::Instant::now();
            let result = if matches!(name, "web_search" | "web_fetch") {
                if !allow_network {
                    ToolExecution::failure(
                        "本次请求未开启联网权限。".to_string(),
                        Some("network_disabled"),
                    )
                } else if let Ok(key) = network::duplicate_key(name, &arguments)
                    && !completed_web_operations.insert(key)
                {
                    ToolExecution::failure(
                        "相同的联网操作已经完成，请基于已有资料继续。".to_string(),
                        Some("duplicate_operation"),
                    )
                } else {
                    match network::execute(name, &arguments).await {
                        Ok(output) => {
                            let _ = window.emit("agent-status", output.status);
                            ToolExecution::success(output.content)
                        }
                        Err(error) => ToolExecution::failure(error.to_string(), Some(error.kind())),
                    }
                }
            } else if local_tools_enabled {
                ToolExecution::content(workspace::execute_read_only_tool(name, &arguments, scopes))
            } else {
                ToolExecution::failure(
                    "本次请求未开放本地只读资料。".to_string(),
                    Some("local_tools_unavailable"),
                )
            };
            diagnostics.info("agent.tool_completed", Some(session_id), serde_json::json!({"tool":diagnostic_name(name),"success":result.content.is_ok(),"durationMs":started.elapsed().as_millis(),"errorKind":result.error_kind}));
            let content = result
                .content
                .unwrap_or_else(|error| format!("工具执行失败：{error}"));
            messages.push(serde_json::json!({"role":"tool","tool_call_id":id,"tool_name":name,"content":content}));
        }
    }
    Err("The local Agent reached its tool-round limit.".to_string())
}

fn diagnostic_name(name: &str) -> &'static str {
    match name {
        "list_files" => "list_files",
        "search_files" => "search_files",
        "read_file" => "read_file",
        "web_search" => "web_search",
        "web_fetch" => "web_fetch",
        _ => "unknown",
    }
}
