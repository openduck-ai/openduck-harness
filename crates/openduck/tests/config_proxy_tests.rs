use env_lock::lock_env;
use openduck::config::Config;
use std::fs;
use std::path::PathBuf;
use tempfile::NamedTempFile;

fn config_from_yaml(yaml: &str) -> (Config, NamedTempFile, NamedTempFile) {
    let config_file = NamedTempFile::new().unwrap();
    let secrets_file = NamedTempFile::new().unwrap();
    fs::write(config_file.path(), yaml).unwrap();
    let config =
        Config::new_with_config_paths(vec![config_file.path().to_path_buf()], secrets_file.path())
            .unwrap();
    (config, config_file, secrets_file)
}

fn config_from_layers(layers: &[&str]) -> (Config, Vec<NamedTempFile>, NamedTempFile) {
    let secrets_file = NamedTempFile::new().unwrap();
    let mut files = Vec::new();
    let mut paths = Vec::new();
    for yaml in layers {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), yaml).unwrap();
        paths.push(file.path().to_path_buf());
        files.push(file);
    }
    let config = Config::new_with_config_paths(paths, secrets_file.path()).unwrap();
    (config, files, secrets_file)
}

#[test]
fn unset() {
    let (config, _cfg, _sec) = config_from_yaml("");
    assert!(config.get_proxy().unwrap().is_none());

    let (config, _cfg, _sec) = config_from_yaml("other_key: some_value\n");
    assert!(config.get_proxy().unwrap().is_none());

    let secrets_file = NamedTempFile::new().unwrap();
    let non_existent = PathBuf::from("/tmp/nonexistent_openduck_config_proxy_test.yaml");
    let config = Config::new_with_config_paths(vec![non_existent], secrets_file.path()).unwrap();
    assert!(config.get_proxy().unwrap().is_none());
}

#[test]
fn empty_string() {
    let (config, _cfg, _sec) = config_from_yaml("proxy: \"\"\n");
    assert!(config.get_proxy().unwrap().is_none());

    let (config, _cfg, _sec) = config_from_yaml("proxy: \"   \"\n");
    assert!(config.get_proxy().unwrap().is_none());
}

#[test]
fn null() {
    let (config, _cfg, _sec) = config_from_yaml("proxy: null\n");
    assert!(config.get_proxy().unwrap().is_none());

    let (config, _cfg, _sec) = config_from_yaml("proxy:\n");
    assert!(config.get_proxy().unwrap().is_none());

    let (config, _cfg, _sec) = config_from_yaml("proxy: \"null\"\n");
    assert!(config.get_proxy().unwrap().is_none());

    let (config, _cfg, _sec) = config_from_yaml("proxy: \"NULL\"\n");
    assert!(config.get_proxy().is_err());

    let (config, _cfg, _sec) = config_from_yaml("proxy: \"Null\"\n");
    assert!(config.get_proxy().is_err());
}

#[test]
fn valid_http() {
    let (config, _cfg, _sec) = config_from_yaml("proxy: http://127.0.0.1:3128\n");
    let proxy = config.get_proxy().unwrap().expect("proxy should be parsed");
    assert_eq!(proxy.host(), "127.0.0.1");
    assert_eq!(proxy.port(), Some(3128));
    assert_eq!(proxy.as_str(), "http://127.0.0.1:3128/");
}

#[test]
fn valid_https_credentials() {
    let (config, _cfg, _sec) =
        config_from_yaml("proxy: https://alice:s3cr3t-marker@host.example:8443\n");
    let proxy = config.get_proxy().unwrap().expect("proxy should be parsed");
    assert_eq!(proxy.host(), "host.example");
    assert_eq!(proxy.port(), Some(8443));

    let display_str = proxy.to_string();
    assert!(!display_str.contains("alice"));
    assert!(!display_str.contains("s3cr3t-marker"));

    let debug_str = format!("{proxy:?}");
    assert!(!debug_str.contains("alice"));
    assert!(!debug_str.contains("s3cr3t-marker"));

    let cred_url = proxy.url_with_credentials();
    assert!(cred_url.contains("alice"));
    assert!(cred_url.contains("s3cr3t-marker"));
}

#[test]
fn invalid_scheme() {
    let (config, _cfg, _sec) = config_from_yaml("proxy: socks5://host:1080\n");
    let err = config.get_proxy().unwrap_err().to_string();
    assert!(err.contains("proxy"));
    assert!(err.contains("expected an http:// or https:// proxy URL"));

    let (config, _cfg, _sec) = config_from_yaml("proxy: file:///tmp/x\n");
    let err = config.get_proxy().unwrap_err().to_string();
    assert!(err.contains("proxy"));
    assert!(err.contains("expected an http:// or https:// proxy URL"));
}

#[test]
fn missing_host() {
    for invalid in &[
        "proxy: http://\n",
        "proxy: http:///path\n",
        "proxy: http://user:pass@\n",
    ] {
        let (config, _cfg, _sec) = config_from_yaml(invalid);
        assert!(config.get_proxy().is_err());
    }
}

#[test]
fn missing_scheme() {
    let (config, _cfg, _sec) = config_from_yaml("proxy: 127.0.0.1:3128\n");
    let err = config.get_proxy().unwrap_err().to_string();
    assert!(err.contains("<redacted>"));
}

#[test]
fn invalid_port() {
    for invalid in &[
        "proxy: http://host:bad\n",
        "proxy: http://host:65536\n",
        "proxy: http://host:-1\n",
        "proxy: http://host:\n",
    ] {
        let (config, _cfg, _sec) = config_from_yaml(invalid);
        assert!(config.get_proxy().is_err(), "expected error for {invalid}");
    }

    let (config, _cfg, _sec) = config_from_yaml("proxy: http://host:0\n");
    let proxy = config.get_proxy().unwrap().unwrap();
    assert_eq!(proxy.port(), Some(0));

    let (config, _cfg, _sec) = config_from_yaml("proxy: http://host:65535\n");
    let proxy = config.get_proxy().unwrap().unwrap();
    assert_eq!(proxy.port(), Some(65535));
}

#[test]
fn credential_redaction_error() {
    for invalid in &[
        "proxy: https://alice:s3cr3t-marker@host:bad\n",
        "proxy: http://alice:s3cr3t-marker@\n",
    ] {
        let (config, _cfg, _sec) = config_from_yaml(invalid);
        let err = config.get_proxy().unwrap_err().to_string();
        assert!(!err.contains("alice"), "error leaked username: {err}");
        assert!(
            !err.contains("s3cr3t-marker"),
            "error leaked credential: {err}"
        );
        assert!(err.contains("proxy"), "error should name key 'proxy'");
    }
}

#[test]
fn non_string_type() {
    for invalid in &[
        "proxy: 3128\n",
        "proxy: [a, b]\n",
        "proxy: true\n",
        "proxy: { host: example }\n",
        "proxy: !custom tagged_value\n",
    ] {
        let (config, _cfg, _sec) = config_from_yaml(invalid);
        let err = config.get_proxy().unwrap_err().to_string();
        assert!(
            err.contains("Invalid config key `proxy`"),
            "expected key in error: {err}"
        );
        assert!(
            err.contains("expected a string proxy URL or null"),
            "expected safe expectation in error: {err}"
        );
    }
}

#[test]
fn broken_yaml() {
    let (config, _cfg, _sec) = config_from_yaml("proxy: [unclosed-marker-s3cr3t: hello\n");
    let err = config.get_proxy().unwrap_err().to_string();
    assert!(
        err.contains("Unable to read config key `proxy`: configuration could not be loaded"),
        "expected safe load error: {err}"
    );
    assert!(
        !err.contains("s3cr3t"),
        "broken YAML error leaked source content: {err}"
    );
}

#[test]
fn layered_precedence() {
    let (config, _files, _sec) = config_from_layers(&[
        "proxy: http://earlier.example:8080\n",
        "proxy: http://later.example:8080\n",
    ]);
    let proxy = config.get_proxy().unwrap().unwrap();
    assert_eq!(proxy.host(), "later.example");
}

#[test]
fn layered_disable() {
    let (config, _files, _sec) =
        config_from_layers(&["proxy: http://earlier.example:8080\n", "proxy: \"\"\n"]);
    assert!(config.get_proxy().unwrap().is_none());

    let (config, _files, _sec) =
        config_from_layers(&["proxy: http://earlier.example:8080\n", "proxy: null\n"]);
    assert!(config.get_proxy().unwrap().is_none());

    let (config, _files, _sec) =
        config_from_layers(&["proxy: null\n", "proxy: http://later.example:8080\n"]);
    let proxy = config.get_proxy().unwrap().unwrap();
    assert_eq!(proxy.host(), "later.example");
}

#[test]
fn env_is_ignored() {
    let _guard = lock_env([
        (
            "OPENDUCK_PROXY",
            Some("http://alice:s3cr3t-marker@env.invalid:9999/"),
        ),
        (
            "GOOSE_PROXY",
            Some("http://alice:s3cr3t-marker@env.invalid:9999/"),
        ),
        (
            "PROXY",
            Some("http://alice:s3cr3t-marker@env.invalid:9999/"),
        ),
        (
            "HTTP_PROXY",
            Some("http://alice:s3cr3t-marker@env.invalid:9999/"),
        ),
        (
            "HTTPS_PROXY",
            Some("http://alice:s3cr3t-marker@env.invalid:9999/"),
        ),
        (
            "ALL_PROXY",
            Some("http://alice:s3cr3t-marker@env.invalid:9999/"),
        ),
    ]);

    let (config, _cfg, _sec) = config_from_yaml("");
    assert!(config.get_proxy().unwrap().is_none());

    let (config, _cfg, _sec) = config_from_yaml("proxy: http://file.example:3128\n");
    let proxy = config.get_proxy().unwrap().unwrap();
    assert_eq!(proxy.host(), "file.example");
    assert_eq!(proxy.port(), Some(3128));
}

#[test]
fn debug_redaction() {
    let (config, _cfg, _sec) =
        config_from_yaml("proxy: https://alice:s3cr3t-marker@host.example:8443\n");
    let proxy = config.get_proxy().unwrap().unwrap();
    assert_eq!(proxy.to_string(), "https://host.example:8443");
    assert_eq!(format!("{proxy:?}"), "ProxyUrl(https://host.example:8443)");
}

#[test]
fn documented_example_config_parses() {
    let yaml = r#"
proxy: "http://127.0.0.1:3128"
active_provider: openai
providers:
  openai:
    model: gpt-4o
extensions:
  developer:
    enabled: true
harness:
  max_turns: 25
"#;
    let (config, _cfg, _sec) = config_from_yaml(yaml);
    let proxy = config.get_proxy().unwrap().expect("proxy should be parsed");
    assert_eq!(proxy.host(), "127.0.0.1");
    assert_eq!(proxy.port(), Some(3128));
}

#[test]
fn documented_disable_forms_yield_none() {
    let yaml_empty = r#"
proxy: ""
active_provider: openai
providers:
  openai:
    model: gpt-4o
extensions:
  developer:
    enabled: true
harness:
  max_turns: 25
"#;
    let (config, _cfg, _sec) = config_from_yaml(yaml_empty);
    assert!(config.get_proxy().unwrap().is_none());

    let yaml_null = r#"
proxy: null
active_provider: openai
providers:
  openai:
    model: gpt-4o
extensions:
  developer:
    enabled: true
harness:
  max_turns: 25
"#;
    let (config, _cfg, _sec) = config_from_yaml(yaml_null);
    assert!(config.get_proxy().unwrap().is_none());
}

#[test]
fn display_and_debug_redact_credentials() {
    let (config, _cfg, _sec) =
        config_from_yaml("proxy: https://alice:s3cr3t-marker@host.example:8443\n");
    let proxy = config.get_proxy().unwrap().unwrap();
    assert_eq!(proxy.to_string(), "https://host.example:8443");
    assert_eq!(format!("{proxy:?}"), "ProxyUrl(https://host.example:8443)");
    assert!(!proxy.to_string().contains("alice"));
    assert!(!proxy.to_string().contains("s3cr3t-marker"));
    assert!(!format!("{proxy:?}").contains("alice"));
    assert!(!format!("{proxy:?}").contains("s3cr3t-marker"));
    let cred_url = proxy.url_with_credentials();
    assert_eq!(cred_url, "https://alice:s3cr3t-marker@host.example:8443/");
    assert!(cred_url.contains("alice"));
    assert!(cred_url.contains("s3cr3t-marker"));
}
