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
            effort: None,
            https_proxy: Some("http://127.0.0.1:8080".to_string()),
            idle_timeout_mins: Some(45),
            exited_retention_mins: Some(15),
            ..Default::default()
        }
    );
    assert_eq!(
        settings.tui_sessions["agy"],
        TuiSessionDefaults {
            effort: None,
            https_proxy: None,
            idle_timeout_mins: Some(60),
            exited_retention_mins: None,
            ..Default::default()
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
            effort: None,
            https_proxy: Some("http://camel-nested:8080".to_string()),
            idle_timeout_mins: Some(30),
            exited_retention_mins: Some(10),
            ..Default::default()
        }
    );
    assert_eq!(
        settings.tui_sessions["claude"],
        TuiSessionDefaults {
            effort: None,
            https_proxy: Some("http://camel-top:8080".to_string()),
            idle_timeout_mins: Some(20),
            exited_retention_mins: Some(5),
            ..Default::default()
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
            effort: None,
            https_proxy: Some("http://file-proxy:2087".to_string()),
            idle_timeout_mins: Some(25),
            exited_retention_mins: None,
            ..Default::default()
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
            effort: None,
            https_proxy: Some("http://openduck-config:8080".to_string()),
            idle_timeout_mins: Some(40),
            exited_retention_mins: None,
            ..Default::default()
        }
    );
    assert_eq!(
        discovered.tui_sessions["agy"],
        TuiSessionDefaults {
            effort: None,
            https_proxy: None,
            idle_timeout_mins: Some(55),
            exited_retention_mins: Some(12),
            ..Default::default()
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
            effort: None,
            https_proxy: Some("http://nested-grok:8080".to_string()),
            idle_timeout_mins: Some(35),
            exited_retention_mins: None,
            ..Default::default()
        }
    );
    assert_eq!(
        settings.tui_sessions["default"],
        TuiSessionDefaults {
            effort: None,
            https_proxy: None,
            idle_timeout_mins: Some(45),
            exited_retention_mins: None,
            ..Default::default()
        }
    );
    assert_eq!(
        settings.tui_sessions["agy"],
        TuiSessionDefaults {
            effort: None,
            https_proxy: None,
            idle_timeout_mins: None,
            exited_retention_mins: Some(20),
            ..Default::default()
        }
    );
}

#[test]
fn tui_session_defaults_yaml_roundtrip_and_skip_serializing_none() {
    let defaults = TuiSessionDefaults {
        effort: None,
        https_proxy: None,
        idle_timeout_mins: Some(45),
        exited_retention_mins: None,
        ..Default::default()
    };

    let yaml = serde_yaml::to_string(&defaults).unwrap();
    assert!(!yaml.contains("effort"));
    assert!(!yaml.contains("https_proxy"));
    assert!(!yaml.contains("exited_retention_mins"));
    assert!(!yaml.contains("model"));
    assert!(!yaml.contains("fallback_models"));
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
            effort: None,
            https_proxy: Some("http://openduck-config-default:8080".to_string()),
            idle_timeout_mins: Some(30),
            exited_retention_mins: None,
            ..Default::default()
        }
    );
    assert_eq!(
        discovered.tui_sessions["agy"],
        TuiSessionDefaults {
            effort: None,
            https_proxy: None,
            idle_timeout_mins: Some(60),
            exited_retention_mins: None,
            ..Default::default()
        }
    );
    assert_eq!(
        discovered.tui_sessions["codex"],
        TuiSessionDefaults {
            effort: None,
            https_proxy: None,
            idle_timeout_mins: None,
            exited_retention_mins: Some(15),
            ..Default::default()
        }
    );
    assert_eq!(
        discovered.tui_sessions["claude"],
        TuiSessionDefaults {
            effort: None,
            https_proxy: Some("http://goose-harness-claude:8080".to_string()),
            idle_timeout_mins: None,
            exited_retention_mins: None,
            ..Default::default()
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

#[test]
fn tui_session_effort_survives_config_roundtrip() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  tui_sessions:
    default:
      effort: low
    agy:
      effort: medium
"#,
    );
    assert_eq!(
        settings.tui_sessions["default"].effort.as_deref(),
        Some("low")
    );
    assert_eq!(
        settings.tui_sessions["agy"].effort.as_deref(),
        Some("medium")
    );
    let yaml = serde_yaml::to_string(&settings).unwrap();
    let parsed = parse_harness_global_settings_str(&yaml);
    assert_eq!(parsed.tui_sessions, settings.tui_sessions);
}

#[test]
fn tui_session_model_and_fallback_models_set_unset_and_invalid() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  tui_sessions:
    unset_agent: {}
    set_agent:
      model: gemini-2.5-pro
      fallback_models:
        - gemini-2.5-flash
        - gemini-2.0-flash
    invalid_blank:
      model: "   "
      fallback_models:
        - ""
        - "  "
    invalid_types:
      model: 12345
      fallback_models: 67890
    bool_types:
      model: true
      fallback_models: false
    map_types:
      model:
        bad: object
      fallback_models:
        bad: object
    null_types:
      model: ~
      fallback_models: ~
    mixed_invalid_list:
      model: "  claude-3-7-sonnet  "
      fallback_models:
        - "   "
        - "claude-3-5-sonnet"
        - 42
        - true
        - ~
        - bad_map: false
        - "claude-3-5-haiku"
    comma_fallback:
      model: grok-4
      fallback_models: "grok-3, grok-2"
    comma_fallback_with_blanks:
      model: "  "
      fallback_models: "  ,   ,  "
    sibling_fields_kept:
      model: 12345
      effort: high
      https_proxy: http://127.0.0.1:2087
      fallback_models: false
      idle_timeout_mins: 30
"#,
    );

    // Unset defaults: model is None and fallback_models is empty
    let unset = &settings.tui_sessions["unset_agent"];
    assert_eq!(unset.model, None);
    assert!(!unset.has_model());
    assert!(unset.fallback_models.is_empty());
    assert!(!unset.has_fallback_models());
    assert_eq!(unset.resolved_model(), None);
    assert!(unset.resolved_fallback_models().is_empty());

    // Set configuration: model and fallback_models preserved
    let set = &settings.tui_sessions["set_agent"];
    assert_eq!(set.model.as_deref(), Some("gemini-2.5-pro"));
    assert!(set.has_model());
    assert_eq!(
        set.fallback_models,
        vec!["gemini-2.5-flash", "gemini-2.0-flash"]
    );
    assert!(set.has_fallback_models());
    assert_eq!(set.resolved_model(), Some("gemini-2.5-pro"));
    assert_eq!(
        set.resolved_fallback_models(),
        vec!["gemini-2.5-flash", "gemini-2.0-flash"]
    );

    // Blank values: trimmed and filtered to None/empty
    let blank = &settings.tui_sessions["invalid_blank"];
    assert_eq!(blank.model, None);
    assert!(blank.fallback_models.is_empty());
    assert_eq!(blank.resolved_model(), None);

    // Non-string types: degrade to None / empty list gracefully
    let invalid = &settings.tui_sessions["invalid_types"];
    assert_eq!(invalid.model, None);
    assert!(invalid.fallback_models.is_empty());
    assert_eq!(invalid.resolved_model(), None);
    assert!(invalid.resolved_fallback_models().is_empty());

    // Booleans: degrade to None / empty list gracefully
    let bools = &settings.tui_sessions["bool_types"];
    assert_eq!(bools.model, None);
    assert!(bools.fallback_models.is_empty());
    assert_eq!(bools.resolved_model(), None);
    assert!(bools.resolved_fallback_models().is_empty());

    // Mapping/objects: degrade to None / empty list gracefully
    let maps = &settings.tui_sessions["map_types"];
    assert_eq!(maps.model, None);
    assert!(maps.fallback_models.is_empty());
    assert_eq!(maps.resolved_model(), None);
    assert!(maps.resolved_fallback_models().is_empty());

    // Explicit null (~): degrades to None / empty list gracefully
    let nulls = &settings.tui_sessions["null_types"];
    assert_eq!(nulls.model, None);
    assert!(nulls.fallback_models.is_empty());
    assert_eq!(nulls.resolved_model(), None);
    assert!(nulls.resolved_fallback_models().is_empty());

    // Mixed list with blanks and invalid elements: cleans up to valid entries
    let mixed = &settings.tui_sessions["mixed_invalid_list"];
    assert_eq!(mixed.model.as_deref(), Some("claude-3-7-sonnet"));
    assert_eq!(
        mixed.fallback_models,
        vec!["claude-3-5-sonnet", "claude-3-5-haiku"]
    );
    assert_eq!(mixed.resolved_model(), Some("claude-3-7-sonnet"));
    assert_eq!(
        mixed.resolved_fallback_models(),
        vec!["claude-3-5-sonnet", "claude-3-5-haiku"]
    );

    // Comma-delimited string fallback format
    let comma = &settings.tui_sessions["comma_fallback"];
    assert_eq!(comma.model.as_deref(), Some("grok-4"));
    assert_eq!(comma.fallback_models, vec!["grok-3", "grok-2"]);

    // Comma-delimited string containing only blanks
    let comma_blank = &settings.tui_sessions["comma_fallback_with_blanks"];
    assert_eq!(comma_blank.model, None);
    assert!(comma_blank.fallback_models.is_empty());
    assert_eq!(comma_blank.resolved_model(), None);
    assert!(comma_blank.resolved_fallback_models().is_empty());

    // Invalid model/fallback values leave sibling fields intact
    let kept = &settings.tui_sessions["sibling_fields_kept"];
    assert_eq!(kept.model, None);
    assert!(kept.fallback_models.is_empty());
    assert_eq!(kept.effort.as_deref(), Some("high"));
    assert_eq!(kept.https_proxy.as_deref(), Some("http://127.0.0.1:2087"));
    assert_eq!(kept.idle_timeout_mins, Some(30));
}

#[test]
fn tui_session_model_and_fallback_models_aliases() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  tui_sessions:
    codex:
      default_model: o3-mini
      static_fallback_models:
        - gpt-4o
        - gpt-4o-mini
    agy:
      defaultModel: gemini-2.5-pro
      staticFallbackModels:
        - gemini-2.5-flash
    grok:
      model: grok-4
      fallbackModels:
        - grok-3
    claude:
      default_model: claude-3-7-sonnet
      fallback_model_list:
        - claude-3-5-sonnet
    custom:
      defaultModel: my-custom-model
      fallbackModelList:
        - other-model
    candidate_alias_snake:
      default_tui_model: deepseek-r1
      fallback_candidates:
        - deepseek-v3
        - qwen-2.5
    candidate_alias_camel:
      defaultTuiModel: deepseek-r1-distill
      fallbackCandidates:
        - qwen-2.5-coder
    static_candidate_alias_snake:
      tui_model: mistral-large
      static_fallback_candidates:
        - mistral-small
    static_candidate_alias_camel:
      tuiModel: command-r-plus
      staticFallbackCandidates:
        - command-r
    candidate_list_alias:
      model: llama-3.3-70b
      fallback_candidate_list:
        - llama-3.1-8b
    candidate_list_camel_alias:
      model: glm-4-plus
      fallbackCandidateList:
        - glm-4-air
"#,
    );

    let codex = &settings.tui_sessions["codex"];
    assert_eq!(codex.model.as_deref(), Some("o3-mini"));
    assert_eq!(codex.fallback_models, vec!["gpt-4o", "gpt-4o-mini"]);

    let agy = &settings.tui_sessions["agy"];
    assert_eq!(agy.model.as_deref(), Some("gemini-2.5-pro"));
    assert_eq!(agy.fallback_models, vec!["gemini-2.5-flash"]);

    let grok = &settings.tui_sessions["grok"];
    assert_eq!(grok.model.as_deref(), Some("grok-4"));
    assert_eq!(grok.fallback_models, vec!["grok-3"]);

    let claude = &settings.tui_sessions["claude"];
    assert_eq!(claude.model.as_deref(), Some("claude-3-7-sonnet"));
    assert_eq!(claude.fallback_models, vec!["claude-3-5-sonnet"]);

    let custom = &settings.tui_sessions["custom"];
    assert_eq!(custom.model.as_deref(), Some("my-custom-model"));
    assert_eq!(custom.fallback_models, vec!["other-model"]);

    let cand_snake = &settings.tui_sessions["candidate_alias_snake"];
    assert_eq!(cand_snake.model.as_deref(), Some("deepseek-r1"));
    assert_eq!(cand_snake.fallback_models, vec!["deepseek-v3", "qwen-2.5"]);

    let cand_camel = &settings.tui_sessions["candidate_alias_camel"];
    assert_eq!(cand_camel.model.as_deref(), Some("deepseek-r1-distill"));
    assert_eq!(cand_camel.fallback_models, vec!["qwen-2.5-coder"]);

    let static_cand_snake = &settings.tui_sessions["static_candidate_alias_snake"];
    assert_eq!(static_cand_snake.model.as_deref(), Some("mistral-large"));
    assert_eq!(static_cand_snake.fallback_models, vec!["mistral-small"]);

    let static_cand_camel = &settings.tui_sessions["static_candidate_alias_camel"];
    assert_eq!(static_cand_camel.model.as_deref(), Some("command-r-plus"));
    assert_eq!(static_cand_camel.fallback_models, vec!["command-r"]);

    let cand_list = &settings.tui_sessions["candidate_list_alias"];
    assert_eq!(cand_list.model.as_deref(), Some("llama-3.3-70b"));
    assert_eq!(cand_list.fallback_models, vec!["llama-3.1-8b"]);

    let cand_list_camel = &settings.tui_sessions["candidate_list_camel_alias"];
    assert_eq!(cand_list_camel.model.as_deref(), Some("glm-4-plus"));
    assert_eq!(cand_list_camel.fallback_models, vec!["glm-4-air"]);
}

#[test]
fn tui_session_model_roundtrip_and_serialization() {
    let defaults = TuiSessionDefaults {
        model: Some("gemini-2.5-pro".to_string()),
        fallback_models: vec!["gemini-2.5-flash".to_string()],
        ..Default::default()
    };

    let yaml = serde_yaml::to_string(&defaults).unwrap();
    assert!(yaml.contains("model: gemini-2.5-pro"));
    assert!(yaml.contains("gemini-2.5-flash"));

    let parsed: TuiSessionDefaults = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed, defaults);
}

#[test]
fn tui_session_defaults_sanitize_in_place() {
    let mut defaults = TuiSessionDefaults {
        model: Some("   ".to_string()),
        fallback_models: vec![
            "   ".to_string(),
            "  gemini-2.5-pro  ".to_string(),
            "".to_string(),
            "  claude-3-5-sonnet".to_string(),
        ],
        ..Default::default()
    };
    defaults.sanitize();
    assert_eq!(defaults.model, None);
    assert_eq!(
        defaults.fallback_models,
        vec![
            "gemini-2.5-pro".to_string(),
            "claude-3-5-sonnet".to_string(),
        ]
    );

    let mut valid_defaults = TuiSessionDefaults {
        model: Some("  gpt-4o  ".to_string()),
        fallback_models: vec!["  o3-mini  ".to_string()],
        ..Default::default()
    };
    valid_defaults.sanitize();
    assert_eq!(valid_defaults.model.as_deref(), Some("gpt-4o"));
    assert_eq!(valid_defaults.fallback_models, vec!["o3-mini".to_string()]);
}

#[test]
fn tui_session_json_serialization_and_deserialization() {
    let defaults = TuiSessionDefaults {
        model: Some("gemini-2.5-pro".to_string()),
        fallback_models: vec![
            "gemini-2.5-flash".to_string(),
            "gemini-2.0-flash".to_string(),
        ],
        idle_timeout_mins: Some(30),
        ..Default::default()
    };

    let json = serde_json::to_string(&defaults).unwrap();
    assert!(json.contains("\"model\":\"gemini-2.5-pro\""));
    assert!(json.contains("\"fallback_models\":[\"gemini-2.5-flash\",\"gemini-2.0-flash\"]"));

    let deserialized: TuiSessionDefaults = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, defaults);

    // Test malformed and blank fields via JSON
    let malformed_json = r#"{
        "model": "   ",
        "fallback_models": [123, "claude-3-5-sonnet", true, null, "   ", "claude-3-haiku"]
    }"#;
    let parsed: TuiSessionDefaults = serde_json::from_str(malformed_json).unwrap();
    assert_eq!(parsed.model, None);
    assert_eq!(
        parsed.fallback_models,
        vec![
            "claude-3-5-sonnet".to_string(),
            "claude-3-haiku".to_string()
        ]
    );
    assert_eq!(parsed.resolved_model(), None);
    assert_eq!(
        parsed.resolved_fallback_models(),
        vec![
            "claude-3-5-sonnet".to_string(),
            "claude-3-haiku".to_string()
        ]
    );
}

#[test]
fn tui_session_fallback_models_comma_in_list_items() {
    let settings = parse_harness_global_settings_str(
        r#"
harness:
  tui_sessions:
    codex:
      model: o3-mini
      fallback_models:
        - "gpt-4o, gpt-4o-mini"
        - "gpt-4-turbo"
        - "  "
"#,
    );

    let codex = &settings.tui_sessions["codex"];
    assert_eq!(codex.model.as_deref(), Some("o3-mini"));
    assert_eq!(
        codex.fallback_models,
        vec!["gpt-4o", "gpt-4o-mini", "gpt-4-turbo"]
    );
    assert!(codex.has_model());
    assert!(codex.has_fallback_models());
}

#[test]
fn discover_global_harness_settings_merges_models_across_workspace_files() {
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
      model: gemini-2.5-pro
      fallback_models:
        - gemini-2.5-flash
    grok:
      model: grok-4
"#,
    )
    .unwrap();

    std::fs::write(
        goose_dir.join("config.yaml"),
        r#"
harness:
  tui_sessions:
    default:
      model: gpt-4o
    claude:
      model: claude-3-7-sonnet
      fallback_candidates:
        - claude-3-5-sonnet
"#,
    )
    .unwrap();

    let discovered = discover_global_harness_settings(Some(dir.path()));
    // High precedence .openduck/config.yaml wins for "default"
    let default_entry = &discovered.tui_sessions["default"];
    assert_eq!(default_entry.model.as_deref(), Some("gemini-2.5-pro"));
    assert_eq!(default_entry.fallback_models, vec!["gemini-2.5-flash"]);

    // "grok" from openduck/config.yaml
    let grok_entry = &discovered.tui_sessions["grok"];
    assert_eq!(grok_entry.model.as_deref(), Some("grok-4"));

    // "claude" from goose/config.yaml
    let claude_entry = &discovered.tui_sessions["claude"];
    assert_eq!(claude_entry.model.as_deref(), Some("claude-3-7-sonnet"));
    assert_eq!(claude_entry.fallback_models, vec!["claude-3-5-sonnet"]);
}
