use openduck::config::Config;
use openduck::providers::api_client::{ApiClient, AuthMethod};
use openduck::providers::proxy;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const ORIGIN: &str = "http://proxy-target.invalid";
const REQUEST_LINE: &str = "POST http://proxy-target.invalid/responses HTTP/1.1";

fn isolated_config(yaml: &str) -> (Config, NamedTempFile, NamedTempFile) {
    let config_file = NamedTempFile::new().unwrap();
    let secrets_file = NamedTempFile::new().unwrap();
    fs::write(config_file.path(), yaml).unwrap();
    let config = Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap();
    (config, config_file, secrets_file)
}

fn lock_probe_provider_env() -> env_lock::EnvGuard<'static> {
    env_lock::lock_env([
        ("OPENDUCK_PROVIDER_PROBEPROXY_PROXY", None::<&str>),
        ("GOOSE_PROVIDER_PROBEPROXY_PROXY", None::<&str>),
        ("OPENDUCK_PROBEPROXY_PROXY", None::<&str>),
        ("GOOSE_PROBEPROXY_PROXY", None::<&str>),
        ("PROBEPROXY_PROXY", None::<&str>),
    ])
}

async fn absolute_form_request_line(listener: TcpListener) -> String {
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("proxy accept timed out")
        .expect("proxy accept");
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let header_end = loop {
        if let Some(pos) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            break pos + 4;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "proxy header read timed out");
        let n = tokio::time::timeout(remaining, socket.read(&mut tmp))
            .await
            .expect("proxy header read timed out")
            .expect("proxy read");
        assert!(n > 0, "proxy closed before headers");
        buf.extend_from_slice(&tmp[..n]);
        assert!(buf.len() < 64 * 1024, "proxy headers too large");
    };
    let headers = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let request_line = headers.lines().next().expect("request line").to_string();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let mut received = buf.len() - header_end;
    while received < content_length {
        let n = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut tmp))
            .await
            .expect("proxy body read timed out")
            .expect("proxy body read");
        assert!(n > 0, "proxy closed before body");
        received += n;
    }
    let body = br#"{"ok":true}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    socket
        .write_all(response.as_bytes())
        .await
        .expect("proxy write");
    socket.write_all(body).await.expect("proxy write body");
    let _ = socket.shutdown().await;
    request_line
}

#[tokio::test]
async fn config_proxy_reaches_transport() {
    let _env = env_lock::lock_env([
        ("HTTP_PROXY", Some("http://127.0.0.1:1")),
        ("HTTPS_PROXY", Some("http://127.0.0.1:1")),
        ("ALL_PROXY", Some("http://127.0.0.1:1")),
        ("http_proxy", Some("http://127.0.0.1:1")),
        ("https_proxy", Some("http://127.0.0.1:1")),
        ("all_proxy", Some("http://127.0.0.1:1")),
        ("NO_PROXY", None::<&str>),
        ("no_proxy", None::<&str>),
    ]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = tokio::spawn(absolute_form_request_line(listener));
    let (config, _config_file, _secrets_file) =
        isolated_config(&format!("proxy: \"http://127.0.0.1:{port}\"\n"));
    let proxy = proxy::resolve_global_proxy(&config)
        .unwrap()
        .expect("configured proxy");
    let client = ApiClient::new_with_tls(ORIGIN.to_string(), AuthMethod::NoAuth, None)
        .unwrap()
        .with_proxy(Some(proxy))
        .unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        client.response_post("responses", &serde_json::json!({"n": 1})),
    )
    .await
    .expect("request timed out")
    .expect("request through proxy");
    let parsed: serde_json::Value = response.json().await.expect("response json");
    assert_eq!(parsed["ok"], true);
    let request_line = tokio::time::timeout(Duration::from_secs(5), captured)
        .await
        .expect("proxy task timed out")
        .expect("proxy task");
    assert_eq!(request_line, REQUEST_LINE);
}

#[tokio::test]
async fn config_proxy_absent_leaves_transport_unchanged() {
    let _env = env_lock::lock_env([
        ("HTTP_PROXY", None::<&str>),
        ("HTTPS_PROXY", None::<&str>),
        ("ALL_PROXY", None::<&str>),
        ("http_proxy", None::<&str>),
        ("https_proxy", None::<&str>),
        ("all_proxy", None::<&str>),
        ("NO_PROXY", None::<&str>),
        ("no_proxy", None::<&str>),
    ]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&hit);
    tokio::spawn(async move {
        if listener.accept().await.is_ok() {
            flag.store(true, Ordering::SeqCst);
        }
    });
    let (config, _config_file, _secrets_file) = isolated_config("");
    assert!(proxy::resolve_global_proxy(&config).unwrap().is_none());
    let client = ApiClient::new_with_tls(ORIGIN.to_string(), AuthMethod::NoAuth, None).unwrap();
    assert!(client.proxy().is_none());
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        client.response_post("responses", &serde_json::json!({"n": 1})),
    )
    .await;
    assert!(
        outcome.is_err() || outcome.unwrap().is_err(),
        "direct dial of an invalid origin succeeded"
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!hit.load(Ordering::SeqCst));
}

#[test]
fn invalid_config_proxy_is_an_error_and_redacted() {
    let raw = "http://user:hunter2@proxy.example:notaport";
    let (config, _config_file, _secrets_file) = isolated_config(&format!("proxy: \"{raw}\"\n"));
    let err = proxy::resolve_global_proxy(&config).unwrap_err();
    let rendered = format!("{err:#}");
    assert!(rendered.contains("`proxy`"), "{rendered}");
    assert!(!rendered.contains("hunter2"), "{rendered}");
    assert!(!rendered.contains(raw), "{rendered}");
}

#[test]
fn provider_specific_proxy_still_used_when_global_absent() {
    let _env = lock_probe_provider_env();
    let (config, _config_file, _secrets_file) =
        isolated_config("providers:\n  probeproxy:\n    proxy: \"http://probe-proxy.example:9\"\n");
    assert_eq!(
        proxy::resolve_outbound_proxy(&config, "probeproxy")
            .unwrap()
            .as_deref(),
        Some("http://probe-proxy.example:9")
    );
}

#[test]
fn global_proxy_wins_over_provider_setting() {
    let _env = lock_probe_provider_env();
    let (config, _config_file, _secrets_file) = isolated_config(
        "proxy: \"http://global-proxy.example:9\"\nproviders:\n  probeproxy:\n    proxy: \"http://probe-proxy.example:9\"\n",
    );
    assert_eq!(
        proxy::resolve_outbound_proxy(&config, "probeproxy")
            .unwrap()
            .as_deref(),
        Some("http://global-proxy.example:9/")
    );
}

#[tokio::test]
async fn raw_client_config_proxy_reaches_transport() {
    let _env = env_lock::lock_env([
        ("HTTP_PROXY", Some("http://127.0.0.1:1")),
        ("HTTPS_PROXY", Some("http://127.0.0.1:1")),
        ("ALL_PROXY", Some("http://127.0.0.1:1")),
        ("http_proxy", Some("http://127.0.0.1:1")),
        ("https_proxy", Some("http://127.0.0.1:1")),
        ("all_proxy", Some("http://127.0.0.1:1")),
        ("NO_PROXY", None::<&str>),
        ("no_proxy", None::<&str>),
    ]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (config, _config_file, _secrets_file) =
        isolated_config(&format!("proxy: \"http://127.0.0.1:{port}\"\n"));
    let proxy = proxy::resolve_global_proxy(&config)
        .unwrap()
        .expect("configured proxy");
    let builder = proxy::apply_proxy(reqwest::Client::builder(), Some(proxy.as_str())).unwrap();
    let client = builder.build().unwrap();
    let captured = tokio::spawn(absolute_form_request_line(listener));
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        client
            .post(format!("{ORIGIN}/responses"))
            .json(&serde_json::json!({"n": 1}))
            .send(),
    )
    .await
    .expect("request timed out")
    .expect("request through proxy");
    let parsed: serde_json::Value = response.json().await.expect("response json");
    assert_eq!(parsed["ok"], true);
    let request_line = tokio::time::timeout(Duration::from_secs(5), captured)
        .await
        .expect("proxy task timed out")
        .expect("proxy task");
    assert_eq!(request_line, REQUEST_LINE);
}

#[tokio::test]
async fn streaming_request_traverses_config_proxy() {
    let _env = env_lock::lock_env([
        ("HTTP_PROXY", Some("http://127.0.0.1:1")),
        ("HTTPS_PROXY", Some("http://127.0.0.1:1")),
        ("ALL_PROXY", Some("http://127.0.0.1:1")),
        ("http_proxy", Some("http://127.0.0.1:1")),
        ("https_proxy", Some("http://127.0.0.1:1")),
        ("all_proxy", Some("http://127.0.0.1:1")),
        ("NO_PROXY", None::<&str>),
        ("no_proxy", None::<&str>),
    ]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = tokio::spawn(absolute_form_request_line(listener));
    let (config, _config_file, _secrets_file) =
        isolated_config(&format!("proxy: \"http://127.0.0.1:{port}\"\n"));
    let proxy = proxy::resolve_global_proxy(&config)
        .unwrap()
        .expect("configured proxy");
    let client = ApiClient::new_with_tls(ORIGIN.to_string(), AuthMethod::NoAuth, None)
        .unwrap()
        .with_proxy(Some(proxy))
        .unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        client
            .request("responses")
            .streaming(true)
            .response_post(&serde_json::json!({"stream": true})),
    )
    .await
    .expect("request timed out")
    .expect("request through proxy");
    let parsed: serde_json::Value = response.json().await.expect("response json");
    assert_eq!(parsed["ok"], true);
    let request_line = tokio::time::timeout(Duration::from_secs(5), captured)
        .await
        .expect("proxy task timed out")
        .expect("proxy task");
    assert_eq!(request_line, REQUEST_LINE);
}

#[tokio::test]
async fn absent_proxy_still_honors_environment_fallback() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_uri = format!("http://{}", listener.local_addr().unwrap());
    let _env = env_lock::lock_env([
        ("HTTP_PROXY", Some(proxy_uri.as_str())),
        ("http_proxy", Some(proxy_uri.as_str())),
        ("HTTPS_PROXY", None::<&str>),
        ("https_proxy", None::<&str>),
        ("ALL_PROXY", None::<&str>),
        ("all_proxy", None::<&str>),
        ("NO_PROXY", None::<&str>),
        ("no_proxy", None::<&str>),
    ]);
    let (config, _config_file, _secrets_file) = isolated_config("");
    let proxy = proxy::resolve_global_proxy(&config).unwrap();
    assert!(proxy.is_none());
    let client = ApiClient::new_with_tls(ORIGIN.to_string(), AuthMethod::NoAuth, None)
        .unwrap()
        .with_proxy(proxy)
        .unwrap();
    assert!(client.proxy().is_none());
    let captured = tokio::spawn(absolute_form_request_line(listener));
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        client.response_post("responses", &serde_json::json!({"n": 1})),
    )
    .await
    .expect("request timed out")
    .expect("request through proxy");
    let parsed: serde_json::Value = response.json().await.expect("response json");
    assert_eq!(parsed["ok"], true);
    let request_line = tokio::time::timeout(Duration::from_secs(5), captured)
        .await
        .expect("proxy task timed out")
        .expect("proxy task");
    assert_eq!(request_line, REQUEST_LINE);
}
