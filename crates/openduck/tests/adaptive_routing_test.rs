use async_trait::async_trait;
use futures::{stream, StreamExt};
use openduck::agents::{Agent, AgentConfig, AgentEvent, GoosePlatform, SessionConfig};
use openduck::config::permission::PermissionManager;
use openduck::config::ExtensionConfig;
use openduck::config::GooseMode;
use openduck::conversation::message::{Message, MessageContent};
use openduck::providers::adaptive_def::resolve_candidate_spec;
use openduck::providers::base::{MessageStream, Provider};
use openduck::session::{SessionManager, SessionType};
use openduck_providers::adaptive::policy::{AdaptivePolicyConfig, RoutingMode};
use openduck_providers::adaptive::{AdaptiveProvider, CandidateTarget};
use openduck_providers::conversation::token_usage::{ProviderUsage, Usage};
use openduck_providers::errors::ProviderError;
use openduck_providers::model::ModelConfig;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use std::sync::{Arc, Mutex};

struct RecordingProvider {
    name: String,
    seen_models: Arc<Mutex<Vec<String>>>,
    switch_on_first_tool_call: bool,
    on_tool_request: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl RecordingProvider {
    fn new(
        name: &str,
        seen_models: Arc<Mutex<Vec<String>>>,
        switch_on_first_tool_call: bool,
    ) -> Self {
        Self {
            name: name.to_string(),
            seen_models,
            switch_on_first_tool_call,
            on_tool_request: None,
        }
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        _system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.seen_models
            .lock()
            .unwrap()
            .push(model_config.model_name.clone());

        let has_tool_response = messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, MessageContent::ToolResponse(_)))
        });

        let msg = if self.switch_on_first_tool_call && !has_tool_response {
            if let Some(cb) = &self.on_tool_request {
                cb();
            }
            let params = CallToolRequestParams::new("frontend_ping");
            Message::assistant().with_tool_request("call_123", Ok(params))
        } else {
            Message::assistant().with_text(format!("answered by {}", model_config.model_name))
        };

        let usage = ProviderUsage::new(
            model_config.model_name.clone(),
            Usage::new(Some(10), Some(5), Some(15)),
        );
        Ok(Box::pin(stream::iter(vec![Ok((Some(msg), Some(usage)))])))
    }
}

async fn make_test_agent(temp_dir: &tempfile::TempDir) -> (Agent, Arc<SessionManager>, String) {
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    let permission_manager = Arc::new(PermissionManager::new(temp_dir.path().join("perms.yaml")));
    let config = AgentConfig::new(
        session_manager.clone(),
        permission_manager,
        None,
        GooseMode::Auto,
        true,
        GoosePlatform::GooseCli,
    );
    let agent = Agent::with_config(config);
    let session = session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "adaptive-test".to_string(),
            SessionType::Hidden,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    (agent, session_manager, session.id)
}

#[tokio::test]
async fn persistence_failure_keeps_existing_provider_and_concurrent_updates_serialize() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (agent, _sm, session_id) = make_test_agent(&temp_dir).await;
    let seen = Arc::new(Mutex::new(Vec::new()));

    let p1: Arc<dyn Provider> = Arc::new(RecordingProvider::new("openai", seen.clone(), false));
    let p2: Arc<dyn Provider> = Arc::new(RecordingProvider::new("openai", seen.clone(), false));

    let v1 = agent
        .update_provider_versioned(p1.clone(), ModelConfig::new("model-a"), &session_id)
        .await
        .unwrap();
    assert_eq!(v1, 1);

    // Persisting to a non-existent session must fail and keep model-a active
    let err = agent
        .update_provider_versioned(
            p2.clone(),
            ModelConfig::new("model-b"),
            "missing-session-id",
        )
        .await
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("Failed to persist provider config"));

    let snap = agent.routing_handle().snapshot().unwrap();
    assert_eq!(snap.version, 1);
    assert_eq!(snap.model_config.model_name, "model-a");

    // Concurrent updates serialize cleanly with monotonic versions
    let a = Arc::new(agent);
    let (r1, r2) = tokio::join!(
        a.update_provider_versioned(p2.clone(), ModelConfig::new("model-b"), &session_id),
        a.update_provider_versioned(p2.clone(), ModelConfig::new("model-c"), &session_id),
    );
    let v_a = r1.unwrap();
    let v_b = r2.unwrap();
    assert_ne!(v_a, v_b);
    assert_eq!(v_a.max(v_b), 3);
    assert_eq!(a.routing_handle().current_version(), 3);
}

async fn run_mid_turn_switch_scenario(use_state_machine: bool) {
    let _env = env_lock::lock_env([(
        "OPENDUCK_STATE_MACHINE",
        Some(if use_state_machine { "1" } else { "0" }),
    )]);

    let temp_dir = tempfile::tempdir().unwrap();
    let (agent, session_manager, session_id) = make_test_agent(&temp_dir).await;
    let agent = Arc::new(agent);

    // Register a frontend tool so the agent loop executes a tool turn and then runs a second inference
    let tool = Tool::new("frontend_ping", "ping", Arc::new(serde_json::Map::new()));
    agent
        .add_extension(
            ExtensionConfig::Frontend {
                name: "frontend_test".to_string(),
                description: "frontend test".to_string(),
                tools: vec![tool],
                instructions: None,
                bundled: None,
                available_tools: Vec::new(),
            },
            &session_id,
        )
        .await
        .unwrap();

    let seen_models = Arc::new(Mutex::new(Vec::new()));
    let p_regular: Arc<dyn Provider> =
        Arc::new(RecordingProvider::new("openai", seen_models.clone(), true));
    let p_economy: Arc<dyn Provider> =
        Arc::new(RecordingProvider::new("openai", seen_models.clone(), false));

    agent
        .update_provider_versioned(p_regular, ModelConfig::new("regular-model"), &session_id)
        .await
        .unwrap();

    let session_cfg = SessionConfig {
        id: session_id.clone(),
        schedule_id: None,
        max_turns: Some(5),
        retry_config: None,
        permission_timeout: None,
    };

    let mut events = agent
        .reply(Message::user().with_text("run tool"), session_cfg, None)
        .await
        .unwrap();

    let mut saw_tool_request = false;
    let mut final_text = String::new();
    while let Some(event) = events.next().await {
        match event.unwrap() {
            AgentEvent::Message(msg) => {
                for block in &msg.content {
                    if let MessageContent::FrontendToolRequest(req) = block {
                        saw_tool_request = true;
                        // Mid-turn, while the tool is executing, switch the model!
                        agent
                            .update_provider_versioned(
                                p_economy.clone(),
                                ModelConfig::new("economy-model"),
                                &session_id,
                            )
                            .await
                            .unwrap();
                        agent
                            .handle_tool_result(
                                req.id.clone(),
                                Ok(CallToolResult::success(vec![ContentBlock::text("pong")])),
                            )
                            .await;
                    }
                }
                let txt = msg.as_concat_text();
                if !txt.is_empty() {
                    final_text = txt;
                }
            }
            _ => {}
        }
    }

    assert!(saw_tool_request, "expected frontend tool request");
    assert_eq!(final_text, "answered by economy-model");
    assert_eq!(
        *seen_models.lock().unwrap(),
        vec!["regular-model".to_string(), "economy-model".to_string()]
    );

    // Verify conversation history preserved tool call and result
    let persisted = session_manager
        .get_session(&session_id, true)
        .await
        .unwrap();
    let msgs = persisted.conversation.unwrap().messages().clone();
    assert!(msgs.iter().any(|m| m
        .content
        .iter()
        .any(|c| matches!(c, MessageContent::ToolRequest(_)))));
    assert!(msgs.iter().any(|m| m
        .content
        .iter()
        .any(|c| matches!(c, MessageContent::ToolResponse(_)))));
}

#[tokio::test]
async fn mid_turn_switch_works_in_both_legacy_and_state_machine_loops() {
    run_mid_turn_switch_scenario(false).await;
    run_mid_turn_switch_scenario(true).await;
}

#[tokio::test]
async fn adaptive_provider_integrates_with_agent_and_resolves_candidates() {
    // Candidate resolution checks
    let (prov, model) = resolve_candidate_spec("openai/gpt-4o-mini", None, None)
        .await
        .unwrap();
    assert_eq!(prov, "openai");
    assert_eq!(model, "gpt-4o-mini");

    let (prov2, model2) = resolve_candidate_spec("deepseek-v4.1-flash", None, Some("ark"))
        .await
        .unwrap();
    assert_eq!(prov2, "ark");
    assert_eq!(model2, "deepseek-v4.1-flash");

    assert!(resolve_candidate_spec("adaptive/foo", None, None)
        .await
        .is_err());

    // Agent + AdaptiveProvider manual pin
    let temp_dir = tempfile::tempdir().unwrap();
    let (agent, _sm, session_id) = make_test_agent(&temp_dir).await;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let underlying: Arc<dyn Provider> =
        Arc::new(RecordingProvider::new("ark", seen.clone(), false));

    let adaptive = Arc::new(
        AdaptiveProvider::new(
            CandidateTarget {
                provider: underlying.clone(),
                model_config: ModelConfig::new("deepseek-v4.1-flash"),
            },
            CandidateTarget {
                provider: underlying,
                model_config: ModelConfig::new("deepseek-v4.1-flash-economy"),
            },
            AdaptivePolicyConfig::default(),
            RoutingMode::Auto,
        )
        .unwrap(),
    );

    agent
        .update_provider_versioned(adaptive.clone(), ModelConfig::new("economy"), &session_id)
        .await
        .unwrap();

    let status = agent.routing_status().unwrap();
    assert_eq!(status.mode, "fixed_economy");
    assert_eq!(status.primary_model, "deepseek-v4.1-flash-economy");
}
