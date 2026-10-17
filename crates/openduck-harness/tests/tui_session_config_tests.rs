use openduck_harness::{
    discover_global_harness_settings, load_harness_global_settings_file,
    parse_harness_global_settings_str, HarnessGlobalSettings, TuiSessionDefaults,
};
use tempfile::tempdir;

#[test]
fn tui_session_defaults_coexist_with_advisor_settings() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  max_turns: 50
  advisors:
    grok:
      https_proxy: http://advisor:8080
  tui_sessions:
    default:
      https_proxy: http://default:8080
    grok:
      https_proxy: http://127.0.0.1:2087
"#,
    );

    assert_eq!(settings.max_turns, Some(50));
    assert_eq!(
        settings.advisors["grok"].https_proxy.as_deref(),
        Some("http://advisor:8080")
    );
    assert_eq!(
        settings.tui_sessions["grok"].https_proxy.as_deref(),
        Some("http://127.0.0.1:2087")
    );
    assert_eq!(
        settings.tui_sessions["default"].https_proxy.as_deref(),
        Some("http://default:8080")
    );
}

#[test]
fn nested_tui_session_settings_take_precedence_over_top_level_settings() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  tui_sessions:
    grok:
      https_proxy: http://nested:8080
tui_sessions:
  grok:
    https_proxy: http://top-level:8080
  claude:
    https_proxy: http://claude:8080
"#,
    );

    assert_eq!(
        settings.tui_sessions["grok"].https_proxy.as_deref(),
        Some("http://nested:8080")
    );
    assert_eq!(
        settings.tui_sessions["claude"].https_proxy.as_deref(),
        Some("http://claude:8080")
    );
}

#[test]
fn tui_session_parses_idle_timeout_and_exited_retention_fields() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  tui_sessions:
    default:
      https_proxy: http://127.0.0.1:8080
      idle_timeout_mins: 45
      exited_retention_mins: 15
    agy:
      idle_timeout_mins: 60
    codex: {}
"#,
    );

    assert_eq!(
        settings.tui_sessions["default"],
        TuiSessionDefaults {
            https_proxy: Some("http://127.0.0.1:8080".to_string()),
            idle_timeout_mins: Some(45),
            exited_retention_mins: Some(15),
        }
    );
    assert_eq!(
        settings.tui_sessions["agy"],
        TuiSessionDefaults {
            https_proxy: None,
            idle_timeout_mins: Some(60),
            exited_retention_mins: None,
        }
    );
    assert_eq!(
        settings.tui_sessions["codex"],
        TuiSessionDefaults::default()
    );
}

#[test]
fn tui_session_supports_camel_case_aliases() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  tuiSessions:
    default:
      httpsProxy: http://camel-nested:8080
      idleTimeoutMins: 30
      exitedRetentionMins: 10
tuiSessions:
  claude:
    httpsProxy: http://camel-top:8080
    idleTimeoutMins: 20
    exitedRetentionMins: 5
"#,
    );

    assert_eq!(
        settings.tui_sessions["default"],
        TuiSessionDefaults {
            https_proxy: Some("http://camel-nested:8080".to_string()),
            idle_timeout_mins: Some(30),
            exited_retention_mins: Some(10),
        }
    );
    assert_eq!(
        settings.tui_sessions["claude"],
        TuiSessionDefaults {
            https_proxy: Some("http://camel-top:8080".to_string()),
            idle_timeout_mins: Some(20),
            exited_retention_mins: Some(5),
        }
    );
}

#[test]
fn load_harness_global_settings_file_reads_tui_sessions_and_handles_missing_file() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");
    std::fs::write(
        &config_path,
        r#"
harness:
  tui_sessions:
    grok:
      https_proxy: http://file-proxy:2087
      idle_timeout_mins: 25
"#,
    )
    .unwrap();

    let loaded = load_harness_global_settings_file(&config_path);
    assert_eq!(
        loaded.tui_sessions["grok"],
        TuiSessionDefaults {
            https_proxy: Some("http://file-proxy:2087".to_string()),
            idle_timeout_mins: Some(25),
            exited_retention_mins: None,
        }
    );

    let missing = load_harness_global_settings_file(&dir.path().join("nonexistent.yaml"));
    assert!(missing.tui_sessions.is_empty());
}

#[test]
fn discover_global_harness_settings_merges_workspace_configs_and_lowercases_keys() {
    let dir = tempdir().unwrap();
    let openduck_dir = dir.path().join(".openduck");
    let goose_dir = dir.path().join(".goose");
    std::fs::create_dir_all(&openduck_dir).unwrap();
    std::fs::create_dir_all(&goose_dir).unwrap();

    std::fs::write(
        openduck_dir.join("config.yaml"),
        r#"
harness:
  tui_sessions:
    Grok:
      https_proxy: http://openduck-config:8080
      idle_timeout_mins: 40
"#,
    )
    .unwrap();

    std::fs::write(
        goose_dir.join("harness.yaml"),
        r#"
harness:
  tui_sessions:
    grok:
      https_proxy: http://goose-harness:8080
      idle_timeout_mins: 99
    Agy:
      idle_timeout_mins: 55
      exited_retention_mins: 12
"#,
    )
    .unwrap();

    let discovered = discover_global_harness_settings(Some(dir.path()));
    assert_eq!(
        discovered.tui_sessions["grok"],
        TuiSessionDefaults {
            https_proxy: Some("http://openduck-config:8080".to_string()),
            idle_timeout_mins: Some(40),
            exited_retention_mins: None,
        }
    );
    assert_eq!(
        discovered.tui_sessions["agy"],
        TuiSessionDefaults {
            https_proxy: None,
            idle_timeout_mins: Some(55),
            exited_retention_mins: Some(12),
        }
    );
}

#[test]
fn parse_harness_global_settings_str_handles_empty_and_invalid_yaml() {
    let empty = parse_harness_global_settings_str("");
    assert!(empty.tui_sessions.is_empty());

    let invalid = parse_harness_global_settings_str("::: not valid yaml [[[");
    assert!(invalid.tui_sessions.is_empty());
}

#[test]
fn parse_harness_global_settings_str_lowercases_nested_and_top_level_keys() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  tui_sessions:
    Grok:
      https_proxy: http://nested-grok:8080
      idle_timeout_mins: 35
    DEFAULT:
      idle_timeout_mins: 45
tui_sessions:
  grok:
    https_proxy: http://top-grok:8080
    idle_timeout_mins: 99
  AGY:
    exited_retention_mins: 20
"#,
    );

    assert_eq!(settings.tui_sessions.len(), 3);
    assert!(!settings.tui_sessions.contains_key("Grok"));
    assert!(!settings.tui_sessions.contains_key("DEFAULT"));
    assert!(!settings.tui_sessions.contains_key("AGY"));
    assert_eq!(
        settings.tui_sessions["grok"],
        TuiSessionDefaults {
            https_proxy: Some("http://nested-grok:8080".to_string()),
            idle_timeout_mins: Some(35),
            exited_retention_mins: None,
        }
    );
    assert_eq!(
        settings.tui_sessions["default"],
        TuiSessionDefaults {
            https_proxy: None,
            idle_timeout_mins: Some(45),
            exited_retention_mins: None,
        }
    );
    assert_eq!(
        settings.tui_sessions["agy"],
        TuiSessionDefaults {
            https_proxy: None,
            idle_timeout_mins: None,
            exited_retention_mins: Some(20),
        }
    );
}

#[test]
fn tui_session_defaults_yaml_roundtrip_and_skip_serializing_none() {
    let defaults = TuiSessionDefaults {
        https_proxy: None,
        idle_timeout_mins: Some(45),
        exited_retention_mins: None,
    };

    let yaml = serde_yaml::to_string(&defaults).unwrap();
    assert!(!yaml.contains("https_proxy"));
    assert!(!yaml.contains("exited_retention_mins"));
    assert!(yaml.contains("idle_timeout_mins: 45"));

    let deserialized: TuiSessionDefaults = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(deserialized, defaults);

    let mut global = HarnessGlobalSettings::default();
    global.tui_sessions.insert("agy".to_string(), defaults);
    let global_yaml = serde_yaml::to_string(&global).unwrap();
    let parsed_back = parse_harness_global_settings_str(&global_yaml);
    assert_eq!(parsed_back.tui_sessions, global.tui_sessions);
}

#[test]
fn discover_global_harness_settings_precedence_across_standard_workspace_files() {
    let dir = tempdir().unwrap();
    let openduck_dir = dir.path().join(".openduck");
    let goose_dir = dir.path().join(".goose");
    std::fs::create_dir_all(&openduck_dir).unwrap();
    std::fs::create_dir_all(&goose_dir).unwrap();

    std::fs::write(
        openduck_dir.join("config.yaml"),
        r#"
harness:
  tui_sessions:
    default:
      https_proxy: http://openduck-config-default:8080
      idle_timeout_mins: 30
"#,
    )
    .unwrap();

    std::fs::write(
        goose_dir.join("config.yaml"),
        r#"
harness:
  tui_sessions:
    default:
      https_proxy: http://goose-config-default:8080
      idle_timeout_mins: 99
    agy:
      idle_timeout_mins: 60
"#,
    )
    .unwrap();

    std::fs::write(
        openduck_dir.join("harness.yaml"),
        r#"
harness:
  tui_sessions:
    agy:
      idle_timeout_mins: 120
    codex:
      exited_retention_mins: 15
"#,
    )
    .unwrap();

    std::fs::write(
        goose_dir.join("harness.yaml"),
        r#"
harness:
  tui_sessions:
    codex:
      exited_retention_mins: 99
    claude:
      https_proxy: http://goose-harness-claude:8080
"#,
    )
    .unwrap();

    let discovered = discover_global_harness_settings(Some(dir.path()));
    assert_eq!(
        discovered.tui_sessions["default"],
        TuiSessionDefaults {
            https_proxy: Some("http://openduck-config-default:8080".to_string()),
            idle_timeout_mins: Some(30),
            exited_retention_mins: None,
        }
    );
    assert_eq!(
        discovered.tui_sessions["agy"],
        TuiSessionDefaults {
            https_proxy: None,
            idle_timeout_mins: Some(60),
            exited_retention_mins: None,
        }
    );
    assert_eq!(
        discovered.tui_sessions["codex"],
        TuiSessionDefaults {
            https_proxy: None,
            idle_timeout_mins: None,
            exited_retention_mins: Some(15),
        }
    );
    assert_eq!(
        discovered.tui_sessions["claude"],
        TuiSessionDefaults {
            https_proxy: Some("http://goose-harness-claude:8080".to_string()),
            idle_timeout_mins: None,
            exited_retention_mins: None,
        }
    );
}

#[test]
fn parse_harness_global_settings_str_ignores_malformed_tui_sessions_entry_gracefully() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  max_turns: 40
  tui_sessions: "not-a-map"
"#,
    );

    assert_eq!(settings.max_turns, Some(40));
    assert!(settings.tui_sessions.is_empty());
}
