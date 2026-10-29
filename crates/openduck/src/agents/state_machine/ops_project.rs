//! Adds the selected project's instructions to inference prompts.

use anyhow::Result;
use async_trait::async_trait;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::Operation;
use crate::conversation::Conversation;
use crate::session::Session;

pub struct ProjectOperation;

#[async_trait]
impl Operation<Session, GooseEffect> for ProjectOperation {
    fn name(&self) -> &'static str {
        "project"
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let Some(project_id) = session.project_id.as_deref() else {
            return Ok(Vec::new());
        };
        let Ok(entry) = crate::sources::read_project(project_id) else {
            return Ok(Vec::new());
        };
        let mut parts = vec![format!("# Project: {}", entry.name)];
        if !entry.description.is_empty() {
            parts.push(entry.description);
        }
        if !entry.content.is_empty() {
            parts.push(entry.content);
        }
        parts.push(
            "## Project Tasks\n\
             You can view, create, modify, and start project tasks for this project using your `project_tasks` tools:\n\
             - `project_tasks__list_tasks`: List all tasks in this project\n\
             - `project_tasks__get_task`: Inspect a specific task's definition\n\
             - `project_tasks__create_task`: Create a new task\n\
             - `project_tasks__update_task`: Update/modify an existing task\n\
             - `project_tasks__run_task`: Start/run a task\n\
             When the user asks you to view, list, create, edit, or run tasks, use these tools directly."
                .to_string(),
        );
        Ok(vec![("project".to_string(), parts.join("\n\n"))])
    }
}
