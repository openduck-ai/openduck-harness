use std::time::Duration;

use openduck_providers::api_client::{ApiClient, AuthMethod};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const ORIGIN: &str = "http://proxy-target.invalid";
const EXPECTED_REQUEST_LINE: &str = "POST http://proxy-target.invalid/responses HTTP/1.1";
const TIMEOUT: Duration = Duration::from_secs(5);

struct CapturedRequest {
    request_line: String,
    headers: Vec<(String, String)>,
}

fn http_response(content_type: &str, body: &str) -> Vec<u8> {
    let mut bytes = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name == name)
        .map(|(_, value)| value.as_str())
}

async fn read_captured(socket: &mut TcpStream) -> CapturedRequest {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = tokio::time::timeout(TIMEOUT, socket.read(&mut chunk))
            .await
            .expect("proxy read timed out")
            .expect("proxy read");
        assert_ne!(read, 0, "proxy connection closed before headers");
        buf.extend_from_slice(&chunk[..read]);
        if let Some(end) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
    };

    let head = std::str::from_utf8(&buf[..header_end]).expect("request headers are utf-8");
    let mut lines = head.split("\r\n");
    let request_line = lines.next().expect("request line").to_string();
    let headers = lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (name, value) = line.split_once(':').expect("header field");
            (name.trim().to_ascii_lowercase(), value.trim().to_string())
        })
        .collect();
    CapturedRequest {
        request_line,
        headers,
    }
}

async fn accept_and_reply(listener: TcpListener, response: Vec<u8>) -> CapturedRequest {
    let (mut socket, _) = tokio::time::timeout(TIMEOUT, listener.accept())
        .await
        .expect("proxy accept timed out")
        .expect("proxy accept");
    let captured = read_captured(&mut socket).await;
    tokio::time::timeout(TIMEOUT, socket.write_all(&response))
        .await
        .expect("proxy write timed out")
        .expect("proxy write");
    let _ = socket.shutdown().await;
    captured
}

async fn bind_proxy() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind proxy");
    let proxy_url = format!("http://{}", listener.local_addr().expect("proxy addr"));
    (listener, proxy_url)
}

fn proxied_client(proxy_url: String) -> ApiClient {
    ApiClient::new_with_tls(ORIGIN.to_string(), AuthMethod::NoAuth, None)
        .unwrap()
        .with_proxy(Some(proxy_url))
        .unwrap()
}

#[tokio::test]
async fn explicit_proxy_non_streaming_uses_absolute_form() {
    let (listener, proxy_url) = bind_proxy().await;
    let body = r#"{"ok":true}"#;
    let client = proxied_client(proxy_url);
    let (response, captured) = tokio::join!(
        async {
            tokio::time::timeout(TIMEOUT, client.response_post("responses", &json!({"n": 1})))
                .await
                .expect("client timed out")
                .expect("response")
        },
        accept_and_reply(listener, http_response("application/json", body)),
    );

    assert_eq!(captured.request_line, EXPECTED_REQUEST_LINE);
    let parsed: Value = response.json().await.expect("json body");
    assert_eq!(parsed, json!({"ok": true}));
}

#[tokio::test]
async fn explicit_proxy_streaming_uses_absolute_form() {
    let (listener, proxy_url) = bind_proxy().await;
    let body = "data: {\"ok\":true}\n\n";
    let client = proxied_client(proxy_url);
    let (response, captured) = tokio::join!(
        async {
            tokio::time::timeout(
                TIMEOUT,
                client
                    .request("responses")
                    .streaming(true)
                    .response_post(&json!({"stream": true})),
            )
            .await
            .expect("client timed out")
            .expect("response")
        },
        accept_and_reply(listener, http_response("text/event-stream", body)),
    );

    assert_eq!(captured.request_line, EXPECTED_REQUEST_LINE);
    let payload = response.text().await.expect("sse body");
    assert_eq!(payload, body);
}

#[tokio::test]
async fn proxy_survives_header_rebuild() {
    let (listener, proxy_url) = bind_proxy().await;
    let body = r#"{"ok":true}"#;
    let client = proxied_client(proxy_url)
        .with_header("x-test", "value")
        .unwrap()
        .with_header("x-test-2", "v2")
        .unwrap();
    let (response, captured) = tokio::join!(
        async {
            tokio::time::timeout(TIMEOUT, client.response_post("responses", &json!({"n": 1})))
                .await
                .expect("client timed out")
                .expect("response")
        },
        accept_and_reply(listener, http_response("application/json", body)),
    );

    assert_eq!(captured.request_line, EXPECTED_REQUEST_LINE);
    assert_eq!(header_value(&captured.headers, "x-test"), Some("value"));
    assert_eq!(header_value(&captured.headers, "x-test-2"), Some("v2"));
    let parsed: Value = response.json().await.expect("json body");
    assert_eq!(parsed, json!({"ok": true}));
}
