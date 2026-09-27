use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;

use super::config::{NotificationConfig, NotificationRule, TriggerCondition};
use super::email::smtp::SmtpEmailProvider;
use super::email::template::{render_html_body, render_subject, render_text_body};
use super::email::{EmailMessage, EmailProvider};
use super::report::{TaskExecutionReport, TaskStatus};

#[derive(Clone)]
pub struct NotificationService {
    config: NotificationConfig,
    provider: Option<Arc<dyn EmailProvider>>,
}

impl NotificationService {
    pub fn new(config: NotificationConfig) -> Self {
        let provider: Option<Arc<dyn EmailProvider>> = if config.email.enabled {
            match config.email.smtp.clone() {
                Some(smtp_cfg) => {
                    tracing::info!(
                        host = %smtp_cfg.host,
                        port = smtp_cfg.port,
                        use_tls = smtp_cfg.use_tls,
                        from = %smtp_cfg.from,
                        rule_count = config.email.rules.len(),
                        default_recipient_count = config.email.default_recipients.len(),
                        project_count = config.email.projects.len(),
                        "SMTP email provider configured for notifications"
                    );
                    Some(Arc::new(SmtpEmailProvider::new(smtp_cfg)) as Arc<dyn EmailProvider>)
                }
                None => {
                    tracing::info!(
                        "Email notifications enabled but no SMTP config provided; emails will be skipped"
                    );
                    None
                }
            }
        } else {
            tracing::info!("Email notifications are disabled");
            None
        };

        Self { config, provider }
    }

    pub fn with_provider(config: NotificationConfig, provider: Arc<dyn EmailProvider>) -> Self {
        Self {
            config,
            provider: Some(provider),
        }
    }

    pub fn from_env() -> Self {
        let config = NotificationConfig::from_env();
        Self::new(config)
    }

    pub fn load() -> Self {
        let config = NotificationConfig::load();
        Self::new(config)
    }

    fn should_trigger(rule: &NotificationRule, report: &TaskExecutionReport) -> bool {
        if let Some(projects) = &rule.projects {
            let matched = report
                .project_id
                .as_ref()
                .map(|p| projects.iter().any(|proj| proj.eq_ignore_ascii_case(p)))
                .unwrap_or(false);
            if !matched {
                tracing::debug!(
                    job_id = %report.job_id,
                    rule_projects = ?projects,
                    task_project = report.project_id.as_deref(),
                    "Notification rule skipped: project filter did not match"
                );
                return false;
            }
        }

        if let Some(min_duration) = rule.min_duration_seconds {
            if report.duration_seconds < min_duration {
                tracing::debug!(
                    job_id = %report.job_id,
                    duration_seconds = report.duration_seconds,
                    min_duration_seconds = min_duration,
                    "Notification rule skipped: task duration is less than min_duration_seconds"
                );
                return false;
            }
        }

        let condition_met = match rule.trigger_on {
            TriggerCondition::Always => true,
            TriggerCondition::OnSuccess => report.status == TaskStatus::Succeeded,
            TriggerCondition::OnFailure => report.status == TaskStatus::Failed,
            TriggerCondition::OnStatusChange => true,
        };

        if !condition_met {
            tracing::debug!(
                job_id = %report.job_id,
                trigger_on = ?rule.trigger_on,
                task_status = %report.status,
                "Notification rule skipped: task status does not match trigger condition"
            );
            false
        } else {
            true
        }
    }

    pub async fn handle_task_completion(&self, report: TaskExecutionReport) -> Result<()> {
        tracing::info!(
            job_id = %report.job_id,
            job_name = %report.job_name,
            session_id = %report.session_id,
            status = %report.status,
            trigger = %report.trigger_type,
            duration_seconds = report.duration_seconds,
            project_id = report.project_id.as_deref(),
            model_name = report.model_name.as_deref(),
            tokens_used = report.total_tokens_used,
            "Handling task completion notification"
        );

        if !self.config.email.enabled {
            tracing::info!(
                job_id = %report.job_id,
                "Skipping email notification: email is disabled in configuration (email.enabled is false)"
            );
            return Ok(());
        }

        let provider = match &self.provider {
            Some(p) => p,
            None => {
                tracing::warn!(
                    job_id = %report.job_id,
                    "Skipping email notification: email notifications enabled but no SMTP provider configured (missing host, port, or from address in SMTP config)"
                );
                return Ok(());
            }
        };

        let mut recipients = HashSet::new();

        // 1. Check project-specific configurations if project_id is present
        if let Some(ref project_id) = report.project_id {
            // Check project metadata saved via UI / Hub / Control API
            let ui_project_recipients = crate::sources::project_email_recipients(project_id);
            if !ui_project_recipients.is_empty() {
                tracing::info!(
                    job_id = %report.job_id,
                    project_id = %project_id,
                    recipient_count = ui_project_recipients.len(),
                    recipients = ?ui_project_recipients,
                    "Added project UI email recipients"
                );
                for r in ui_project_recipients {
                    recipients.insert(r);
                }
            } else {
                tracing::debug!(
                    job_id = %report.job_id,
                    project_id = %project_id,
                    "No project UI email recipients found"
                );
            }

            // Check project configuration in notifications.yaml / config.yaml
            if let Some(project_cfg) = self
                .config
                .email
                .projects
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(project_id))
                .map(|(_, v)| v)
            {
                tracing::info!(
                    job_id = %report.job_id,
                    project_id = %project_id,
                    project_recipient_count = project_cfg.recipients.len(),
                    project_rule_count = project_cfg.rules.len(),
                    "Evaluating project notification config"
                );
                let mut project_rule_matched = false;
                for (idx, rule) in project_cfg.rules.iter().enumerate() {
                    if Self::should_trigger(rule, &report) {
                        tracing::info!(
                            job_id = %report.job_id,
                            project_id = %project_id,
                            rule_index = idx,
                            trigger_on = ?rule.trigger_on,
                            recipients = ?rule.recipients,
                            "Project notification rule matched"
                        );
                        for r in &rule.recipients {
                            recipients.insert(r.clone());
                        }
                        project_rule_matched = true;
                    }
                }

                // If no specific rules defined for the project, or if default recipients are configured
                if !project_rule_matched || project_cfg.rules.is_empty() {
                    if !project_cfg.recipients.is_empty() {
                        tracing::info!(
                            job_id = %report.job_id,
                            project_id = %project_id,
                            recipients = ?project_cfg.recipients,
                            "Using project default email recipients"
                        );
                        for r in &project_cfg.recipients {
                            recipients.insert(r.clone());
                        }
                    } else {
                        tracing::debug!(
                            job_id = %report.job_id,
                            project_id = %project_id,
                            "Project config has no default recipients"
                        );
                    }
                }
            } else {
                tracing::info!(
                    job_id = %report.job_id,
                    project_id = %project_id,
                    "No YAML project notification config found for this project"
                );
            }
        } else {
            tracing::debug!(
                job_id = %report.job_id,
                "Task report has no project_id; skipping project-level email recipients lookup"
            );
        }

        // 2. Evaluate global notification rules
        tracing::debug!(
            job_id = %report.job_id,
            global_rule_count = self.config.email.rules.len(),
            "Evaluating global notification rules"
        );
        for (idx, rule) in self.config.email.rules.iter().enumerate() {
            if Self::should_trigger(rule, &report) {
                tracing::info!(
                    job_id = %report.job_id,
                    rule_index = idx,
                    trigger_on = ?rule.trigger_on,
                    recipients = ?rule.recipients,
                    min_duration_seconds = rule.min_duration_seconds,
                    "Global notification rule matched"
                );
                for r in &rule.recipients {
                    recipients.insert(r.clone());
                }
            }
        }

        // 3. Fallback to default recipients if no specific recipients were matched
        if recipients.is_empty() {
            if !self.config.email.default_recipients.is_empty() {
                tracing::info!(
                    job_id = %report.job_id,
                    recipients = ?self.config.email.default_recipients,
                    "Using default email notification recipients"
                );
                for r in &self.config.email.default_recipients {
                    recipients.insert(r.clone());
                }
            } else {
                tracing::warn!(
                    job_id = %report.job_id,
                    project_id = report.project_id.as_deref(),
                    status = %report.status,
                    "Skipping email notification: no matching recipients found (no project recipients, no matching rules, and default_recipients is empty)"
                );
                return Ok(());
            }
        }

        let from_addr = self
            .config
            .email
            .smtp
            .as_ref()
            .map(|s| s.from.clone())
            .unwrap_or_else(|| "openduck@localhost".to_string());

        let subject = render_subject(&report);
        let text_body = render_text_body(&report);
        let html_body = Some(render_html_body(&report));

        let message = EmailMessage {
            from: from_addr,
            to: recipients.into_iter().collect(),
            subject,
            text_body,
            html_body,
        };

        tracing::info!(
            job_id = %report.job_id,
            job_name = %report.job_name,
            status = %report.status,
            from = %message.from,
            recipients = ?message.to,
            recipient_count = message.to.len(),
            subject = %message.subject,
            "Sending task completion email notification"
        );

        if let Err(err) = provider.send(message).await {
            tracing::error!(
                job_id = %report.job_id,
                job_name = %report.job_name,
                status = %report.status,
                %err,
                "Failed to send task completion email notification"
            );
            return Err(err);
        }

        tracing::info!(
            job_id = %report.job_id,
            job_name = %report.job_name,
            status = %report.status,
            "Task completion email notification sent successfully"
        );
        Ok(())
    }
}
