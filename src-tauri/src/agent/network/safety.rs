//! URL and DNS validation for public-web reads.

use std::{
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    time::Duration,
};

use reqwest::{Client, Url, redirect::Policy};

use super::WebError;

pub(super) fn fetch_url(arguments: &serde_json::Value) -> Result<Url, WebError> {
    let raw = arguments
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| WebError::new("网页读取需要有效的公开 URL。", "invalid_url"))?;
    public_url(raw)
}

pub(super) fn public_url(raw: &str) -> Result<Url, WebError> {
    let url = Url::parse(raw)
        .map_err(|_| WebError::new("网页读取需要有效的公开 URL。", "invalid_url"))?;
    if !matches!(url.scheme(), "http" | "https") || url.username() != "" || url.password().is_some()
    {
        return Err(WebError::new(
            "仅允许读取不含凭据的公开 HTTP(S) 地址。",
            "invalid_url",
        ));
    }
    if url.host_str().is_none() {
        return Err(WebError::new("网页读取需要有效的公开 URL。", "invalid_url"));
    }
    Ok(url)
}

pub(super) async fn client_for(url: &Url) -> Result<Client, WebError> {
    let host = url
        .host_str()
        .ok_or_else(|| WebError::new("网页地址缺少主机名。", "invalid_url"))?;
    let addresses = public_addresses(host, url.port_or_known_default().unwrap_or(443)).await?;
    Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(12))
        .timeout(Duration::from_secs(20))
        .user_agent("Codex Input Enhancer/0.1")
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(|_| WebError::new("无法创建联网请求。", "client_creation"))
}

async fn public_addresses(host: &str, port: u16) -> Result<Vec<SocketAddr>, WebError> {
    let host = host.to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return Err(WebError::new(
            "不允许读取本机或局域网地址。",
            "blocked_address",
        ));
    }
    let addresses = tauri::async_runtime::spawn_blocking(move || {
        (host.as_str(), port)
            .to_socket_addrs()
            .map(|items| items.collect::<Vec<_>>())
    })
    .await
    .map_err(|_| WebError::new("无法解析公开网页地址。", "dns_failure"))?
    .map_err(|_| WebError::new("无法解析公开网页地址。", "dns_failure"))?;
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(WebError::new(
            "不允许读取本机或局域网地址。",
            "blocked_address",
        ));
    }
    Ok(addresses)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_multicast()
                && first != 0
                && first != 10
                && !(first == 100 && (64..=127).contains(&second))
                && !(first == 172 && (16..=31).contains(&second))
                && !(first == 192 && second == 168)
                && first < 224
        }
        IpAddr::V6(ip) => {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_multicast()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_local_and_private_addresses() {
        assert!(!is_public_ip("127.0.0.1".parse().expect("IP")));
        assert!(!is_public_ip("10.0.0.1".parse().expect("IP")));
        assert!(!is_public_ip("192.168.1.1".parse().expect("IP")));
        assert!(!is_public_ip("::1".parse().expect("IP")));
        assert!(is_public_ip("8.8.8.8".parse().expect("IP")));
    }
}
