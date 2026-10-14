use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use axum::Json;
use futures::{stream, StreamExt};
use serde::Serialize;
use tokio::net::{lookup_host, TcpStream};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;
use url::Url;

use crate::config::{providers, proxies, Config};

const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const MAX_ENDPOINTS: usize = 12;
const MAX_PROXIES: usize = 24;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub status: &'static str,
    pub latency_ms: Option<f64>,
    pub detail: String,
    pub http_status: Option<u16>,
}

impl Probe {
    fn ok(start: Instant, detail: impl Into<String>) -> Self {
        Self {
            status: "reachable",
            latency_ms: Some(start.elapsed().as_secs_f64() * 1000.0),
            detail: detail.into(),
            http_status: None,
        }
    }

    fn failed(status: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status,
            latency_ms: None,
            detail: detail.into(),
            http_status: None,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointStatus {
    pub provider: String,
    pub url: String,
    pub dns: Probe,
    pub addresses: Vec<String>,
    pub ping: Probe,
    pub tcp: Probe,
    pub direct_http: Probe,
    pub provider_http: Probe,
    pub proxy: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub source: String,
    pub url: String,
    pub tcp: Probe,
    pub tunnel: Probe,
    pub target: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub checked_at: String,
    pub refresh_interval_seconds: u64,
    pub endpoints: Vec<EndpointStatus>,
    pub proxies: Vec<ProxyStatus>,
    pub no_proxy: Option<String>,
}

struct Endpoint {
    provider: String,
    url: String,
    proxy: Option<String>,
}

pub async fn status() -> Json<NetworkStatus> {
    static CACHE: OnceLock<Mutex<Option<(Instant, NetworkStatus)>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(|| Mutex::new(None)).lock().await;
    if let Some((sampled, report)) = cache.as_ref() {
        if sampled.elapsed() < REFRESH_INTERVAL {
            return Json(report.clone());
        }
    }
    let report = collect(Config::global()).await;
    *cache = Some((Instant::now(), report.clone()));
    Json(report)
}

fn public_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return "Invalid URL".to_string();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

fn endpoints(config: &Config) -> Vec<Endpoint> {
    let mut names = Vec::new();
    names.extend(providers::get_active_provider(config));
    names.extend(providers::get_secondary_provider(config));
    names.extend(["google".to_string(), "opencode_go".to_string()]);
    names.extend(providers::get_providers_map(config).into_keys());
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for name in names {
        let name = if name == "gemini" {
            "google".to_string()
        } else {
            name
        };
        if !seen.insert(name.clone()) {
            continue;
        }
        let mut definition_proxy = None;
        let url = match name.as_str() {
            "google" | "gemini" => config
                .get_param::<String>("GOOGLE_HOST")
                .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string()),
            "openai" => std::env::var("OPENAI_HOST")
                .ok()
                .or_else(|| {
                    config
                        .get_param::<String>("OPENAI_BASE_URL")
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                })
                .or_else(|| config.get_param::<String>("OPENAI_HOST").ok())
                .unwrap_or_else(|| "https://api.openai.com".to_string()),
            "anthropic" => config
                .get_param::<String>("ANTHROPIC_HOST")
                .unwrap_or_else(|_| "https://api.anthropic.com".to_string()),
            _ => {
                let Ok(loaded) = crate::config::declarative_providers::load_provider(&name) else {
                    continue;
                };
                let definition = loaded.config;
                let vars = definition.env_vars.unwrap_or_default();
                definition_proxy = definition.proxy.and_then(|proxy| {
                    crate::config::declarative_providers::expand_env_vars(&proxy, &vars).ok()
                });
                match crate::config::declarative_providers::expand_env_vars(
                    &definition.base_url,
                    &vars,
                ) {
                    Ok(url) => url,
                    Err(_) => definition.base_url,
                }
            }
        };
        let url = if url.contains("://") {
            url
        } else {
            format!("https://{url}")
        };
        let proxy = if matches!(name.as_str(), "google" | "gemini") {
            providers::get_provider_proxy(config, "google")
                .or_else(|| providers::get_provider_proxy(config, "gemini"))
        } else {
            definition_proxy.or_else(|| providers::get_provider_proxy(config, &name))
        };
        result.push(Endpoint {
            provider: name,
            url,
            proxy,
        });
        if result.len() == MAX_ENDPOINTS {
            break;
        }
    }
    result
}

async fn collect(config: &Config) -> NetworkStatus {
    let endpoints = endpoints(config);
    let mut proxy_urls = BTreeMap::new();
    for (id, entry) in proxies::get_proxies_map(config) {
        proxy_urls.insert(format!("Named proxy: {id}"), entry.url);
    }
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                proxy_urls.insert(format!("Environment: {key}"), value);
            }
        }
    }
    for endpoint in &endpoints {
        if let Some(proxy) = &endpoint.proxy {
            proxy_urls.insert(format!("Provider: {}", endpoint.provider), proxy.clone());
        }
    }
    let target = endpoints
        .iter()
        .find(|endpoint| endpoint.provider == "google")
        .map(|endpoint| endpoint.url.clone())
        .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
    let endpoint_checks = stream::iter(endpoints)
        .map(check_endpoint)
        .buffered(MAX_ENDPOINTS)
        .collect();
    let proxy_checks = stream::iter(proxy_urls.into_iter().take(MAX_PROXIES))
        .map(|(source, url)| check_proxy(source, url, target.clone()))
        .buffered(8)
        .collect();
    let (endpoints, proxies) = tokio::join!(endpoint_checks, proxy_checks);
    NetworkStatus {
        checked_at: chrono::Utc::now().to_rfc3339(),
        refresh_interval_seconds: REFRESH_INTERVAL.as_secs(),
        endpoints,
        proxies,
        no_proxy: std::env::var("NO_PROXY")
            .or_else(|_| std::env::var("no_proxy"))
            .ok(),
    }
}

async fn dns(host: &str, port: u16) -> (Probe, Vec<String>) {
    let start = Instant::now();
    match timeout(PROBE_TIMEOUT, lookup_host((host, port))).await {
        Ok(Ok(addresses)) => {
            let mut addresses: Vec<_> = addresses.map(|address| address.ip().to_string()).collect();
            addresses.sort();
            addresses.dedup();
            if addresses.is_empty() {
                (
                    Probe::failed("failed", "DNS returned no addresses"),
                    addresses,
                )
            } else {
                (Probe::ok(start, "System DNS resolver"), addresses)
            }
        }
        Ok(Err(error)) => (
            Probe::failed("failed", format!("DNS resolution failed: {error}")),
            Vec::new(),
        ),
        Err(_) => (Probe::failed("timeout", "DNS timed out"), Vec::new()),
    }
}

async fn tcp(host: &str, port: u16) -> Probe {
    let start = Instant::now();
    match timeout(PROBE_TIMEOUT, TcpStream::connect((host, port))).await {
        Ok(Ok(_)) => Probe::ok(start, format!("TCP port {port} (includes DNS)")),
        Ok(Err(error)) => Probe::failed("failed", format!("TCP connection failed: {error}")),
        Err(_) => Probe::failed("timeout", "TCP connection timed out"),
    }
}

async fn ping(address: Option<&String>) -> Probe {
    let Some(address) = address else {
        return Probe::failed("unavailable", "No resolved address to ping");
    };
    let mut command = Command::new("ping");
    if cfg!(target_os = "windows") {
        command.args(["-n", "1", "-w", "2000"]);
    } else {
        command.args(["-n", "-c", "1"]);
    }
    command.arg(address).env("LC_ALL", "C").kill_on_drop(true);
    match timeout(PROBE_TIMEOUT, command.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let latency = ping_latency(&text);
            Probe {
                status: "reachable",
                latency_ms: latency,
                detail: if text.to_ascii_lowercase().contains("time<1") {
                    format!("ICMP to {address}: less than 1 ms")
                } else {
                    format!("ICMP to {address}")
                },
                http_status: None,
            }
        }
        Ok(Ok(_)) => Probe::failed("unavailable", "ICMP blocked, unavailable, or no reply"),
        Ok(Err(_)) => Probe::failed("unavailable", "ping executable unavailable"),
        Err(_) => Probe::failed("timeout", "No ICMP reply within 4 seconds"),
    }
}

fn ping_latency(output: &str) -> Option<f64> {
    let lower = output.to_ascii_lowercase();
    let rest = lower.split_once("time=").map(|(_, rest)| rest)?;
    rest.trim_start()
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()?
        .parse()
        .ok()
}

async fn http(url: &str, proxy: Option<&str>, direct: bool) -> Probe {
    let start = Instant::now();
    let mut builder = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!(
            "openduck-network-monitor/",
            env!("CARGO_PKG_VERSION")
        ));
    if direct {
        builder = builder.no_proxy();
    } else if let Some(proxy) = proxy {
        let Ok(proxy) = reqwest::Proxy::all(proxy.trim()) else {
            return Probe::failed("failed", "Invalid proxy URL");
        };
        builder = builder.proxy(proxy);
    }
    let Ok(client) = builder.build() else {
        return Probe::failed("failed", "Could not initialize HTTP client");
    };
    // Network checks never send provider credentials or configured query strings.
    let Ok(mut url) = Url::parse(url) else {
        return Probe::failed("failed", "Invalid endpoint URL");
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    match client.head(url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let mut probe = Probe::ok(start, format!("HTTP {status}"));
            probe.http_status = Some(status);
            if status == 407 {
                probe.status = "failed";
                probe.detail = "HTTP 407: proxy authentication required".to_string();
            }
            probe
        }
        Err(error) => Probe::failed(
            if error.is_timeout() {
                "timeout"
            } else {
                "failed"
            },
            if error.is_timeout() {
                "HTTP request timed out"
            } else if error.is_connect() {
                "Connection, proxy handshake, or TLS failed"
            } else {
                "HTTP transport failed"
            },
        ),
    }
}

async fn check_endpoint(endpoint: Endpoint) -> EndpointStatus {
    let parsed = Url::parse(&endpoint.url).ok();
    let host = parsed
        .as_ref()
        .and_then(Url::host_str)
        .unwrap_or("")
        .trim_matches(['[', ']']);
    let port = parsed
        .as_ref()
        .and_then(Url::port_or_known_default)
        .unwrap_or(443);
    let dns_check = dns(host, port);
    let tcp_check = tcp(host, port);
    let direct_check = http(&endpoint.url, None, true);
    let provider_check = http(&endpoint.url, endpoint.proxy.as_deref(), false);
    let ((dns, addresses), tcp, direct_http, provider_http) =
        tokio::join!(dns_check, tcp_check, direct_check, provider_check);
    let ping_address = addresses
        .iter()
        .find(|address| !address.contains(':'))
        .or_else(|| addresses.first());
    let ping = ping(ping_address).await;
    EndpointStatus {
        url: public_url(&endpoint.url),
        proxy: endpoint.proxy.as_deref().map(public_url),
        provider: endpoint.provider,
        dns,
        addresses,
        ping,
        tcp,
        direct_http,
        provider_http,
    }
}

async fn check_proxy(source: String, url: String, target: String) -> ProxyStatus {
    let parsed = Url::parse(url.trim()).ok();
    let host = parsed
        .as_ref()
        .and_then(Url::host_str)
        .unwrap_or("")
        .trim_matches(['[', ']']);
    let port = parsed
        .as_ref()
        .and_then(Url::port_or_known_default)
        .unwrap_or(1080);
    let (tcp, tunnel) = tokio::join!(tcp(host, port), http(&target, Some(&url), false));
    ProxyStatus {
        source,
        url: public_url(&url),
        tcp,
        tunnel,
        target: public_url(&target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn proxy_and_endpoint_urls_hide_credentials() {
        assert_eq!(
            public_url("https://user:password@proxy.example:8443/v1?key=secret#token"),
            "https://proxy.example:8443/v1"
        );
        assert_eq!(public_url("not-a-url-password"), "Invalid URL");
        assert_eq!(
            public_url("socks5h://user:password@[::1]:1080"),
            "socks5h://[::1]:1080"
        );
    }

    #[test]
    fn ping_latency_uses_packet_time_instead_of_command_duration() {
        assert_eq!(ping_latency("64 bytes: time=12.3 ms"), Some(12.3));
        assert_eq!(ping_latency("Reply from 1.1.1.1: time=12ms"), Some(12.0));
        assert_eq!(ping_latency("100% packet loss"), None);
        assert_eq!(ping_latency("Reply: time<1ms"), None);
    }

    async fn serve_status(code: u16) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let size = socket.read(&mut bytes).await.unwrap();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 {code} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            String::from_utf8_lossy(&bytes[..size]).into_owned()
        });
        (format!("http://{address}"), server)
    }

    #[tokio::test]
    async fn unauthorized_http_still_confirms_transport_and_strips_secrets() {
        let (url, server) = serve_status(403).await;
        let url = url.replace("http://", "http://user:password@");
        let probe = http(&format!("{url}/v1?key=secret"), None, true).await;
        assert_eq!(probe.status, "reachable");
        assert_eq!(probe.http_status, Some(403));
        assert!(probe.latency_ms.is_some());
        let request = server.await.unwrap();
        assert!(request.starts_with("HEAD /v1 HTTP/1.1"));
        assert!(!request.contains("secret"));
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
    }

    #[tokio::test]
    async fn proxy_authentication_failure_is_not_healthy() {
        let (url, server) = serve_status(407).await;
        let probe = http("http://provider.example/v1", Some(&url), false).await;
        assert_eq!(probe.status, "failed");
        assert_eq!(probe.http_status, Some(407));
        assert!(server.await.unwrap().contains("provider.example"));
    }

    #[tokio::test]
    async fn invalid_proxy_does_not_fall_back_to_direct_network() {
        let probe = http("http://127.0.0.1:1", Some("http://["), false).await;
        assert_eq!(probe.status, "failed");
        assert_eq!(probe.detail, "Invalid proxy URL");
    }
}
