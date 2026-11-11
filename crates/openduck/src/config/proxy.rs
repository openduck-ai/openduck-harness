use std::fmt;

use crate::config::ConfigError;

#[derive(Clone)]
pub struct ProxyUrl {
    url: url::Url,
}

impl ProxyUrl {
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }

    pub fn url_with_credentials(&self) -> &str {
        self.url.as_str()
    }

    pub fn host(&self) -> &str {
        self.url.host_str().unwrap()
    }

    pub fn port(&self) -> Option<u16> {
        self.url.port()
    }
}

impl fmt::Display for ProxyUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&redact_proxy_url(self.as_str()))
    }
}

impl fmt::Debug for ProxyUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ProxyUrl({})", redact_proxy_url(self.as_str()))
    }
}

pub fn parse_proxy_url(key: &str, raw: &str) -> Result<Option<ProxyUrl>, ConfigError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(None);
    }
    match parse_http_proxy(trimmed) {
        Some(url) => Ok(Some(ProxyUrl { url })),
        None => Err(ConfigError::DeserializeError(invalid_proxy_message(
            key, raw,
        ))),
    }
}

pub fn redact_proxy_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    if trimmed == "null" {
        return "null".to_string();
    }
    match redacted_origin(trimmed) {
        Some(origin) => origin,
        None => "<redacted>".to_string(),
    }
}

fn invalid_proxy_message(key: &str, raw: &str) -> String {
    format!(
        "Invalid config key `{key}` value `{}`: expected an http:// or https:// proxy URL with a host, optional port (0..=65535) and userinfo, root path only, and no query or fragment; use empty string or null to disable",
        redact_proxy_url(raw)
    )
}

fn parse_http_proxy(trimmed: &str) -> Option<url::Url> {
    if contains_forbidden(trimmed) {
        return None;
    }
    let (scheme, rest) = split_scheme(trimmed)?;
    if !is_http_scheme(scheme) {
        return None;
    }
    let (authority, suffix) = split_authority(rest);
    if authority.is_empty() || !is_root_suffix(suffix) {
        return None;
    }
    if !host_port_syntax_ok(strip_userinfo(authority)) {
        return None;
    }
    let url = url::Url::parse(trimmed).ok()?;
    if !is_http_scheme(url.scheme()) {
        return None;
    }
    if url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    if url.query().is_some() || url.fragment().is_some() {
        return None;
    }
    let path = url.path();
    if path != "/" && !path.is_empty() {
        return None;
    }
    Some(url)
}

fn redacted_origin(trimmed: &str) -> Option<String> {
    if contains_forbidden(trimmed) {
        return None;
    }
    let (scheme, rest) = split_scheme(trimmed)?;
    let (authority, _) = split_authority(rest);
    if authority.is_empty() {
        return None;
    }
    let hostport = strip_userinfo(authority);
    if !host_port_syntax_ok(hostport) {
        return None;
    }
    let candidate = url::Url::parse(&format!("http://{hostport}/")).ok()?;
    let host = candidate.host_str().filter(|host| !host.is_empty())?;
    let scheme = scheme.to_ascii_lowercase();
    match candidate.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}")),
        None => Some(format!("{scheme}://{host}")),
    }
}

fn contains_forbidden(value: &str) -> bool {
    value
        .chars()
        .any(|ch| ch == '\\' || ch.is_whitespace() || ch.is_control())
}

fn split_scheme(value: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = value.split_once("://")?;
    if !is_scheme_token(scheme) {
        return None;
    }
    Some((scheme, rest))
}

fn is_scheme_token(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '.' | '-'))
}

fn is_http_scheme(scheme: &str) -> bool {
    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
}

fn split_authority(rest: &str) -> (&str, &str) {
    match rest.find(['/', '?', '#']) {
        Some(index) => rest.split_at(index),
        None => (rest, ""),
    }
}

fn is_root_suffix(suffix: &str) -> bool {
    suffix.is_empty() || suffix == "/"
}

fn strip_userinfo(authority: &str) -> &str {
    match authority.rfind('@') {
        Some(index) => authority.split_at(index + 1).1,
        None => authority,
    }
}

fn host_port_syntax_ok(hostport: &str) -> bool {
    if hostport.is_empty() {
        return false;
    }
    if let Some(inside) = hostport.strip_prefix('[') {
        let Some(end) = inside.find(']') else {
            return false;
        };
        if end == 0 {
            return false;
        }
        let after = inside.split_at(end + 1).1;
        if after.is_empty() {
            return true;
        }
        let Some(port) = after.strip_prefix(':') else {
            return false;
        };
        return is_port(port);
    }
    match hostport.split_once(':') {
        Some((host, port)) => !host.is_empty() && is_port(port),
        None => true,
    }
}

fn is_port(port: &str) -> bool {
    !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
        && port.parse::<u16>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::{parse_proxy_url, redact_proxy_url, ProxyUrl};
    use crate::config::ConfigError;

    const EXPECTATION: &str = "expected an http:// or https:// proxy URL with a host, optional port (0..=65535) and userinfo, root path only, and no query or fragment; use empty string or null to disable";

    fn error_payload(raw: &str) -> String {
        match parse_proxy_url("proxy", raw) {
            Err(ConfigError::DeserializeError(message)) => message,
            Err(error) => panic!("expected DeserializeError for {raw:?}: {error}"),
            Ok(value) => panic!("expected error for {raw:?}, got {value:?}"),
        }
    }

    fn expected_payload(raw: &str) -> String {
        format!(
            "Invalid config key `proxy` value `{}`: {EXPECTATION}",
            redact_proxy_url(raw)
        )
    }

    fn expect_proxy(raw: &str) -> ProxyUrl {
        match parse_proxy_url("proxy", raw) {
            Ok(Some(proxy)) => proxy,
            Ok(None) => panic!("expected proxy for {raw:?}"),
            Err(error) => panic!("expected proxy for {raw:?}: {error}"),
        }
    }

    fn assert_rejected(raw: &str) {
        assert_eq!(error_payload(raw), expected_payload(raw), "{raw}");
    }

    #[test]
    fn empty_string() {
        for raw in ["", " ", "\t", "\n", " \t\r\n ", "\u{00a0}"] {
            assert!(matches!(parse_proxy_url("proxy", raw), Ok(None)), "{raw:?}");
        }
    }

    #[test]
    fn null() {
        for raw in ["null", " null", "null ", "\tnull\n", " \tnull\r\n "] {
            assert!(matches!(parse_proxy_url("proxy", raw), Ok(None)), "{raw:?}");
        }
        for raw in ["NULL", "Null", "nUll"] {
            assert_rejected(raw);
            assert!(error_payload(raw).contains("value `<redacted>`"), "{raw}");
            assert!(!error_payload(raw).contains(raw), "{raw}");
        }
    }

    #[test]
    fn valid_http() {
        for raw in ["http://127.0.0.1:3128", "  http://127.0.0.1:3128  "] {
            let proxy = expect_proxy(raw);
            assert_eq!(proxy.host(), "127.0.0.1");
            assert_eq!(proxy.port(), Some(3128));
            assert_eq!(proxy.as_str(), "http://127.0.0.1:3128/");
            assert_eq!(proxy.url_with_credentials(), proxy.as_str());
        }
    }

    #[test]
    fn valid_https_credentials() {
        let proxy = expect_proxy("https://alice:secret@host:8443");
        assert_eq!(proxy.host(), "host");
        assert_eq!(proxy.port(), Some(8443));
        assert_eq!(proxy.as_str(), "https://alice:secret@host:8443/");
        assert_eq!(
            proxy.url_with_credentials(),
            "https://alice:secret@host:8443/"
        );
        assert_eq!(proxy.to_string(), "https://host:8443");
        assert!(!proxy.to_string().contains("alice"));
        assert!(!proxy.to_string().contains("secret"));
    }

    #[test]
    fn scheme_case() {
        let proxy = expect_proxy("HtTpS://HoSt");
        assert_eq!(proxy.as_str(), "https://host/");
        assert_eq!(proxy.host(), "host");
        assert_eq!(proxy.port(), None);
        assert_eq!(redact_proxy_url("HtTpS://HoSt"), "https://host");
    }

    #[test]
    fn invalid_scheme() {
        assert_eq!(
            error_payload("socks5://host"),
            "Invalid config key `proxy` value `socks5://host`: expected an http:// or https:// proxy URL with a host, optional port (0..=65535) and userinfo, root path only, and no query or fragment; use empty string or null to disable"
        );
        assert_rejected("socks5://127.0.0.1:1080");
        assert_rejected("file:///tmp/proxy");
        assert_rejected("file://localhost/tmp");
        assert!(error_payload("file:///tmp/proxy").contains("value `<redacted>`"));
        assert!(error_payload("socks5://127.0.0.1:1080").contains("socks5://127.0.0.1:1080"));

        match parse_proxy_url("custom_key", "file://host/tmp") {
            Err(ConfigError::DeserializeError(message)) => {
                assert_eq!(
                    message,
                    format!("Invalid config key `custom_key` value `file://host`: {EXPECTATION}")
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn missing_host() {
        for raw in ["http://", "http:///path", "http://user:pass@", "https://"] {
            assert_rejected(raw);
            assert!(error_payload(raw).contains("value `<redacted>`"), "{raw}");
            assert!(!error_payload(raw).contains("user:pass"), "{raw}");
        }
    }

    #[test]
    fn missing_scheme() {
        let raw = "127.0.0.1:3128";
        assert_rejected(raw);
        assert!(error_payload(raw).contains("value `<redacted>`"));
        assert!(!error_payload(raw).contains("127.0.0.1"));
    }

    #[test]
    fn invalid_port() {
        for raw in [
            "http://host:bad",
            "http://host:65536",
            "http://host:-1",
            "http://host:",
            "http://[::1]:bad",
            "http://[::1]:",
            "http://[::1]:65536",
            "https://host:99999",
        ] {
            assert_rejected(raw);
            assert!(error_payload(raw).contains("value `<redacted>`"), "{raw}");
        }
        let zero = expect_proxy("http://host:0");
        assert_eq!(zero.port(), Some(0));
        assert_eq!(zero.as_str(), "http://host:0/");
        let max = expect_proxy("https://host:65535");
        assert_eq!(max.port(), Some(65535));
        assert_eq!(max.as_str(), "https://host:65535/");
    }

    #[test]
    fn endpoint_components() {
        assert_eq!(expect_proxy("http://host").as_str(), "http://host/");
        assert_eq!(expect_proxy("http://host/").as_str(), "http://host/");
        assert_eq!(expect_proxy("https://host/").as_str(), "https://host/");

        let normalized = url::Url::parse("http://host/a/..").unwrap();
        assert_eq!(
            normalized.path(),
            "/",
            "normalized /a/.. must not be accepted"
        );

        for raw in [
            "http://host/a",
            "http://host/a/..",
            "http://host/%61",
            "http://host?",
            "http://host#",
            "http://host/?",
            "http://host/#",
            "http://host?q=1",
            "http://host#frag",
            "http://host/..",
            "https://host/a/../",
            "http://host//",
        ] {
            assert_rejected(raw);
            let message = error_payload(raw);
            assert!(
                message.contains("value `http://host`") || message.contains("value `https://host`"),
                "{raw}: {message}"
            );
        }
    }

    #[test]
    fn username_only() {
        let proxy = expect_proxy("http://alice@host");
        assert_eq!(proxy.host(), "host");
        assert_eq!(proxy.port(), None);
        assert_eq!(proxy.as_str(), "http://alice@host/");
        assert_eq!(proxy.url_with_credentials(), proxy.as_str());
        assert_eq!(proxy.to_string(), "http://host");
        assert_eq!(format!("{proxy:?}"), "ProxyUrl(http://host)");
        assert!(!proxy.to_string().contains("alice"));
        assert!(!format!("{proxy:?}").contains("alice"));

        let message = error_payload("http://alice@host/a");
        assert_eq!(message, expected_payload("http://alice@host/a"));
        assert!(message.contains("value `http://host`"));
        assert!(!message.contains("alice"));

        let empty_password = expect_proxy("http://alice:@host");
        assert_eq!(empty_password.to_string(), "http://host");
        assert!(empty_password.as_str().contains("alice"));
        assert!(!empty_password.to_string().contains("alice"));

        let empty_user = expect_proxy("http://:secret@host");
        assert_eq!(empty_user.host(), "host");
        assert!(empty_user.url_with_credentials().contains("secret"));
        assert!(!empty_user.to_string().contains("secret"));
        assert!(!format!("{empty_user:?}").contains("secret"));
    }

    #[test]
    fn ipv6_default_port() {
        let proxy = expect_proxy("http://[::1]");
        assert_eq!(proxy.host(), "[::1]");
        assert_eq!(proxy.port(), None);
        assert_eq!(proxy.as_str(), "http://[::1]/");

        let custom = expect_proxy("http://[2001:db8::2]:8443");
        assert_eq!(custom.host(), "[2001:db8::2]");
        assert_eq!(custom.port(), Some(8443));
        assert_eq!(custom.as_str(), "http://[2001:db8::2]:8443/");

        let http_default = expect_proxy("http://[::1]:80");
        assert_eq!(http_default.host(), "[::1]");
        assert_eq!(http_default.port(), None);
        assert_eq!(http_default.as_str(), "http://[::1]/");

        let https_default = expect_proxy("https://[::1]:443");
        assert_eq!(https_default.host(), "[::1]");
        assert_eq!(https_default.port(), None);
        assert_eq!(https_default.as_str(), "https://[::1]/");

        let http_host_default = expect_proxy("http://127.0.0.1:80");
        assert_eq!(http_host_default.port(), None);

        for raw in [
            "http://::1",
            "http://2001:db8::1",
            "http://::1:8080",
            "http://[::1]extra",
            "http://[::1]extra:80",
        ] {
            assert_rejected(raw);
        }
    }

    #[test]
    fn credential_redaction_error() {
        const MARKER: &str = "XyZ_CRED_MARKER_9f3a";
        let invalid = format!("http://{MARKER}:{MARKER}@host:bad/path?q={MARKER}#{MARKER}");
        let payload = error_payload(&invalid);
        let shown = parse_proxy_url("proxy", &invalid).unwrap_err().to_string();
        assert_eq!(payload, expected_payload(&invalid));
        assert!(payload.contains("value `<redacted>`"));
        assert!(!payload.contains(MARKER), "{payload}");
        assert!(!shown.contains(MARKER), "{shown}");

        let routed = format!("https://{MARKER}:{MARKER}@host:8443/a?q={MARKER}#{MARKER}");
        let routed_payload = error_payload(&routed);
        assert!(routed_payload.contains("value `https://host:8443`"));
        assert!(!routed_payload.contains(MARKER), "{routed_payload}");
        assert!(!parse_proxy_url("proxy", &routed)
            .unwrap_err()
            .to_string()
            .contains(MARKER));

        let proxy = expect_proxy(&format!("https://{MARKER}:{MARKER}@host:8443"));
        let display = proxy.to_string();
        let debug = format!("{proxy:?}");
        assert_eq!(display, "https://host:8443");
        assert_eq!(debug, "ProxyUrl(https://host:8443)");
        assert!(!display.contains(MARKER));
        assert!(!debug.contains(MARKER));
        assert!(proxy.as_str().contains(MARKER));
        assert!(proxy.url_with_credentials().contains(MARKER));
        assert_eq!(proxy.as_str(), proxy.url_with_credentials());
    }

    #[test]
    fn redaction_fallbacks() {
        assert_eq!(redact_proxy_url(""), "<empty>");
        assert_eq!(redact_proxy_url("   "), "<empty>");
        assert_eq!(redact_proxy_url("null"), "null");
        assert_eq!(redact_proxy_url("  null  "), "null");
        assert_eq!(redact_proxy_url("NULL"), "<redacted>");
        assert_eq!(
            redact_proxy_url("https://alice:secret@host:8443/a?q=secret#secret"),
            "https://host:8443"
        );
        assert_eq!(redact_proxy_url("http://a:b@c:d@host:bad"), "<redacted>");
        assert_eq!(redact_proxy_url("http://a:b@c:d@host"), "http://host");
        assert_eq!(redact_proxy_url("127.0.0.1:3128"), "<redacted>");
        assert_eq!(redact_proxy_url("user:secret@127.0.0.1:3128"), "<redacted>");
        assert_eq!(
            redact_proxy_url("http://a:b@c:d@host:3128/x?y=secret#z"),
            "http://host:3128"
        );
        assert_eq!(
            redact_proxy_url("socks5://alice:secret@host:1080/path?q=secret#secret"),
            "socks5://host:1080"
        );
        assert_eq!(
            redact_proxy_url("file://alice:secret@host/tmp?q=secret"),
            "file://host"
        );
        assert_eq!(redact_proxy_url("file:///tmp/secret"), "<redacted>");
        assert_eq!(
            redact_proxy_url("https://alice:secret@[::1]:8443/a?q=secret#secret"),
            "https://[::1]:8443"
        );
        assert_eq!(redact_proxy_url("http://host\nbad"), "<redacted>");
        assert_eq!(redact_proxy_url("http://ho\tst"), "<redacted>");
        assert_eq!(redact_proxy_url("http://ho st"), "<redacted>");
        assert_eq!(redact_proxy_url("http://host\\evil"), "<redacted>");
        assert_eq!(redact_proxy_url("http://host\u{0001}x"), "<redacted>");
        assert_eq!(redact_proxy_url("http://host:80"), "http://host");
        assert_eq!(redact_proxy_url("https://host:80/secret"), "https://host");
        assert_eq!(redact_proxy_url("https://host:443"), "https://host:443");

        let secret = "query-secret-token";
        let redacted = redact_proxy_url(&format!(
            "https://alice:{secret}@host:8443/a?q={secret}#{secret}"
        ));
        assert_eq!(redacted, "https://host:8443");
        assert!(!redacted.contains(secret));
        assert!(!redacted.contains("alice"));
        assert!(!redacted.contains('?'));
        assert!(!redacted.contains('#'));
        assert!(!redacted.contains('@'));
    }

    #[test]
    fn debug_redaction() {
        let proxy = expect_proxy("https://alice:secret@host:8443");
        assert_eq!(proxy.to_string(), "https://host:8443");
        assert_eq!(format!("{proxy:?}"), "ProxyUrl(https://host:8443)");
        assert!(!format!("{proxy:?}").contains("alice"));
        assert!(!format!("{proxy:?}").contains("secret"));
        assert!(!format!("{proxy:?}").contains('@'));

        let plain = expect_proxy("http://127.0.0.1:3128");
        assert_eq!(plain.to_string(), "http://127.0.0.1:3128");
        assert_eq!(format!("{plain:?}"), "ProxyUrl(http://127.0.0.1:3128)");

        let v6 = expect_proxy("http://[::1]:8080");
        assert_eq!(v6.to_string(), "http://[::1]:8080");
        assert_eq!(format!("{v6:?}"), "ProxyUrl(http://[::1]:8080)");
    }
}
