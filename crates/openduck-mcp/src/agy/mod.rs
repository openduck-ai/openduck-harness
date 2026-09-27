use indoc::formatdoc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ErrorCode, ErrorData, Implementation, InitializeResult,
        MetaObject, ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::process::Command;

const WORKING_DIR_HEADER: &str = "agent-working-dir";
const DEFAULT_OUTPUT_FORMAT: &str = "json";
const DEFAULT_PRINT_TIMEOUT: &str = "10m";
const DEFAULT_SESSION_LIMIT: usize = 20;

#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x00000010;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgyRunParams {
    /// Prompt for Antigravity CLI to execute
    pub prompt: String,
    /// Working directory for the Antigravity session. Defaults to goose's current project directory.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Existing Antigravity conversation ID to resume (`agy --conversation`)
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// Continue the most recent Antigravity conversation in the working directory (`agy --continue`)
    #[serde(default)]
    pub continue_last: bool,
    /// Antigravity model slug (for example gemini-3.1-pro-high). Omit to use agy's default.
    #[serde(default)]
    pub model: Option<String>,
    /// Reasoning effort: low, medium, or high
    #[serde(default)]
    pub effort: Option<String>,
    /// Agent name for this run (`agy agents` lists them)
    #[serde(default)]
    pub agent: Option<String>,
    /// Output format for headless execution. Defaults to `json`.
    #[serde(default)]
    pub output_format: Option<String>,
    /// Maximum time to wait for a response, for example `10m` or `600s`. Defaults to `10m`.
    #[serde(default)]
    pub print_timeout: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgySessionsParams {
    /// Optional search query over conversation titles, previews, ids, and workspaces
    #[serde(default)]
    pub query: Option<String>,
    /// Maximum number of conversations to list (default 20)
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgyStatusParams {
    /// Antigravity conversation ID from agy_run or agy_sessions
    pub conversation_id: String,
}

#[derive(Clone)]
pub struct AgyServer {
    tool_router: ToolRouter<Self>,
    instructions: String,
}

impl Default for AgyServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl AgyServer {
    pub fn new() -> Self {
        let instructions = formatdoc! {r#"
            The Antigravity CLI extension is enabled. Use it to delegate work to the local `agy` CLI
            without changing goose's LLM provider.

            Workflow:
            1. Call agy_run with a clear prompt. It starts an Antigravity conversation, lets agy use
               its own tools, and returns the result plus a conversation_id.
            2. To continue the same Antigravity conversation, call agy_run again with that
               conversation_id.
            3. Call agy_sessions to list recent Antigravity conversations, or agy_status to inspect one.

            agy_run always uses headless mode (`agy -p`) with --dangerously-skip-permissions so it can
            finish without a TTY. Conversations are stored under ~/.gemini/antigravity-cli/.

            Requires the `agy` binary on PATH (or AGY_COMMAND / ~/.local/bin/agy) and an authenticated
            Antigravity CLI install (interactive `agy` sign-in, or GEMINI_API_KEY with
            modelProvider=gemini in ~/.gemini/antigravity-cli/settings.json).
            "#};

        Self {
            tool_router: Self::tool_router(),
            instructions,
        }
    }

    /// Run a prompt through the Antigravity CLI (`agy -p`) and return the result plus conversation id.
    #[tool(
        name = "agy_run",
        description = "Send a prompt to the local Antigravity CLI (agy -p). Returns the result and conversation_id so you can resume or check status later."
    )]
    pub async fn agy_run(
        &self,
        params: Parameters<AgyRunParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        if params.prompt.trim().is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "prompt must not be empty",
                None,
            ));
        }

        let cwd = resolve_cwd(params.cwd.as_deref(), &context);
        let args = build_run_args(&params);
        let output = run_agy(&args, Some(&cwd)).await?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format_run_output(&output),
        )]))
    }

    /// List or search Antigravity CLI conversations.
    #[tool(
        name = "agy_sessions",
        description = "List recent Antigravity conversations, or search them by keyword. Use this to find a conversation_id for agy_run or agy_status."
    )]
    pub async fn agy_sessions(
        &self,
        params: Parameters<AgySessionsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let metadata = load_conversation_metadata();
        let query = params
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty());
        let limit = params
            .limit
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_SESSION_LIMIT);
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format_sessions(&metadata, query, limit),
        )]))
    }

    /// Read status for an Antigravity CLI conversation (title, workspace, timestamps).
    #[tool(
        name = "agy_status",
        description = "Inspect an Antigravity conversation by id: title, preview, workspace, step count, and timestamps."
    )]
    pub async fn agy_status(
        &self,
        params: Parameters<AgyStatusParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let conversation_id = params.0.conversation_id.trim();
        if conversation_id.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "conversation_id must not be empty",
                None,
            ));
        }

        let metadata = load_conversation_metadata();
        let Some(status) = format_conversation_status(&metadata, conversation_id) else {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("No Antigravity conversation found for id {conversation_id}"),
                None,
            ));
        };

        Ok(CallToolResult::success(vec![ContentBlock::text(status)]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AgyServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("goose-agy", env!("CARGO_PKG_VERSION")))
            .with_instructions(self.instructions.clone())
    }
}

fn extract_working_dir_from_meta(meta: &MetaObject) -> Option<PathBuf> {
    meta.0
        .get(WORKING_DIR_HEADER)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn resolve_cwd(explicit: Option<&str>, context: &RequestContext<RoleServer>) -> PathBuf {
    if let Some(cwd) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return PathBuf::from(cwd);
    }
    extract_working_dir_from_meta(&context.meta)
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn user_home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn agy_home() -> PathBuf {
    env::var("AGY_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| user_home().map(|home| home.join(".gemini/antigravity-cli")))
        .unwrap_or_else(|| PathBuf::from(".gemini/antigravity-cli"))
}

fn resolve_agy_command() -> Result<PathBuf, ErrorData> {
    if let Ok(explicit) = env::var("AGY_COMMAND") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("AGY_COMMAND is set but not a file: {}", path.display()),
            None,
        ));
    }

    if let Ok(path) = which::which("agy") {
        return Ok(path);
    }

    let fallbacks = [
        user_home().map(|h| h.join(".local/bin/agy")),
        env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("agy/bin/agy.exe")),
    ];
    for candidate in fallbacks.into_iter().flatten() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(ErrorData::new(
        ErrorCode::INTERNAL_ERROR,
        "Could not find the Antigravity CLI. Install it with `curl -fsSL https://antigravity.google/cli/install.sh | bash`, then sign in with an interactive `agy` session. Set AGY_COMMAND if agy is not on PATH.",
        None,
    ))
}

fn optional_flag(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn build_run_args(params: &AgyRunParams) -> Vec<String> {
    let output_format = optional_flag(params.output_format.as_deref())
        .unwrap_or_else(|| DEFAULT_OUTPUT_FORMAT.to_string());
    let print_timeout = optional_flag(params.print_timeout.as_deref())
        .unwrap_or_else(|| DEFAULT_PRINT_TIMEOUT.to_string());

    let mut args = vec![
        "--output-format".to_string(),
        output_format,
        "--dangerously-skip-permissions".to_string(),
        "--print-timeout".to_string(),
        print_timeout,
    ];
    if let Some(conversation_id) = optional_flag(params.conversation_id.as_deref()) {
        args.push("--conversation".to_string());
        args.push(conversation_id);
    } else if params.continue_last {
        args.push("--continue".to_string());
    }
    if let Some(model) = optional_flag(params.model.as_deref()) {
        args.push("--model".to_string());
        args.push(model);
    }
    if let Some(effort) = optional_flag(params.effort.as_deref()) {
        args.push("--effort".to_string());
        args.push(effort);
    }
    if let Some(agent) = optional_flag(params.agent.as_deref()) {
        args.push("--agent".to_string());
        args.push(agent);
    }
    // `-p` takes the prompt as its value, so it must come last.
    args.push("-p".to_string());
    args.push(params.prompt.clone());
    args
}

async fn run_agy(args: &[String], cwd: Option<&Path>) -> Result<String, ErrorData> {
    let command = resolve_agy_command()?;
    let mut cmd = Command::new(&command);
    cmd.args(args).stdin(Stdio::null());
    // agy hangs on Windows when launched with no console (CREATE_NO_WINDOW).
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    let output = cmd.output().await.map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to start {}: {e}", command.display()),
            None,
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        let detail = match (stdout.is_empty(), stderr.is_empty()) {
            (false, false) => format!("{stderr}\n{stdout}"),
            (false, true) => stdout,
            (true, false) => stderr,
            (true, true) => "(no output)".to_string(),
        };
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!(
                "agy exited with {}: {detail}",
                output.status.code().unwrap_or(-1)
            ),
            None,
        ));
    }
    Ok(stdout)
}

fn format_run_output(stdout: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(stdout) {
        if let Some(formatted) = format_result_object(result_payload(&value)) {
            return formatted;
        }
    }

    let mut last_result: Option<Value> = None;
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if let Some(payload) = result_payload(&value) {
                last_result = Some(payload.clone());
            }
        }
    }
    if let Some(formatted) = last_result
        .as_ref()
        .and_then(|value| format_result_object(Some(value)))
    {
        return formatted;
    }

    stdout.to_string()
}

fn result_payload(value: &Value) -> Option<&Value> {
    if value.get("event").and_then(Value::as_str) == Some("result") {
        return value.get("result");
    }
    if value.get("conversation_id").is_some() || value.get("status").is_some() {
        return Some(value);
    }
    None
}

fn format_result_object(value: Option<&Value>) -> Option<String> {
    let value = value?;
    let conversation_id = json_str(value, "conversation_id");
    let status = json_str(value, "status");
    if conversation_id.is_none() && status.is_none() {
        return None;
    }

    let mut lines = vec![
        format!(
            "conversation_id: {}",
            conversation_id.unwrap_or_else(|| "(unknown)".to_string())
        ),
        format!(
            "status: {}",
            status.unwrap_or_else(|| "unknown".to_string())
        ),
    ];
    if let Some(turns) = json_str(value, "num_turns") {
        lines.push(format!("num_turns: {turns}"));
    }
    if let Some(duration) = json_str(value, "duration_seconds") {
        lines.push(format!("duration_seconds: {duration}"));
    }
    if let Some(error) = json_str(value, "error") {
        lines.push(format!("error: {error}"));
    }

    let response = json_str(value, "response").unwrap_or_default();
    Some(
        format!("{}\n\n{response}", lines.join("\n"))
            .trim()
            .to_string(),
    )
}

fn json_str(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::Null => None,
        Value::String(s) if s.trim().is_empty() => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn load_conversation_metadata() -> Value {
    read_json(agy_home().join("cache/conversation_metadata.json"))
}

fn format_sessions(metadata: &Value, query: Option<&str>, limit: usize) -> String {
    let mut entries = collect_conversations(metadata);
    if let Some(query) = query {
        let needle = query.to_lowercase();
        entries.retain(|entry| entry.search_blob.contains(&needle));
    }
    if entries.is_empty() {
        return match query {
            Some(query) => format!("No Antigravity conversations matched {query:?}."),
            None => "No Antigravity conversations found.".to_string(),
        };
    }

    entries.truncate(limit);
    entries
        .iter()
        .map(|entry| {
            format!(
                "conversation_id: {}\n\
                 title: {}\n\
                 preview: {}\n\
                 steps: {}\n\
                 workspace: {}\n\
                 updated_at: {}",
                entry.id,
                entry.title,
                entry.preview,
                entry.steps,
                entry.workspace,
                entry.updated_at
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_conversation_status(metadata: &Value, conversation_id: &str) -> Option<String> {
    let conversations = metadata.get("conversations")?.as_object()?;
    let entry = conversations.get(conversation_id)?;
    let summary = entry.get("summary").cloned().unwrap_or(Value::Null);
    Some(format!(
        "conversation_id: {conversation_id}\n\
         title: {}\n\
         preview: {}\n\
         steps: {}\n\
         workspace: {}\n\
         agent: {}\n\
         project_id: {}\n\
         app_data_dir: {}\n\
         updated_at: {}\n\
         last_modified_time: {}\n\
         is_internal: {}",
        summary_field(&summary, "Title"),
        summary_field(&summary, "Preview"),
        summary_field(&summary, "NumSteps"),
        format_workspaces(&summary),
        summary_field(&summary, "AgentName"),
        summary_field(&summary, "ProjectID"),
        summary_field(&summary, "AppDataDir"),
        summary_field(&summary, "UpdatedAt"),
        json_str(entry, "last_modified_time").unwrap_or_else(|| "-".to_string()),
        json_str(entry, "is_internal").unwrap_or_else(|| "-".to_string()),
    ))
}

struct ConversationEntry {
    id: String,
    title: String,
    preview: String,
    steps: String,
    workspace: String,
    updated_at: String,
    sort_key: String,
    search_blob: String,
}

fn collect_conversations(metadata: &Value) -> Vec<ConversationEntry> {
    let Some(conversations) = metadata.get("conversations").and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut entries: Vec<ConversationEntry> = conversations
        .iter()
        .filter_map(|(id, entry)| {
            if entry.get("is_internal").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let summary = entry.get("summary").cloned().unwrap_or(Value::Null);
            let title = summary_field(&summary, "Title");
            let preview = summary_field(&summary, "Preview");
            let steps = summary_field(&summary, "NumSteps");
            let workspace = format_workspaces(&summary);
            let updated_at = json_str(entry, "last_modified_time")
                .or_else(|| json_str(&summary, "UpdatedAt"))
                .unwrap_or_else(|| "-".to_string());
            let search_blob = format!("{id} {title} {preview} {workspace}").to_lowercase();
            Some(ConversationEntry {
                id: id.clone(),
                title,
                preview,
                steps,
                workspace,
                sort_key: updated_at.clone(),
                updated_at,
                search_blob,
            })
        })
        .collect();

    entries.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));
    entries
}

fn summary_field(summary: &Value, key: &str) -> String {
    json_str(summary, key).unwrap_or_else(|| "-".to_string())
}

fn format_workspaces(summary: &Value) -> String {
    match summary.get("WorkspaceURIs") {
        Some(Value::Array(uris)) => {
            let formatted: Vec<String> = uris
                .iter()
                .filter_map(Value::as_str)
                .map(strip_file_uri)
                .filter(|s| !s.is_empty())
                .collect();
            if formatted.is_empty() {
                "-".to_string()
            } else {
                formatted.join(", ")
            }
        }
        Some(Value::String(s)) if !s.trim().is_empty() => strip_file_uri(s),
        _ => "-".to_string(),
    }
}

fn strip_file_uri(uri: &str) -> String {
    uri.strip_prefix("file://")
        .unwrap_or(uri)
        .trim()
        .to_string()
}

fn read_json(path: PathBuf) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_params(prompt: &str) -> AgyRunParams {
        AgyRunParams {
            prompt: prompt.to_string(),
            cwd: None,
            conversation_id: None,
            continue_last: false,
            model: None,
            effort: None,
            agent: None,
            output_format: None,
            print_timeout: None,
        }
    }

    fn sample_metadata() -> Value {
        serde_json::json!({
            "conversations": {
                "sess-new": {
                    "summary": {
                        "ID": "sess-new",
                        "Title": "Review ACP wiring",
                        "Preview": "Looking at providers",
                        "NumSteps": 12,
                        "UpdatedAt": "2026-08-24T10:00:00Z",
                        "WorkspaceURIs": ["file:///mnt/e/goose"],
                        "AppDataDir": "antigravity-cli",
                        "ProjectID": "proj-1",
                        "AgentName": "default"
                    },
                    "is_internal": false,
                    "last_modified_time": "2026-08-24T12:00:00Z"
                },
                "sess-old": {
                    "summary": {
                        "ID": "sess-old",
                        "Title": "",
                        "Preview": "Older mqtt work",
                        "NumSteps": 4,
                        "UpdatedAt": "2026-05-01T00:00:00Z",
                        "WorkspaceURIs": ["file:///home/user/mqtt-broker"]
                    },
                    "is_internal": false,
                    "last_modified_time": "2026-05-01T00:00:00Z"
                },
                "sess-hidden": {
                    "summary": {
                        "ID": "sess-hidden",
                        "Title": "Internal",
                        "Preview": "should be skipped"
                    },
                    "is_internal": true,
                    "last_modified_time": "2026-08-25T00:00:00Z"
                }
            }
        })
    }

    #[test]
    fn run_args_include_headless_json_and_skip_permissions() {
        assert_eq!(
            build_run_args(&run_params("review this crate")),
            vec![
                "--output-format",
                "json",
                "--dangerously-skip-permissions",
                "--print-timeout",
                "10m",
                "-p",
                "review this crate"
            ]
        );
    }

    #[test]
    fn run_args_resume_model_effort_and_timeout() {
        let mut params = run_params("continue");
        params.conversation_id = Some("abc-123".to_string());
        params.continue_last = true;
        params.model = Some("gemini-3.1-pro-high".to_string());
        params.effort = Some("high".to_string());
        params.agent = Some("reviewer".to_string());
        params.print_timeout = Some("20m".to_string());
        assert_eq!(
            build_run_args(&params),
            vec![
                "--output-format",
                "json",
                "--dangerously-skip-permissions",
                "--print-timeout",
                "20m",
                "--conversation",
                "abc-123",
                "--model",
                "gemini-3.1-pro-high",
                "--effort",
                "high",
                "--agent",
                "reviewer",
                "-p",
                "continue"
            ]
        );
    }

    #[test]
    fn run_args_custom_output_format_and_continue() {
        let mut params = run_params("plan");
        params.output_format = Some("stream-json".to_string());
        params.continue_last = true;
        assert_eq!(
            build_run_args(&params),
            vec![
                "--output-format",
                "stream-json",
                "--dangerously-skip-permissions",
                "--print-timeout",
                "10m",
                "--continue",
                "-p",
                "plan"
            ]
        );
    }

    #[test]
    fn format_run_output_extracts_conversation_and_response() {
        let formatted = format_run_output(
            r#"{"conversation_id":"sess-1","status":"SUCCESS","response":"done","num_turns":1,"duration_seconds":7.16}"#,
        );
        assert!(formatted.contains("conversation_id: sess-1"));
        assert!(formatted.contains("status: SUCCESS"));
        assert!(formatted.contains("num_turns: 1"));
        assert!(formatted.contains("done"));
    }

    #[test]
    fn format_run_output_extracts_from_stream_json() {
        let ndjson = indoc::indoc! {r#"
            {"event":"init","conversation_id":"sess-stream-1","init":{"cwd":"/tmp"}}
            {"event":"step_update","step_update":{"conversation_id":"sess-stream-1","text_delta":"Hello "}}
            {"event":"result","result":{"conversation_id":"sess-stream-1","status":"SUCCESS","response":"Hello World!","num_turns":1}}
        "#};
        let formatted = format_run_output(ndjson);
        assert!(formatted.contains("conversation_id: sess-stream-1"));
        assert!(formatted.contains("status: SUCCESS"));
        assert!(formatted.contains("Hello World!"));
    }

    #[test]
    fn sessions_lists_newest_first_and_skips_internal() {
        let listed = format_sessions(&sample_metadata(), None, 20);
        assert!(listed.contains("conversation_id: sess-new"));
        assert!(listed.contains("conversation_id: sess-old"));
        assert!(!listed.contains("sess-hidden"));
        assert!(listed.find("sess-new").unwrap() < listed.find("sess-old").unwrap());
        assert!(listed.contains("workspace: /mnt/e/goose"));
    }

    #[test]
    fn sessions_filters_by_query_and_limit() {
        let listed = format_sessions(&sample_metadata(), Some("mqtt"), 1);
        assert!(listed.contains("sess-old"));
        assert!(!listed.contains("sess-new"));

        let limited = format_sessions(&sample_metadata(), None, 1);
        assert!(limited.contains("sess-new"));
        assert!(!limited.contains("sess-old"));
    }

    #[test]
    fn status_formats_known_conversation() {
        let status = format_conversation_status(&sample_metadata(), "sess-new").unwrap();
        assert!(status.contains("conversation_id: sess-new"));
        assert!(status.contains("title: Review ACP wiring"));
        assert!(status.contains("workspace: /mnt/e/goose"));
        assert!(status.contains("steps: 12"));
        assert!(format_conversation_status(&sample_metadata(), "missing").is_none());
    }

    #[test]
    fn agy_server_advertises_tools() {
        let server = AgyServer::new();
        let info = server.get_info();
        assert_eq!(info.server_info.name, "goose-agy");
        assert!(info
            .instructions
            .unwrap()
            .contains("Antigravity CLI extension"));
    }
}
