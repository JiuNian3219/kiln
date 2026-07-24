//! DeepSeek transport primitives.
//!
//! Credentials are deliberately supplied by the caller for each request. This
//! module never persists or logs API keys, selected text, prompts, or responses.

use std::time::Duration;

pub const CHAT_COMPLETIONS_URL: &str = "https://api.deepseek.com/chat/completions";

pub fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| format!("Unable to create the HTTP client: {error}"))
}

pub async fn test_connection(model: &str, api_key: &str) -> Result<String, String> {
    let response = client(Duration::from_secs(20))?
        .post(CHAT_COMPLETIONS_URL)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": "Reply with exactly OK." }],
            "stream": false,
            "max_tokens": 8,
            "thinking": { "type": "disabled" }
        }))
        .send()
        .await
        .map_err(|error| format!("Unable to reach DeepSeek: {error}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "DeepSeek returned HTTP {status}: {}",
            body.chars().take(320).collect::<String>()
        ));
    }
    let reply = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|payload| {
            payload
                .pointer("/choices/0/message/content")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Connected".to_string());
    Ok(reply)
}
