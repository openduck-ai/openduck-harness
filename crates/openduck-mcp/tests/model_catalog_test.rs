//! Focused integration tests for the TUI Agent Model Catalog module behavior in a standalone test file.
//!
//! # Scope
//! - Cached model catalog: dynamic loader results are cached across calls.
//! - Dynamic loader caching: subsequent calls do not re-invoke the loader until refresh/TTL expiry.
//! - Failed loader fallback: errors in dynamic discovery degrade gracefully to static fallbacks.
//! - Refresh path: explicit refresh re-queries loader and updates cached entries; failed refresh degrades gracefully.
//! - Static fallback models: verified defaults for Agy, Grok, Codex, Claude, Custom.
//! - Free-text indication & resolution: validates `supports_free_text` and free-text custom model handling.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use openduck_mcp::tui_session::catalog::{
    AgentModelCatalog, CatalogModelEntry, CliModelLoader, ModelLoader, ModelSource,
};
use openduck_mcp::tui_session::AgentCliKind;
use tokio::sync::RwLock;

// ============================================================================
// Test Mock Loader Helper
// ============================================================================

struct MockModelLoader {
    call_count: AtomicUsize,
    models: RwLock<Vec<CatalogModelEntry>>,
    fail: AtomicBool,
    error_message: String,
}

impl MockModelLoader {
    fn new(initial_models: Vec<CatalogModelEntry>) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            models: RwLock::new(initial_models),
            fail: AtomicBool::new(false),
            error_message: "Simulated discovery failure".to_string(),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    fn set_fail(&self, fail: bool) {
        self.fail.store(fail, Ordering::SeqCst);
    }

    async fn set_models(&self, new_models: Vec<CatalogModelEntry>) {
        let mut guard = self.models.write().await;
        *guard = new_models;
    }
}

#[async_trait::async_trait]
impl ModelLoader for MockModelLoader {
    async fn load_models(&self, _agent: AgentCliKind) -> anyhow::Result<Vec<CatalogModelEntry>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            anyhow::bail!("{}", self.error_message);
        }
        Ok(self.models.read().await.clone())
    }
}

fn sample_dynamic_models(prefix: &str) -> Vec<CatalogModelEntry> {
    vec![
        CatalogModelEntry {
            id: format!("{prefix}-flash"),
            display_name: format!("{prefix} Flash"),
            description: Some("Fast and lightweight model".to_string()),
            is_default: false,
            source: ModelSource::DynamicLoader,
        },
        CatalogModelEntry {
            id: format!("{prefix}-pro"),
            display_name: format!("{prefix} Pro"),
            description: Some("High capability reasoning model".to_string()),
            is_default: true,
            source: ModelSource::DynamicLoader,
        },
    ]
}

// ============================================================================
// 1. Static Fallback Models Tests
// ============================================================================

#[test]
fn test_static_fallback_models_populated_for_all_standard_agents() {
    let standard_agents = [
        AgentCliKind::Agy,
        AgentCliKind::Grok,
        AgentCliKind::Codex,
        AgentCliKind::Claude,
    ];

    for agent in standard_agents {
        let models = AgentModelCatalog::static_fallback_models(agent);
        assert!(
            !models.is_empty(),
            "Expected non-empty static fallback models for agent {agent}"
        );

        for m in &models {
            assert!(!m.id.trim().is_empty(), "Model ID must not be empty");
            assert!(
                !m.display_name.trim().is_empty(),
                "Display name must not be empty"
            );
            assert_eq!(
                m.source,
                ModelSource::StaticFallback,
                "Source must be StaticFallback"
            );
        }

        let default_count = models.iter().filter(|m| m.is_default).count();
        assert_eq!(
            default_count, 1,
            "Agent {agent} should have exactly one default static model, found {default_count}"
        );

        let default_slug = AgentModelCatalog::default_model_for_agent(agent);
        let default_model = models.iter().find(|m| m.is_default).unwrap();
        assert_eq!(default_model.id, default_slug);
    }
}

#[test]
fn test_static_defaults_specific_agents() {
    assert_eq!(
        AgentModelCatalog::default_model_for_agent(AgentCliKind::Agy),
        "gemini-2.5-pro"
    );
    assert_eq!(
        AgentModelCatalog::default_model_for_agent(AgentCliKind::Grok),
        "grok-4"
    );
    assert_eq!(
        AgentModelCatalog::default_model_for_agent(AgentCliKind::Codex),
        "gpt-5"
    );
    assert_eq!(
        AgentModelCatalog::default_model_for_agent(AgentCliKind::Claude),
        "claude-3-7-sonnet"
    );
}

// ============================================================================
// 2. Free-Text Indication & Resolution Tests
// ============================================================================

#[test]
fn test_supports_free_text_indication() {
    assert!(AgentModelCatalog::supports_free_text(AgentCliKind::Agy));
    assert!(AgentModelCatalog::supports_free_text(AgentCliKind::Grok));
    assert!(AgentModelCatalog::supports_free_text(AgentCliKind::Codex));
    assert!(AgentModelCatalog::supports_free_text(AgentCliKind::Claude));
    assert!(AgentModelCatalog::supports_free_text(AgentCliKind::Custom));
}

#[tokio::test]
async fn test_resolve_model_behavior() {
    let catalog = AgentModelCatalog::new();

    // 1. None or empty string resolves to default model
    let resolved_default = catalog.resolve_model(AgentCliKind::Grok, None).await;
    assert_eq!(resolved_default, "grok-4");

    let resolved_empty = catalog.resolve_model(AgentCliKind::Grok, Some("")).await;
    assert_eq!(resolved_empty, "grok-4");

    let resolved_whitespace = catalog.resolve_model(AgentCliKind::Grok, Some("   ")).await;
    assert_eq!(resolved_whitespace, "grok-4");

    // 2. Exact match from static fallback resolves to itself
    let resolved_known = catalog
        .resolve_model(AgentCliKind::Codex, Some("gpt-4o"))
        .await;
    assert_eq!(resolved_known, "gpt-4o");

    // 3. Custom free-text model accepted and trimmed because supports_free_text is true
    let resolved_custom = catalog
        .resolve_model(AgentCliKind::Agy, Some("  gemini-experimental-preview  "))
        .await;
    assert_eq!(resolved_custom, "gemini-experimental-preview");
}

#[tokio::test]
async fn test_free_text_acceptance_across_all_supported_agents() {
    let catalog = AgentModelCatalog::new();
    let agents = [
        AgentCliKind::Agy,
        AgentCliKind::Grok,
        AgentCliKind::Codex,
        AgentCliKind::Claude,
        AgentCliKind::Custom,
    ];

    for agent in agents {
        let entry = catalog.get_models(agent).await;
        assert!(
            entry.supports_free_text,
            "Agent {agent} catalog entry must declare supports_free_text = true"
        );

        // Arbitrary free-text custom models should be accepted and trimmed
        let resolved = catalog
            .resolve_model(agent, Some("  vendor-x/custom-model:v2.5  "))
            .await;
        assert_eq!(
            resolved, "vendor-x/custom-model:v2.5",
            "Agent {agent} must accept arbitrary trimmed free-text slug"
        );

        let resolved_tag = catalog
            .resolve_model(agent, Some("my-org.llm-experiment-42"))
            .await;
        assert_eq!(
            resolved_tag, "my-org.llm-experiment-42",
            "Agent {agent} must accept custom free-text model slug"
        );
    }
}

#[tokio::test]
async fn test_free_text_acceptance_case_insensitive_matching_vs_arbitrary_slug() {
    let catalog = AgentModelCatalog::new();

    // Upper/mixed case of known ID resolves to canonical ID
    assert_eq!(
        catalog
            .resolve_model(AgentCliKind::Agy, Some("GEMINI-2.5-PRO"))
            .await,
        "gemini-2.5-pro"
    );
    assert_eq!(
        catalog
            .resolve_model(AgentCliKind::Codex, Some("GPT-4O-MINI"))
            .await,
        "gpt-4o-mini"
    );

    // Case-insensitive display name resolves to canonical ID
    assert_eq!(
        catalog
            .resolve_model(AgentCliKind::Claude, Some("claude 3.7 sonnet"))
            .await,
        "claude-3-7-sonnet"
    );
    assert_eq!(
        catalog
            .resolve_model(AgentCliKind::Grok, Some("Grok 4"))
            .await,
        "grok-4"
    );

    // Unmatched arbitrary slugs are preserved as free-text input (trimmed)
    assert_eq!(
        catalog
            .resolve_model(AgentCliKind::Agy, Some("  Ollama/Qwen-2.5-Coder:32B  "))
            .await,
        "Ollama/Qwen-2.5-Coder:32B"
    );
}

#[tokio::test]
async fn test_free_text_acceptance_in_dynamic_and_fallback_modes() {
    let catalog = AgentModelCatalog::new();

    // 1. Dynamic loader state
    let mock_loader = Arc::new(MockModelLoader::new(vec![
        CatalogModelEntry {
            id: "dynamic-alpha".to_string(),
            display_name: "Dynamic Alpha".to_string(),
            description: None,
            is_default: true,
            source: ModelSource::DynamicLoader,
        },
        CatalogModelEntry {
            id: "dynamic-beta".to_string(),
            display_name: "Dynamic Beta".to_string(),
            description: None,
            is_default: false,
            source: ModelSource::DynamicLoader,
        },
    ]));
    catalog
        .set_loader(AgentCliKind::Agy, mock_loader.clone())
        .await;

    let dynamic_entry = catalog.get_models(AgentCliKind::Agy).await;
    assert!(!dynamic_entry.is_fallback);
    assert!(dynamic_entry.supports_free_text);

    // Resolves known dynamic model
    assert_eq!(
        catalog
            .resolve_model(AgentCliKind::Agy, Some("dynamic-beta"))
            .await,
        "dynamic-beta"
    );
    // Accepts unknown free-text model under dynamic loader
    assert_eq!(
        catalog
            .resolve_model(AgentCliKind::Agy, Some("  external/unlisted-model  "))
            .await,
        "external/unlisted-model"
    );

    // 2. Static fallback state (Codex without loader)
    let fallback_entry = catalog.get_models(AgentCliKind::Codex).await;
    assert!(fallback_entry.is_fallback);
    assert!(fallback_entry.supports_free_text);

    // Accepts unknown free-text model under static fallback
    assert_eq!(
        catalog
            .resolve_model(AgentCliKind::Codex, Some("  unlisted-gpt-variant  "))
            .await,
        "unlisted-gpt-variant"
    );
}

// ============================================================================
// 3. Successful Loader Caching Tests
// ============================================================================

#[tokio::test]
async fn test_successful_loader_is_cached() {
    let catalog = AgentModelCatalog::new();
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("gemini-test")));

    catalog
        .set_loader(AgentCliKind::Agy, mock_loader.clone())
        .await;

    // First call: cache is cold, loader must be invoked
    let entry1 = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(
        mock_loader.calls(),
        1,
        "Loader should be called once on cold cache"
    );
    assert!(
        !entry1.is_fallback,
        "Should not be marked as fallback on successful load"
    );
    assert_eq!(entry1.models.len(), 2);
    assert_eq!(entry1.models[0].id, "gemini-test-flash");
    assert_eq!(entry1.models[1].id, "gemini-test-pro");
    assert_eq!(entry1.default_model.as_deref(), Some("gemini-test-pro"));

    // Second call: cache is warm, loader must NOT be invoked again
    let entry2 = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(
        mock_loader.calls(),
        1,
        "Loader must not be called again when cache is warm"
    );
    assert_eq!(entry2.models, entry1.models);
    assert_eq!(entry2.cached_at_unix_secs, entry1.cached_at_unix_secs);

    // Third call: repeated calls continue using cached models
    let entry3 = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(mock_loader.calls(), 1, "Loader call count must remain 1");
    assert_eq!(entry3.models, entry1.models);
}

#[tokio::test]
async fn test_cache_isolation_between_agents() {
    let catalog = AgentModelCatalog::new();
    let agy_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("agy-model")));
    let grok_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("grok-model")));

    catalog
        .set_loader(AgentCliKind::Agy, agy_loader.clone())
        .await;
    catalog
        .set_loader(AgentCliKind::Grok, grok_loader.clone())
        .await;

    // Requesting Agy should only invoke Agy's loader
    let agy_entry = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(agy_loader.calls(), 1);
    assert_eq!(grok_loader.calls(), 0);
    assert_eq!(agy_entry.models[0].id, "agy-model-flash");

    // Requesting Grok should only invoke Grok's loader
    let grok_entry = catalog.get_models(AgentCliKind::Grok).await;
    assert_eq!(agy_loader.calls(), 1);
    assert_eq!(grok_loader.calls(), 1);
    assert_eq!(grok_entry.models[0].id, "grok-model-flash");

    // Re-requesting Agy should use cache and not invoke either loader
    let _ = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(agy_loader.calls(), 1);
    assert_eq!(grok_loader.calls(), 1);
}

#[tokio::test]
async fn test_successful_discovery_global_loader_caching() {
    let catalog = AgentModelCatalog::new();
    let global_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("global")));

    catalog.set_global_loader(global_loader.clone()).await;

    // First call for Claude: uses global loader, populates Claude's cache entry
    let claude_entry = catalog.get_models(AgentCliKind::Claude).await;
    assert_eq!(global_loader.calls(), 1);
    assert!(!claude_entry.is_fallback);
    assert_eq!(claude_entry.models[0].id, "global-flash");
    assert_eq!(claude_entry.models[1].id, "global-pro");

    // Second call for Claude: hits cache, global loader is not called again
    let claude_entry2 = catalog.get_models(AgentCliKind::Claude).await;
    assert_eq!(global_loader.calls(), 1);
    assert_eq!(claude_entry2.models, claude_entry.models);

    // Call for Codex: also unconfigured dedicated loader, so it calls global loader
    let codex_entry = catalog.get_models(AgentCliKind::Codex).await;
    assert_eq!(global_loader.calls(), 2);
    assert!(!codex_entry.is_fallback);
    assert_eq!(codex_entry.models[0].id, "global-flash");

    // Re-requesting Codex hits cache
    let codex_entry2 = catalog.get_models(AgentCliKind::Codex).await;
    assert_eq!(global_loader.calls(), 2);
    assert_eq!(codex_entry2.models, codex_entry.models);
}

#[tokio::test]
async fn test_successful_discovery_dedicated_loader_overrides_global() {
    let catalog = AgentModelCatalog::new();
    let global_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("global")));
    let dedicated_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("dedicated")));

    catalog.set_global_loader(global_loader.clone()).await;
    catalog
        .set_loader(AgentCliKind::Agy, dedicated_loader.clone())
        .await;

    // Agy has a dedicated loader: must use dedicated loader, not global loader
    let entry = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(dedicated_loader.calls(), 1);
    assert_eq!(global_loader.calls(), 0);
    assert_eq!(entry.models[0].id, "dedicated-flash");

    // Second call uses cache
    let entry2 = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(dedicated_loader.calls(), 1);
    assert_eq!(global_loader.calls(), 0);
    assert_eq!(entry2.models, entry.models);
}

#[tokio::test]
async fn test_successful_discovery_default_model_selection() {
    let catalog = AgentModelCatalog::new();

    // Case 1: Explicit is_default = true on a non-first model
    let loader_explicit_default = Arc::new(MockModelLoader::new(vec![
        CatalogModelEntry {
            id: "model-1".to_string(),
            display_name: "Model 1".to_string(),
            description: None,
            is_default: false,
            source: ModelSource::DynamicLoader,
        },
        CatalogModelEntry {
            id: "model-2".to_string(),
            display_name: "Model 2".to_string(),
            description: None,
            is_default: true,
            source: ModelSource::DynamicLoader,
        },
    ]));
    catalog
        .set_loader(AgentCliKind::Grok, loader_explicit_default)
        .await;

    let entry1 = catalog.get_models(AgentCliKind::Grok).await;
    assert_eq!(entry1.default_model.as_deref(), Some("model-2"));

    // Case 2: No model has is_default = true -> falls back to the first model
    let loader_implicit_default = Arc::new(MockModelLoader::new(vec![
        CatalogModelEntry {
            id: "first-model".to_string(),
            display_name: "First Model".to_string(),
            description: None,
            is_default: false,
            source: ModelSource::DynamicLoader,
        },
        CatalogModelEntry {
            id: "second-model".to_string(),
            display_name: "Second Model".to_string(),
            description: None,
            is_default: false,
            source: ModelSource::DynamicLoader,
        },
    ]));
    catalog
        .set_loader(AgentCliKind::Claude, loader_implicit_default)
        .await;

    let entry2 = catalog.get_models(AgentCliKind::Claude).await;
    assert_eq!(entry2.default_model.as_deref(), Some("first-model"));
}

// ============================================================================
// 4. Failed Loader Fallback Tests
// ============================================================================

#[tokio::test]
async fn test_failed_loader_falls_back_to_static_models() {
    let catalog = AgentModelCatalog::new();
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("dynamic")));
    mock_loader.set_fail(true);

    catalog
        .set_loader(AgentCliKind::Codex, mock_loader.clone())
        .await;

    let entry = catalog.get_models(AgentCliKind::Codex).await;

    assert_eq!(mock_loader.calls(), 1, "Failed loader was invoked");
    assert!(
        entry.is_fallback,
        "Entry must be marked as is_fallback = true"
    );
    assert!(
        !entry.models.is_empty(),
        "Must contain static fallback models when loader fails"
    );

    let expected_static = AgentModelCatalog::static_fallback_models(AgentCliKind::Codex);
    assert_eq!(
        entry.models, expected_static,
        "Models must match static fallback models"
    );
    assert_eq!(entry.default_model.as_deref(), Some("gpt-5"));
    assert!(entry.supports_free_text);
}

#[tokio::test]
async fn test_unconfigured_loader_returns_static_models() {
    let catalog = AgentModelCatalog::new();

    let entry = catalog.get_models(AgentCliKind::Claude).await;

    assert!(
        entry.is_fallback,
        "Unconfigured loader returns fallback = true"
    );
    let expected = AgentModelCatalog::static_fallback_models(AgentCliKind::Claude);
    assert_eq!(entry.models, expected);
    assert_eq!(entry.default_model.as_deref(), Some("claude-3-7-sonnet"));
}

#[tokio::test]
async fn test_failed_discovery_falls_back_for_all_agent_kinds() {
    let agents = [
        AgentCliKind::Agy,
        AgentCliKind::Grok,
        AgentCliKind::Codex,
        AgentCliKind::Claude,
        AgentCliKind::Custom,
    ];

    for agent in agents {
        let catalog = AgentModelCatalog::new();
        let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("test")));
        mock_loader.set_fail(true);

        catalog.set_loader(agent, mock_loader.clone()).await;

        let entry = catalog.get_models(agent).await;
        assert_eq!(mock_loader.calls(), 1);
        assert!(
            entry.is_fallback,
            "Agent {agent} must be marked as is_fallback when loader fails"
        );
        let expected_models = AgentModelCatalog::static_fallback_models(agent);
        assert_eq!(
            entry.models, expected_models,
            "Agent {agent} models must equal static fallback models"
        );
        let expected_default = AgentModelCatalog::default_model_for_agent(agent);
        assert_eq!(
            entry.default_model.as_deref(),
            Some(expected_default),
            "Agent {agent} default model must match default_model_for_agent"
        );
        assert!(
            entry.supports_free_text,
            "Agent {agent} must have supports_free_text = true in fallback"
        );
    }
}

#[tokio::test]
async fn test_empty_discovery_falls_back_to_static_models_for_all_agents() {
    let agents = [
        AgentCliKind::Agy,
        AgentCliKind::Grok,
        AgentCliKind::Codex,
        AgentCliKind::Claude,
        AgentCliKind::Custom,
    ];

    for agent in agents {
        let catalog = AgentModelCatalog::new();
        let empty_loader = Arc::new(MockModelLoader::new(Vec::new()));
        catalog.set_loader(agent, empty_loader.clone()).await;

        let entry = catalog.get_models(agent).await;
        assert_eq!(empty_loader.calls(), 1);
        assert!(
            entry.is_fallback,
            "Agent {agent} must fall back when loader returns empty list"
        );
        assert!(
            !entry.models.is_empty(),
            "Agent {agent} must not return empty models on fallback"
        );
        assert_eq!(
            entry.models,
            AgentModelCatalog::static_fallback_models(agent)
        );
    }
}

#[tokio::test]
async fn test_failed_discovery_is_cached_and_prevents_thrashing() {
    let catalog = AgentModelCatalog::new();
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("fail-test")));
    mock_loader.set_fail(true);

    catalog
        .set_loader(AgentCliKind::Codex, mock_loader.clone())
        .await;

    // First call: fails, generates static fallback, stores in cache
    let entry1 = catalog.get_models(AgentCliKind::Codex).await;
    assert_eq!(mock_loader.calls(), 1);
    assert!(entry1.is_fallback);

    // Second call: within TTL, uses cached fallback entry without calling loader again
    let entry2 = catalog.get_models(AgentCliKind::Codex).await;
    assert_eq!(
        mock_loader.calls(),
        1,
        "Failed loader should not be re-invoked while cache is fresh"
    );
    assert_eq!(entry2.models, entry1.models);
    assert_eq!(entry2.cached_at_unix_secs, entry1.cached_at_unix_secs);
}

// ============================================================================
// 5. Refresh Path Tests (Successful & Failed)
// ============================================================================

#[tokio::test]
async fn test_refresh_reinvokes_loader_and_updates_cache() {
    let catalog = AgentModelCatalog::new();
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("v1")));

    catalog
        .set_loader(AgentCliKind::Grok, mock_loader.clone())
        .await;

    // Initial load returns v1
    let entry1 = catalog.get_models(AgentCliKind::Grok).await;
    assert_eq!(mock_loader.calls(), 1);
    assert_eq!(entry1.models[0].id, "v1-flash");

    // Update loader to return v2 models
    mock_loader.set_models(sample_dynamic_models("v2")).await;

    // Regular get_models still returns cached v1
    let cached_entry = catalog.get_models(AgentCliKind::Grok).await;
    assert_eq!(mock_loader.calls(), 1, "Cache is still warm");
    assert_eq!(cached_entry.models[0].id, "v1-flash");

    // Explicit refresh bypasses cache and queries loader
    let refreshed_entry = catalog.refresh(AgentCliKind::Grok).await;
    assert_eq!(mock_loader.calls(), 2, "Refresh must invoke loader again");
    assert!(!refreshed_entry.is_fallback);
    assert_eq!(refreshed_entry.models[0].id, "v2-flash");
    assert_eq!(refreshed_entry.models[1].id, "v2-pro");

    // Subsequent get_models now returns v2 from cache without calling loader again
    let entry_after_refresh = catalog.get_models(AgentCliKind::Grok).await;
    assert_eq!(mock_loader.calls(), 2, "Cache now holds v2");
    assert_eq!(entry_after_refresh.models[0].id, "v2-flash");
}

#[tokio::test]
async fn test_failed_refresh_preserves_stale_cache_or_falls_back() {
    let catalog = AgentModelCatalog::new();
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("v1")));

    catalog
        .set_loader(AgentCliKind::Agy, mock_loader.clone())
        .await;

    // 1. Initial successful load
    let entry1 = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(mock_loader.calls(), 1);
    assert!(!entry1.is_fallback);
    assert_eq!(entry1.models[0].id, "v1-flash");

    // 2. Set loader to fail on subsequent refresh
    mock_loader.set_fail(true);

    // 3. Trigger refresh: loader fails
    let entry_after_failed_refresh = catalog.refresh(AgentCliKind::Agy).await;
    assert_eq!(mock_loader.calls(), 2, "Refresh attempted loader");

    // Catalog preserves existing cached dynamic models instead of wiping them out
    assert!(
        !entry_after_failed_refresh.models.is_empty(),
        "Models must not be empty after failed refresh"
    );
    assert_eq!(
        entry_after_failed_refresh.models, entry1.models,
        "Previously discovered models must be preserved on failed refresh"
    );
    assert!(
        !entry_after_failed_refresh.is_fallback,
        "Entry should keep is_fallback = false because dynamic models are preserved"
    );

    let next_entry = catalog.get_models(AgentCliKind::Agy).await;
    assert_eq!(next_entry.models, entry1.models);
}

#[tokio::test]
async fn test_explicit_refresh_on_cold_cache() {
    let catalog = AgentModelCatalog::new();
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("cold-refresh")));

    catalog
        .set_loader(AgentCliKind::Claude, mock_loader.clone())
        .await;

    // Direct explicit refresh on an empty/cold cache
    let entry = catalog.refresh(AgentCliKind::Claude).await;
    assert_eq!(
        mock_loader.calls(),
        1,
        "Refresh invokes loader on cold cache"
    );
    assert!(!entry.is_fallback);
    assert_eq!(entry.models[0].id, "cold-refresh-flash");

    // Subsequent get_models uses the refreshed cache entry
    let cached = catalog.get_models(AgentCliKind::Claude).await;
    assert_eq!(mock_loader.calls(), 1, "Cache is warm after refresh");
    assert_eq!(cached.models, entry.models);
}

#[tokio::test]
async fn test_explicit_refresh_failure_on_cold_cache_creates_static_fallback() {
    let catalog = AgentModelCatalog::new();
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("fail-cold")));
    mock_loader.set_fail(true);

    catalog
        .set_loader(AgentCliKind::Codex, mock_loader.clone())
        .await;

    // Refresh on cold cache with failing loader produces static fallback
    let entry = catalog.refresh(AgentCliKind::Codex).await;
    assert_eq!(mock_loader.calls(), 1);
    assert!(entry.is_fallback);
    assert_eq!(
        entry.models,
        AgentModelCatalog::static_fallback_models(AgentCliKind::Codex)
    );

    // Cached fallback is returned on subsequent read without re-invoking loader
    let cached = catalog.get_models(AgentCliKind::Codex).await;
    assert_eq!(mock_loader.calls(), 1);
    assert_eq!(cached.models, entry.models);
}

#[tokio::test]
async fn test_explicit_refresh_empty_preserves_cached_dynamic_models() {
    let catalog = AgentModelCatalog::new();
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("initial")));

    catalog
        .set_loader(AgentCliKind::Grok, mock_loader.clone())
        .await;

    // Initial load
    let initial = catalog.get_models(AgentCliKind::Grok).await;
    assert_eq!(mock_loader.calls(), 1);
    assert!(!initial.is_fallback);

    // Loader returns empty list on refresh
    mock_loader.set_models(Vec::new()).await;

    let refreshed = catalog.refresh(AgentCliKind::Grok).await;
    assert_eq!(mock_loader.calls(), 2);
    assert_eq!(
        refreshed.models, initial.models,
        "Empty discovery during refresh must preserve existing cached dynamic models"
    );
    assert!(!refreshed.is_fallback);

    // Cached models remain available on subsequent calls
    let subsequent = catalog.get_models(AgentCliKind::Grok).await;
    assert_eq!(mock_loader.calls(), 2);
    assert_eq!(subsequent.models, initial.models);
}

// ============================================================================
// 6. Cache Expiry / TTL Behavior Tests
// ============================================================================

#[tokio::test]
async fn test_cache_ttl_expiration() {
    let catalog = AgentModelCatalog::with_ttl(Duration::from_millis(50));
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("v1")));

    catalog
        .set_loader(AgentCliKind::Codex, mock_loader.clone())
        .await;

    // 1st call: cold cache, calls loader
    let _ = catalog.get_models(AgentCliKind::Codex).await;
    assert_eq!(mock_loader.calls(), 1);

    // Immediate 2nd call: within TTL, uses cache
    let _ = catalog.get_models(AgentCliKind::Codex).await;
    assert_eq!(mock_loader.calls(), 1);

    // Wait for TTL to expire
    tokio::time::sleep(Duration::from_millis(70)).await;

    // 3rd call: TTL expired, loader re-invoked
    let _ = catalog.get_models(AgentCliKind::Codex).await;
    assert_eq!(
        mock_loader.calls(),
        2,
        "Loader must be invoked after TTL expiration"
    );
}

// ============================================================================
// 7. Concurrent Access & Thread Safety Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_access_thread_safety() {
    let catalog = Arc::new(AgentModelCatalog::new());
    let mock_loader = Arc::new(MockModelLoader::new(sample_dynamic_models("thread-safe")));

    catalog
        .set_loader(AgentCliKind::Grok, mock_loader.clone())
        .await;

    let mut handles = Vec::new();

    for _ in 0..10 {
        let cat = catalog.clone();
        handles.push(tokio::spawn(async move {
            cat.get_models(AgentCliKind::Grok).await
        }));
    }

    for handle in handles {
        let entry = handle.await.expect("task join failed");
        assert_eq!(entry.models.len(), 2);
        assert_eq!(entry.models[0].id, "thread-safe-flash");
    }

    assert_eq!(
        mock_loader.calls(),
        1,
        "concurrent cache misses share discovery"
    );
}

// ============================================================================
// 8. Production CLI Discovery Parser Tests
// ============================================================================

#[test]
fn test_parse_agy_output_standard() {
    let raw = "\
Fetching available models...
gemini-4-argon-medium\tGemini 4 Argon (Medium)
gemini-4-argon-high\tGemini 4 Argon (High)
gemini-3.8-flash-high\tGemini 3.8 Flash (High)
claude-sonnet-5-5-high\tClaude Sonnet 5.5 (High)
gpt-oss-120b-medium\tGPT-OSS 120B (Medium)
";

    let models = CliModelLoader::parse_agy_output(raw);
    assert_eq!(models.len(), 5);

    assert_eq!(models[0].id, "gemini-4-argon-medium");
    assert_eq!(models[0].display_name, "Gemini 4 Argon (Medium)");
    assert_eq!(models[0].source, ModelSource::DynamicLoader);
    assert!(models[0].is_default);

    assert_eq!(models[1].id, "gemini-4-argon-high");
    assert_eq!(models[1].display_name, "Gemini 4 Argon (High)");
    assert!(!models[1].is_default);

    assert_eq!(models[2].id, "gemini-3.8-flash-high");
    assert_eq!(models[3].id, "claude-sonnet-5-5-high");
    assert_eq!(models[4].id, "gpt-oss-120b-medium");

    // Exactly one default model
    let defaults = models.iter().filter(|m| m.is_default).count();
    assert_eq!(defaults, 1);
}

#[test]
fn test_parse_agy_output_with_spinners_and_ansi() {
    let raw = "\x1b[32m⠋ Fetching available models...\x1b[0m\r\n\
\x1b[32m⠙ Fetching available models...\x1b[0m\r\n\
gemini-2.5-pro    Gemini 2.5 Pro\r\n\
gemini-2.5-flash  Gemini 2.5 Flash\r\n";

    let models = CliModelLoader::parse_agy_output(raw);
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "gemini-2.5-pro");
    assert_eq!(models[0].display_name, "Gemini 2.5 Pro");
    assert!(
        models[0].is_default,
        "gemini-2.5-pro should be preferred as default"
    );
    assert_eq!(models[1].id, "gemini-2.5-flash");
    assert!(!models[1].is_default);
}

#[test]
fn test_parse_agy_output_empty_or_malformed() {
    assert!(CliModelLoader::parse_agy_output("").is_empty());
    assert!(CliModelLoader::parse_agy_output("   \n\r\n  ").is_empty());
    assert!(CliModelLoader::parse_agy_output("Fetching available models...\n").is_empty());
    assert!(
        CliModelLoader::parse_agy_output("Usage: agy models [flags]\n  -h Show help\n").is_empty()
    );

    let help_output = "\
Usage: agy models [flags]

List available models

Flags:
  -h      Show help
  --help  Show help
";
    assert!(CliModelLoader::parse_agy_output(help_output).is_empty());
}

#[test]
fn test_parse_grok_output_standard() {
    let raw = "\
You are logged in with grok.com.

Default model: grok-4.6

Available models:
  - grok-4.7
  - grok-4.7-build-fast
  * grok-4.6 (default)
  - grok-4.5
Finishing session…
";

    let models = CliModelLoader::parse_grok_output(raw);
    assert_eq!(models.len(), 4);

    assert_eq!(models[0].id, "grok-4.7");
    assert_eq!(models[0].display_name, "Grok 4.7");
    assert!(!models[0].is_default);

    assert_eq!(models[1].id, "grok-4.7-build-fast");
    assert_eq!(models[1].display_name, "Grok 4.7 Build Fast");
    assert!(!models[1].is_default);

    assert_eq!(models[2].id, "grok-4.6");
    assert_eq!(models[2].display_name, "Grok 4.6");
    assert!(models[2].is_default);
    assert_eq!(models[2].source, ModelSource::DynamicLoader);

    assert_eq!(models[3].id, "grok-4.5");
    assert_eq!(models[3].display_name, "Grok 4.5");
    assert!(!models[3].is_default);

    let defaults = models.iter().filter(|m| m.is_default).count();
    assert_eq!(defaults, 1);
}

#[test]
fn test_parse_grok_output_without_star_marker() {
    let raw = "\
Default model: grok-4.7

Available models:
  - grok-4.7
  - grok-4.6
";

    let models = CliModelLoader::parse_grok_output(raw);
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "grok-4.7");
    assert!(
        models[0].is_default,
        "Should match 'Default model: grok-4.7' line"
    );
    assert_eq!(models[1].id, "grok-4.6");
    assert!(!models[1].is_default);
}

#[test]
fn test_parse_grok_output_empty_or_malformed() {
    assert!(CliModelLoader::parse_grok_output("").is_empty());
    assert!(CliModelLoader::parse_grok_output("Error: unauthorized\nPlease log in\n").is_empty());
}

#[test]
fn test_cli_discovery_factory_constructors() {
    let catalog1 = AgentModelCatalog::with_cli_discovery();
    let catalog2 = AgentModelCatalog::default_discovery();
    let catalog3 = AgentModelCatalog::with_production_discovery();

    let _ = catalog1;
    let _ = catalog2;
    let _ = catalog3;

    let default_loader = CliModelLoader::default_factory();
    assert!(Arc::strong_count(&default_loader) >= 1);
}

#[tokio::test]
async fn test_production_cli_discovery_resolution() {
    let catalog = AgentModelCatalog::with_cli_discovery();

    // Querying Agy and Grok via production catalog:
    // Even if CLI commands are unavailable or fail in certain environments,
    // get_models must succeed and return non-empty models (either dynamic or graceful fallback).
    let agy_models = catalog.get_models(AgentCliKind::Agy).await;
    assert!(!agy_models.models.is_empty());
    assert!(agy_models.default_model.is_some());

    let grok_models = catalog.get_models(AgentCliKind::Grok).await;
    assert!(!grok_models.models.is_empty());
    assert!(grok_models.default_model.is_some());

    // Agents without CLI discovery gracefully return static fallbacks
    let codex_models = catalog.get_models(AgentCliKind::Codex).await;
    assert!(codex_models.is_fallback);
    assert_eq!(codex_models.default_model.as_deref(), Some("gpt-5"));
}

#[tokio::test]
async fn test_empty_discovery_preserves_cached_models_on_expiry_and_refresh() {
    let catalog = AgentModelCatalog::with_ttl(Duration::ZERO);
    let loader = Arc::new(MockModelLoader::new(sample_dynamic_models("live")));
    catalog.set_loader(AgentCliKind::Agy, loader.clone()).await;
    let original = catalog.get_models(AgentCliKind::Agy).await;

    loader.set_models(Vec::new()).await;
    assert_eq!(catalog.get_models(AgentCliKind::Agy).await, original);
    assert_eq!(catalog.refresh(AgentCliKind::Agy).await, original);
    assert_eq!(loader.calls(), 3);
}

#[tokio::test]
async fn test_empty_initial_discovery_uses_cached_static_fallback() {
    let catalog = AgentModelCatalog::new();
    let loader = Arc::new(MockModelLoader::new(Vec::new()));
    catalog.set_global_loader(loader.clone()).await;
    let entry = catalog.get_models(AgentCliKind::Grok).await;
    assert!(entry.is_fallback);
    assert!(!entry.models.is_empty());
    assert_eq!(catalog.get_models(AgentCliKind::Grok).await, entry);
    assert_eq!(loader.calls(), 1);
}

struct BlockingModelLoader {
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl ModelLoader for BlockingModelLoader {
    async fn load_models(&self, _agent: AgentCliKind) -> anyhow::Result<Vec<CatalogModelEntry>> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(sample_dynamic_models("refreshed"))
    }
}

#[tokio::test]
async fn test_discovery_does_not_block_cached_reads_or_other_agents() {
    let catalog = AgentModelCatalog::new();
    let original = catalog.get_models(AgentCliKind::Agy).await;
    let loader = Arc::new(BlockingModelLoader {
        started: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    catalog.set_loader(AgentCliKind::Agy, loader.clone()).await;
    let refreshing = tokio::spawn({
        let catalog = catalog.clone();
        async move { catalog.refresh(AgentCliKind::Agy).await }
    });
    tokio::time::timeout(Duration::from_secs(2), loader.started.notified())
        .await
        .expect("refresh should start");

    let reads = tokio::time::timeout(Duration::from_secs(2), async {
        let cached = catalog.get_models(AgentCliKind::Agy).await;
        let unrelated = catalog.get_models(AgentCliKind::Claude).await;
        (cached, unrelated)
    })
    .await;
    loader.release.notify_one();
    let refreshed = refreshing.await.unwrap();
    let (cached, unrelated) = reads.expect("discovery must not hold the cache lock");
    assert_eq!(cached, original);
    assert!(unrelated.is_fallback);
    assert_eq!(refreshed.models[0].id, "refreshed-flash");
}
