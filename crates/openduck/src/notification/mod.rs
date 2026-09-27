pub mod config;
pub mod email;
pub mod report;
pub mod service;

pub use config::{EmailConfig, NotificationConfig, NotificationRule, SmtpConfig, TriggerCondition};
pub use email::{EmailMessage, EmailProvider};
pub use report::{TaskExecutionReport, TaskStatus};
pub use service::NotificationService;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::email::smtp::MockEmailProvider;
    use chrono::Utc;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_notification_service_triggers_email_on_success() {
        let mock_provider = Arc::new(MockEmailProvider::new());
        let config = NotificationConfig {
            email: EmailConfig {
                enabled: true,
                smtp: Some(SmtpConfig {
                    host: "smtp.example.com".into(),
                    port: 587,
                    use_tls: true,
                    username: None,
                    password: None,
                    from: "noreply@example.com".into(),
                }),
                rules: vec![NotificationRule {
                    trigger_on: TriggerCondition::OnSuccess,
                    recipients: vec!["dev@example.com".into()],
                    min_duration_seconds: None,
                    projects: None,
                }],
                default_recipients: vec![],
                projects: std::collections::HashMap::new(),
            },
        };

        let service = NotificationService::with_provider(config, mock_provider.clone());

        let report = TaskExecutionReport {
            job_id: "job-123".into(),
            job_name: "Daily Code Review".into(),
            session_id: "sess-456".into(),
            trigger_type: "cron".into(),
            status: TaskStatus::Succeeded,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            duration_seconds: 42,
            total_tokens_used: Some(1500),
            model_name: Some("claude-3-5-sonnet".into()),
            project_id: None,
            summary_result: Some("All tasks completed without error.".into()),
            error_message: None,
            log_url: None,
        };

        service.handle_task_completion(report).await.unwrap();

        let sent = mock_provider.messages();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to, vec!["dev@example.com".to_string()]);
        assert!(sent[0].subject.contains("[SUCCESS]"));
        assert!(sent[0].subject.contains("Daily Code Review"));
        assert!(sent[0].text_body.contains("Tokens Used: 1500"));
        assert!(sent[0]
            .html_body
            .as_ref()
            .unwrap()
            .contains("Daily Code Review"));
    }

    #[tokio::test]
    async fn test_notification_service_triggers_email_on_failure_only() {
        let mock_provider = Arc::new(MockEmailProvider::new());
        let config = NotificationConfig {
            email: EmailConfig {
                enabled: true,
                smtp: Some(SmtpConfig {
                    host: "smtp.example.com".into(),
                    port: 587,
                    use_tls: true,
                    username: None,
                    password: None,
                    from: "noreply@example.com".into(),
                }),
                rules: vec![NotificationRule {
                    trigger_on: TriggerCondition::OnFailure,
                    recipients: vec!["alerts@example.com".into()],
                    min_duration_seconds: None,
                    projects: None,
                }],
                default_recipients: vec![],
                projects: std::collections::HashMap::new(),
            },
        };

        let service = NotificationService::with_provider(config, mock_provider.clone());

        // Succeeded report -> should NOT send email
        let success_report = TaskExecutionReport {
            job_id: "job-123".into(),
            job_name: "Backup Database".into(),
            session_id: "sess-456".into(),
            trigger_type: "cron".into(),
            status: TaskStatus::Succeeded,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            duration_seconds: 10,
            total_tokens_used: None,
            model_name: None,
            project_id: None,
            summary_result: None,
            error_message: None,
            log_url: None,
        };

        service
            .handle_task_completion(success_report)
            .await
            .unwrap();
        assert_eq!(mock_provider.messages().len(), 0);

        // Failed report -> SHOULD send email
        let failed_report = TaskExecutionReport {
            job_id: "job-123".into(),
            job_name: "Backup Database".into(),
            session_id: "sess-456".into(),
            trigger_type: "cron".into(),
            status: TaskStatus::Failed,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            duration_seconds: 15,
            total_tokens_used: None,
            model_name: None,
            project_id: None,
            summary_result: None,
            error_message: Some("Connection timed out to remote database".into()),
            log_url: None,
        };

        service.handle_task_completion(failed_report).await.unwrap();
        let sent = mock_provider.messages();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to, vec!["alerts@example.com".to_string()]);
        assert!(sent[0].subject.contains("[FAILED]"));
        assert!(sent[0]
            .text_body
            .contains("Connection timed out to remote database"));
    }

    #[test]
    fn test_deserialize_notification_config_from_yaml() {
        let yaml = r#"
email:
  enabled: true
  smtp:
    host: "smtp.mailgun.org"
    port: 465
    use_tls: true
    username: "user@domain.com"
    password: "secret_password"
    from: "OpenDuck <noreply@domain.com>"
  default_recipients:
    - "admin@domain.com"
  rules:
    - trigger_on: "on_failure"
      recipients:
        - "oncall@domain.com"
      min_duration_seconds: 30
"#;

        let config: NotificationConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.email.enabled);
        let smtp = config.email.smtp.as_ref().unwrap();
        assert_eq!(smtp.host, "smtp.mailgun.org");
        assert_eq!(smtp.port, 465);
        assert_eq!(smtp.username.as_deref(), Some("user@domain.com"));
        assert_eq!(config.email.default_recipients, vec!["admin@domain.com"]);
        assert_eq!(config.email.rules.len(), 1);
        assert_eq!(
            config.email.rules[0].trigger_on,
            TriggerCondition::OnFailure
        );
        assert_eq!(config.email.rules[0].min_duration_seconds, Some(30));
    }

    #[test]
    fn test_notification_config_entry_variants() {
        use crate::notification::config::NotificationConfigEntry;

        // String path variant
        let yaml1 = r#""notifications.yaml""#;
        let entry1: NotificationConfigEntry = serde_yaml::from_str(yaml1).unwrap();
        assert!(
            matches!(entry1, NotificationConfigEntry::FilePath(f) if f == "notifications.yaml")
        );

        // File object variant
        let yaml2 = r#"file: "custom_notify.yaml""#;
        let entry2: NotificationConfigEntry = serde_yaml::from_str(yaml2).unwrap();
        assert!(
            matches!(entry2, NotificationConfigEntry::FileObject { file } if file == "custom_notify.yaml")
        );

        // Inline object variant
        let yaml3 = r#"
email:
  enabled: true
  default_recipients:
    - "test@example.com"
"#;
        let entry3: NotificationConfigEntry = serde_yaml::from_str(yaml3).unwrap();
        assert!(matches!(entry3, NotificationConfigEntry::Inline(cfg) if cfg.email.enabled));
    }

    #[test]
    fn test_notification_config_from_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_notification.yaml");

        let content = r#"
email:
  enabled: true
  smtp:
    host: "smtp.example.com"
    port: 587
    use_tls: true
    from: "test@example.com"
  default_recipients:
    - "dev@example.com"
"#;
        std::fs::write(&file_path, content).unwrap();

        let loaded = NotificationConfig::from_file(&file_path).unwrap();
        assert!(loaded.email.enabled);
        assert_eq!(loaded.email.default_recipients, vec!["dev@example.com"]);
    }

    #[tokio::test]
    async fn test_project_based_email_recipients() {
        let yaml = r#"
email:
  enabled: true
  smtp:
    host: "smtp.example.com"
    port: 587
    use_tls: true
    from: "noreply@example.com"
  default_recipients:
    - "admin@example.com"
  projects:
    backend_service:
      - "backend-devs@example.com"
    frontend_app:
      recipients:
        - "frontend-team@example.com"
      rules:
        - trigger_on: "on_failure"
          recipients:
            - "frontend-oncall@example.com"
"#;

        let config: NotificationConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.email.projects.len(), 2);

        let mock_provider = Arc::new(MockEmailProvider::new());
        let service = NotificationService::with_provider(config, mock_provider.clone());

        // 1. Task for backend_service (matches list recipients)
        let backend_report = TaskExecutionReport {
            job_id: "job-1".into(),
            job_name: "Backend CI".into(),
            session_id: "sess-1".into(),
            trigger_type: "cron".into(),
            status: TaskStatus::Succeeded,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            duration_seconds: 20,
            total_tokens_used: None,
            model_name: None,
            project_id: Some("backend_service".into()),
            summary_result: Some("Backend build passed".into()),
            error_message: None,
            log_url: None,
        };

        service
            .handle_task_completion(backend_report)
            .await
            .unwrap();
        let sent = mock_provider.messages();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to, vec!["backend-devs@example.com".to_string()]);
        assert!(sent[0].text_body.contains("Project: backend_service"));

        // 2. Task for frontend_app on failure (matches frontend-oncall + frontend-team)
        let frontend_fail = TaskExecutionReport {
            job_id: "job-2".into(),
            job_name: "Frontend E2E".into(),
            session_id: "sess-2".into(),
            trigger_type: "cron".into(),
            status: TaskStatus::Failed,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            duration_seconds: 12,
            total_tokens_used: None,
            model_name: None,
            project_id: Some("frontend_app".into()),
            summary_result: None,
            error_message: Some("Playwright tests failed".into()),
            log_url: None,
        };

        service.handle_task_completion(frontend_fail).await.unwrap();
        let sent = mock_provider.messages();
        assert_eq!(sent.len(), 2);
        assert!(sent[1]
            .to
            .contains(&"frontend-oncall@example.com".to_string()));

        // 3. Task without project_id -> fallbacks to default_recipients
        let global_task = TaskExecutionReport {
            job_id: "job-3".into(),
            job_name: "Global Maintenance".into(),
            session_id: "sess-3".into(),
            trigger_type: "manual".into(),
            status: TaskStatus::Succeeded,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            duration_seconds: 5,
            total_tokens_used: None,
            model_name: None,
            project_id: None,
            summary_result: None,
            error_message: None,
            log_url: None,
        };

        service.handle_task_completion(global_task).await.unwrap();
        let sent = mock_provider.messages();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[2].to, vec!["admin@example.com".to_string()]);
    }
}
