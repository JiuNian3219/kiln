//! Bounded HTTP transport with redirect and content-type handling.

use futures_util::StreamExt;
use reqwest::{
    Response, StatusCode, Url,
    header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION},
};

use super::{WebError, safety};

const MAX_REDIRECTS: usize = 4;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;

pub(super) async fn read(mut url: Url, require_text_content: bool) -> Result<String, WebError> {
    for redirect_count in 0..=MAX_REDIRECTS {
        let client = safety::client_for(&url).await?;
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(request_error)?;
        if response.status().is_redirection() {
            if redirect_count == MAX_REDIRECTS {
                return Err(WebError::new("网页重定向次数过多。", "redirect_limit"));
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| WebError::new("网页重定向缺少目标地址。", "invalid_redirect"))?;
            url = url
                .join(location)
                .map_err(|_| WebError::new("网页重定向地址无效。", "invalid_redirect"))?;
            safety::public_url(url.as_str())?;
            continue;
        }
        if !response.status().is_success() {
            return Err(http_error(response.status()));
        }
        if require_text_content && !is_supported_content_type(response.headers().get(CONTENT_TYPE))
        {
            return Err(WebError::new(
                "仅支持读取网页、纯文本或 JSON 资料。",
                "unsupported_content_type",
            ));
        }
        if response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err(WebError::new(
                "网页响应过大，已停止读取。",
                "response_too_large",
            ));
        }
        return read_limited_body(response).await;
    }
    Err(WebError::new("网页重定向次数过多。", "redirect_limit"))
}

fn is_supported_content_type(value: Option<&reqwest::header::HeaderValue>) -> bool {
    value
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase())
        .is_some_and(|value| {
            value.starts_with("text/")
                || value.starts_with("application/json")
                || value.starts_with("application/ld+json")
        })
}

async fn read_limited_body(response: Response) -> Result<String, WebError> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(request_error)?;
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(WebError::new(
                "网页响应过大，已停止读取。",
                "response_too_large",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn request_error(error: reqwest::Error) -> WebError {
    if error.is_timeout() {
        WebError::new("联网请求超时。", "timeout")
    } else if error.is_connect() {
        WebError::new("无法连接公开资料服务。", "connection_failed")
    } else {
        WebError::new("联网请求失败。", "request_failed")
    }
}

fn http_error(status: StatusCode) -> WebError {
    match status.as_u16() {
        401 | 403 => WebError::new("公开资料服务拒绝了本次请求。", "http_forbidden"),
        404 => WebError::new("公开资料不存在或已移除。", "http_not_found"),
        429 => WebError::new("公开资料服务请求过于频繁。", "http_rate_limited"),
        _ => WebError::new("公开资料服务暂时不可用。", "http_failure"),
    }
}
