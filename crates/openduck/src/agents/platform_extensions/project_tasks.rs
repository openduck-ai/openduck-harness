use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use crate::control::project_harness::{
    create_project_task, get_project_task, launch_project_task, list_project_tasks,
    run_project_task, update_project_task, ProjectTaskRunRequest,
};
use anyhow::Result;
use async_trait::async_trait;
use axum::extract::Path as AxumPath;
use axum::Json;
use indoc::indoc;
use openduck_harness::project::{ProjectTaskDefinition, TaskVerifierSpec};
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool, ToolAnnotations,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "project_tasks";

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListTasksParams {
    /// Optional search query to filter tasks by ID, name, category, or prompt preview
    pub query: Option<String>,
    /// Optional tag to filter tasks by
    pub tag: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetTaskParams {
    /// Unique task ID slug (e.g. "deploy-api" or "fetch-data")
    pub task_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateTaskParams {
    /// Unique task ID slug without spaces (e.g. "unit-test-auth" or "data-sync")
    pub id: String,
    /// Detailed prompt / instructions for the task to execute
    pub prompt: String,
    /// Human-friendly display name (e.g. "Unit Test Auth Module")
    pub name: Option<String>,
    /// Category or suite grouping (e.g. "regression", "backend", "e2e")
    pub category: Option<String>,
    /// Tags for categorizing the task (e.g. ["auth", "security", "rust"])
    pub tags: Option<Vec<String>>,
    /// Whether the task accepts additional instructions at runtime
    pub dynamic_prompt: Option<bool>,
    /// Optional 5-field cron expression for scheduled runs (e.g. "0 3 * * *")
    pub cron: Option<String>,
    /// Optional shell command to verify whether the task succeeded (exits with 0)
    pub verifier_command: Option<String>,
    /// Optional maximum number of turns for the task execution
    pub max_turns: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateTaskParams {
    /// Unique task ID slug of the task to update
    pub task_id: String,
    /// New prompt / instructions (omit to leave unchanged)
    pub prompt: Option<String>,
    /// New display name (omit to leave unchanged)
    pub name: Option<String>,
    /// New category (omit to leave unchanged)
    pub category: Option<String>,
    /// New tags (omit to leave unchanged)
    pub tags: Option<Vec<String>>,
    /// Whether the task accepts dynamic prompt (omit to leave unchanged)
    pub dynamic_prompt: Option<bool>,
    /// New cron schedule (omit to leave unchanged)
    pub cron: Option<String>,
    /// New verifier command (omit to leave unchanged)
    pub verifier_command: Option<String>,
    /// New max turns (omit to leave unchanged)
    pub max_turns: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunTaskParams {
    /// Unique task ID slug of the task to start/run
    pub task_id: String,
    /// Additional instructions for this run (if task accepts dynamic prompt)
    pub extra_prompt: Option<String>,
    /// Override max turns for this run
    pub max_turns: Option<usize>,
    /// Whether to wait for task completion (true) or launch in background (false, default)
    pub wait: Option<bool>,
}

pub struct ProjectTasksClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
}

impl ProjectTasksClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Project Tasks"),
            )
            .with_instructions(
                indoc! {r#"
                Provides tools to view, create, modify, and start project tasks for the current project.
                Use these tools when the user asks to inspect, manage, edit, or execute project tasks.
            "#}
                .to_string(),
            );

        Ok(Self { info, context })
    }

    async fn resolve_project_slug(&self, session_id: &str) -> Result<String, String> {
        let session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|e| format!("Failed to read session: {e}"))?;

        if let Some(slug) = session.project_id.filter(|s| !s.is_empty()) {
            return Ok(slug);
        }

        // Fallback: match session.working_dir to known projects
        let projects = crate::sources::list_sources(
            Some(openduck_sdk_types::custom_requests::SourceType::Project),
            None,
            false,
        )
        .map_err(|e| format!("Failed to list project sources: {e}"))?;

        for entry in projects {
            let slug = entry.name.strip_prefix("projects/").unwrap_or(&entry.name);
            let working_dirs = crate::sources::project_working_dirs(slug);
            if working_dirs
                .iter()
                .any(|d| std::path::Path::new(d) == session.working_dir.as_path())
            {
                return Ok(slug.to_string());
            }
        }

        Err("Current chat session is not linked to any project. Please select or open a project first.".to_string())
    }

    async fn handle_list_tasks(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<ContentBlock>, String> {
        let slug = self.resolve_project_slug(session_id).await?;
        let params: ListTasksParams = if let Some(args) = arguments {
            serde_json::from_value(serde_json::Value::Object(args))
                .map_err(|e| format!("Invalid arguments for list_tasks: {e}"))?
        } else {
            ListTasksParams {
                query: None,
                tag: None,
            }
        };

        let Json(mut tasks) = list_project_tasks(AxumPath(slug.clone()))
            .await
            .map_err(|e| format!("Failed to list tasks for project '{slug}': {:?}", e))?;

        if let Some(ref q) = params.query {
            let lower = q.trim().to_lowercase();
            if !lower.is_empty() {
                tasks.retain(|t| {
                    t.id.to_lowercase().contains(&lower)
                        || t.name.to_lowercase().contains(&lower)
                        || t.category
                            .as_ref()
                            .map(|c| c.to_lowercase().contains(&lower))
                            .unwrap_or(false)
                        || t.prompt_preview
                            .as_ref()
                            .map(|p| p.to_lowercase().contains(&lower))
                            .unwrap_or(false)
                });
            }
        }

        if let Some(ref tag) = params.tag {
            let lower = tag.trim().to_lowercase();
            if !lower.is_empty() {
                tasks.retain(|t| t.tags.iter().any(|item| item.to_lowercase() == lower));
            }
        }

        let json = serde_json::to_string_pretty(&tasks)
            .map_err(|e| format!("Failed to serialize task list: {e}"))?;

        let mut summary = format!(
            "Found {} project task(s) in project '{}':\n\n",
            tasks.len(),
            slug
        );
        for t in &tasks {
            let name_label = if t.name.is_empty() || t.name == t.id {
                t.id.clone()
            } else {
                format!("{} ({})", t.name, t.id)
            };
            let running_str = if t.currently_running {
                " [RUNNING]"
            } else {
                ""
            };
            let cron_str = t
                .cron
                .as_deref()
                .map(|c| format!(" · cron: {c}"))
                .unwrap_or_default();
            summary.push_str(&format!(
                "- **{}**{}{}\n",
                name_label, running_str, cron_str
            ));
            if let Some(ref preview) = t.prompt_preview {
                summary.push_str(&format!("  Preview: {}\n", preview));
            }
            if !t.tags.is_empty() {
                summary.push_str(&format!("  Tags: {}\n", t.tags.join(", ")));
            }
        }

        Ok(vec![
            ContentBlock::text(summary),
            ContentBlock::text(format!("```json\n{}\n```", json)),
        ])
    }

    async fn handle_get_task(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<ContentBlock>, String> {
        let slug = self.resolve_project_slug(session_id).await?;
        let args = arguments.ok_or("Missing arguments")?;
        let params: GetTaskParams = serde_json::from_value(serde_json::Value::Object(args))
            .map_err(|e| format!("Invalid arguments for get_task: {e}"))?;

        let Json(task) = get_project_task(AxumPath((slug.clone(), params.task_id.clone())))
            .await
            .map_err(|e| {
                format!(
                    "Failed to get task '{}' in project '{slug}': {:?}",
                    params.task_id, e
                )
            })?;

        let yaml = serde_yaml::to_string(&task)
            .map_err(|e| format!("Failed to serialize task definition: {e}"))?;

        let text = format!(
            "# Task: {}\n\n- ID: `{}`\n- Name: {}\n- Category: {}\n- Tags: {}\n- Dynamic Prompt: {}\n- Cron: {}\n\n## Prompt\n{}\n\n```yaml\n{}\n```",
            task.name.as_deref().unwrap_or(&task.id),
            task.id,
            task.name.as_deref().unwrap_or("(none)"),
            task.category.as_deref().unwrap_or("(none)"),
            if task.tags.is_empty() { "(none)".to_string() } else { task.tags.join(", ") },
            task.dynamic_prompt,
            task.cron.as_deref().unwrap_or("(none)"),
            task.prompt,
            yaml
        );

        Ok(vec![ContentBlock::text(text)])
    }

    async fn handle_create_task(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<ContentBlock>, String> {
        let slug = self.resolve_project_slug(session_id).await?;
        let args = arguments.ok_or("Missing arguments")?;
        let params: CreateTaskParams = serde_json::from_value(serde_json::Value::Object(args))
            .map_err(|e| format!("Invalid arguments for create_task: {e}"))?;

        let verifier = params
            .verifier_command
            .map(|cmd| TaskVerifierSpec::SimpleCommand {
                command: cmd,
                expected_exit_code: Some(0),
                expected_stdout: None,
            });

        let task_def = ProjectTaskDefinition {
            id: params.id.clone(),
            name: params.name,
            category: params.category,
            tags: params.tags.unwrap_or_default(),
            prompt: params.prompt,
            environment: None,
            verifier,
            max_turns: params.max_turns,
            timeout_seconds: None,
            cron: params.cron,
            schedule_paused: false,
            dynamic_prompt: params.dynamic_prompt.unwrap_or(false),
            subtasks: None,
        };

        let (_status, Json(created)) = create_project_task(AxumPath(slug.clone()), Json(task_def))
            .await
            .map_err(|e| format!("Failed to create task in project '{slug}': {:?}", e))?;

        let text = format!(
            "Successfully created project task `{}` in project '{}'. Display name: {}",
            created.id,
            slug,
            created.name.as_deref().unwrap_or(&created.id)
        );

        Ok(vec![ContentBlock::text(text)])
    }

    async fn handle_update_task(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<ContentBlock>, String> {
        let slug = self.resolve_project_slug(session_id).await?;
        let args = arguments.ok_or("Missing arguments")?;
        let params: UpdateTaskParams = serde_json::from_value(serde_json::Value::Object(args))
            .map_err(|e| format!("Invalid arguments for update_task: {e}"))?;

        let Json(mut task) = get_project_task(AxumPath((slug.clone(), params.task_id.clone())))
            .await
            .map_err(|e| {
                format!(
                    "Failed to find task '{}' in project '{slug}': {:?}",
                    params.task_id, e
                )
            })?;

        if let Some(p) = params.prompt {
            task.prompt = p;
        }
        if let Some(n) = params.name {
            task.name = Some(n);
        }
        if let Some(c) = params.category {
            task.category = Some(c);
        }
        if let Some(tags) = params.tags {
            task.tags = tags;
        }
        if let Some(dp) = params.dynamic_prompt {
            task.dynamic_prompt = dp;
        }
        if let Some(cron) = params.cron {
            task.cron = if cron.trim().is_empty() {
                None
            } else {
                Some(cron)
            };
        }
        if let Some(cmd) = params.verifier_command {
            task.verifier = if cmd.trim().is_empty() {
                None
            } else {
                Some(TaskVerifierSpec::SimpleCommand {
                    command: cmd,
                    expected_exit_code: Some(0),
                    expected_stdout: None,
                })
            };
        }
        if let Some(mt) = params.max_turns {
            task.max_turns = Some(mt);
        }

        let Json(updated) =
            update_project_task(AxumPath((slug.clone(), params.task_id.clone())), Json(task))
                .await
                .map_err(|e| {
                    format!(
                        "Failed to update task '{}' in project '{slug}': {:?}",
                        params.task_id, e
                    )
                })?;

        let text = format!(
            "Successfully updated project task `{}` in project '{}'. Display name: {}",
            updated.id,
            slug,
            updated.name.as_deref().unwrap_or(&updated.id)
        );

        Ok(vec![ContentBlock::text(text)])
    }

    async fn handle_run_task(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<ContentBlock>, String> {
        let slug = self.resolve_project_slug(session_id).await?;
        let args = arguments.ok_or("Missing arguments")?;
        let params: RunTaskParams = serde_json::from_value(serde_json::Value::Object(args))
            .map_err(|e| format!("Invalid arguments for run_task: {e}"))?;

        let req = ProjectTaskRunRequest {
            extra_prompt: params.extra_prompt,
            max_turns: params.max_turns,
            ..Default::default()
        };

        if params.wait.unwrap_or(false) {
            let Json(res) =
                run_project_task(AxumPath((slug.clone(), params.task_id.clone())), Json(req))
                    .await
                    .map_err(|e| {
                        format!(
                            "Failed to execute task '{}' in project '{slug}': {:?}",
                            params.task_id, e
                        )
                    })?;

            let answer = res
                .final_answer
                .unwrap_or_else(|| "(no final answer)".to_string());
            let summary = format!(
                "Task `{}` completed in project '{}'.\n- Status: `{:?}`\n- Duration: {} ms\n- Step Count: {}\n- Tool Calls: {}\n\n### Final Output:\n{}",
                res.task_id, slug, res.status, res.duration_ms, res.step_count, res.tool_calls_count, answer
            );
            Ok(vec![ContentBlock::text(summary)])
        } else {
            let _rx = launch_project_task(&slug, &params.task_id, req)
                .await
                .map_err(|e| {
                    format!(
                        "Failed to launch task '{}' in project '{slug}': {e}",
                        params.task_id
                    )
                })?;

            let text = format!(
                "Project task `{}` has been launched and is running in project '{}'. You can inspect its progress in the Harness/History tab.",
                params.task_id, slug
            );
            Ok(vec![ContentBlock::text(text)])
        }
    }

    fn get_tools() -> Vec<Tool> {
        let list_schema = schema_for!(ListTasksParams);
        let list_value = serde_json::to_value(list_schema).expect("schema serialize");

        let get_schema = schema_for!(GetTaskParams);
        let get_value = serde_json::to_value(get_schema).expect("schema serialize");

        let create_schema = schema_for!(CreateTaskParams);
        let create_value = serde_json::to_value(create_schema).expect("schema serialize");

        let update_schema = schema_for!(UpdateTaskParams);
        let update_value = serde_json::to_value(update_schema).expect("schema serialize");

        let run_schema = schema_for!(RunTaskParams);
        let run_value = serde_json::to_value(run_schema).expect("schema serialize");

        vec![
            Tool::new(
                "list_project_tasks".to_string(),
                "List all tasks defined in the current project, with their ID, name, category, tags, and cron schedule."
                    .to_string(),
                list_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("List Project Tasks".to_string()),
                Some(true),
                Some(false),
                Some(false),
                Some(false),
            )),
            Tool::new(
                "get_project_task".to_string(),
                "Get the full definition, instructions, and configuration of a specific project task by its ID."
                    .to_string(),
                get_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("Get Project Task".to_string()),
                Some(true),
                Some(false),
                Some(false),
                Some(false),
            )),
            Tool::new(
                "create_project_task".to_string(),
                "Create a new project task in the current project with an ID, prompt instructions, name, tags, and optional cron schedule or verifier."
                    .to_string(),
                create_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("Create Project Task".to_string()),
                Some(false),
                Some(true),
                Some(false),
                Some(false),
            )),
            Tool::new(
                "update_project_task".to_string(),
                "Update/modify an existing project task's prompt, name, category, tags, dynamic prompt setting, cron schedule, or verifier."
                    .to_string(),
                update_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("Update Project Task".to_string()),
                Some(false),
                Some(true),
                Some(false),
                Some(false),
            )),
            Tool::new(
                "run_project_task".to_string(),
                "Start/run a project task in the current project. Optionally pass extra prompt instructions and specify whether to wait for completion."
                    .to_string(),
                run_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations::from_raw(
                Some("Run Project Task".to_string()),
                Some(false),
                Some(true),
                Some(false),
                Some(false),
            )),
        ]
    }
}

#[async_trait]
impl McpClientTrait for ProjectTasksClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let session_id = &ctx.session_id;
        let content = match name {
            "list_project_tasks" => self.handle_list_tasks(session_id, arguments).await,
            "get_project_task" => self.handle_get_task(session_id, arguments).await,
            "create_project_task" => self.handle_create_task(session_id, arguments).await,
            "update_project_task" => self.handle_update_task(session_id, arguments).await,
            "run_project_task" => self.handle_run_task(session_id, arguments).await,
            _ => Err(format!("Unknown tool: {}", name)),
        };

        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Error: {}",
                error
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_tasks_tool_definitions() {
        let tools = ProjectTasksClient::get_tools();
        let tool_names: Vec<String> = tools.into_iter().map(|t| t.name.to_string()).collect();
        assert_eq!(
            tool_names,
            vec![
                "list_project_tasks",
                "get_project_task",
                "create_project_task",
                "update_project_task",
                "run_project_task",
            ]
        );
    }
}
