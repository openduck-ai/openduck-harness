use crate::notification::report::{TaskExecutionReport, TaskStatus};

pub fn render_subject(report: &TaskExecutionReport) -> String {
    let status_tag = match report.status {
        TaskStatus::Succeeded => "SUCCESS",
        TaskStatus::Failed => "FAILED",
        TaskStatus::Cancelled => "CANCELLED",
    };

    let duration_str = format_duration(report.duration_seconds);
    format!(
        "[{status_tag}] Task '{}' completed ({duration_str})",
        report.job_name
    )
}

pub fn render_text_body(report: &TaskExecutionReport) -> String {
    let mut body = String::new();
    body.push_str(&format!("Task Name: {}\n", report.job_name));
    body.push_str(&format!("Task ID: {}\n", report.job_id));
    body.push_str(&format!("Session ID: {}\n", report.session_id));
    body.push_str(&format!("Status: {}\n", report.status));
    body.push_str(&format!("Trigger: {}\n", report.trigger_type));
    body.push_str(&format!(
        "Started At: {}\n",
        report.started_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    body.push_str(&format!(
        "Finished At: {}\n",
        report.finished_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    body.push_str(&format!(
        "Duration: {}\n",
        format_duration(report.duration_seconds)
    ));

    if let Some(tokens) = report.total_tokens_used {
        body.push_str(&format!("Tokens Used: {}\n", tokens));
    }
    if let Some(project) = &report.project_id {
        body.push_str(&format!("Project: {}\n", project));
    }
    if let Some(model) = &report.model_name {
        body.push_str(&format!("Model: {}\n", model));
    }

    if let Some(err) = &report.error_message {
        body.push_str("\n--- Error Details ---\n");
        body.push_str(err);
        body.push('\n');
    }

    if let Some(summary) = &report.summary_result {
        body.push_str("\n--- Result Summary ---\n");
        body.push_str(summary);
        body.push('\n');
    }

    if let Some(log_url) = &report.log_url {
        body.push_str(&format!("\nLogs: {}\n", log_url));
    }

    body
}

pub fn render_html_body(report: &TaskExecutionReport) -> String {
    let (badge_color, badge_text) = match report.status {
        TaskStatus::Succeeded => ("#10b981", "SUCCESS"),
        TaskStatus::Failed => ("#ef4444", "FAILED"),
        TaskStatus::Cancelled => ("#f59e0b", "CANCELLED"),
    };

    let tokens_str = report
        .total_tokens_used
        .map(|t| t.to_string())
        .unwrap_or_else(|| "N/A".to_string());

    let model_str = report.model_name.as_deref().unwrap_or("Default");

    let error_section = if let Some(err) = &report.error_message {
        format!(
            r#"<div style="margin-top: 20px; padding: 15px; background-color: #fef2f2; border-left: 4px solid #ef4444; border-radius: 4px;">
                <h4 style="margin: 0 0 8px 0; color: #991b1b; font-size: 14px;">Error Details</h4>
                <pre style="margin: 0; color: #7f1d1d; font-family: monospace; font-size: 13px; white-space: pre-wrap; word-break: break-all;">{}</pre>
            </div>"#,
            html_escape(err)
        )
    } else {
        String::new()
    };

    let summary_section = if let Some(summary) = &report.summary_result {
        format!(
            r#"<div style="margin-top: 20px; padding: 15px; background-color: #f8fafc; border: 1px solid #e2e8f0; border-radius: 6px;">
                <h4 style="margin: 0 0 8px 0; color: #334155; font-size: 14px;">Task Output Summary</h4>
                <div style="margin: 0; color: #1e293b; font-size: 14px; line-height: 1.6; white-space: pre-wrap;">{}</div>
            </div>"#,
            html_escape(summary)
        )
    } else {
        String::new()
    };

    let log_section = if let Some(log_url) = &report.log_url {
        format!(
            r#"<div style="margin-top: 15px;">
                <a href="{}" style="color: #2563eb; font-size: 13px; text-decoration: none;">View Logs / Session &rarr;</a>
            </div>"#,
            html_escape(log_url)
        )
    } else {
        String::new()
    };

    let project_row = if let Some(proj) = &report.project_id {
        format!(
            r#"<tr>
                <td style="padding: 8px 0; color: #64748b; font-size: 13px;">Project</td>
                <td style="padding: 8px 0; color: #0f172a; font-size: 13px; font-weight: 500;">{}</td>
            </tr>"#,
            html_escape(proj)
        )
    } else {
        String::new()
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="margin: 0; padding: 20px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; background-color: #f1f5f9; color: #0f172a;">
    <div style="max-width: 600px; margin: 0 auto; background-color: #ffffff; border-radius: 8px; overflow: hidden; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1);">
        <div style="padding: 24px 28px; border-bottom: 1px solid #e2e8f0;">
            <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px;">
                <span style="display: inline-block; padding: 4px 10px; font-size: 12px; font-weight: 700; color: #ffffff; background-color: {badge_color}; border-radius: 12px;">
                    {badge_text}
                </span>
                <span style="font-size: 13px; color: #64748b;">Trigger: {trigger_type}</span>
            </div>
            <h2 style="margin: 0 0 6px 0; font-size: 20px; font-weight: 600; color: #0f172a;">{job_name}</h2>
            <div style="font-size: 12px; color: #94a3b8;">Job ID: {job_id} | Session: {session_id}</div>
        </div>

        <div style="padding: 24px 28px;">
            <table style="width: 100%; border-collapse: collapse; margin-bottom: 16px;">
                <tr>
                    <td style="padding: 8px 0; color: #64748b; font-size: 13px; width: 35%;">Started At</td>
                    <td style="padding: 8px 0; color: #0f172a; font-size: 13px; font-weight: 500;">{started_at}</td>
                </tr>
                <tr>
                    <td style="padding: 8px 0; color: #64748b; font-size: 13px;">Finished At</td>
                    <td style="padding: 8px 0; color: #0f172a; font-size: 13px; font-weight: 500;">{finished_at}</td>
                </tr>
                <tr>
                    <td style="padding: 8px 0; color: #64748b; font-size: 13px;">Duration</td>
                    <td style="padding: 8px 0; color: #0f172a; font-size: 13px; font-weight: 500;">{duration}</td>
                </tr>
                {project_row}
                <tr>
                    <td style="padding: 8px 0; color: #64748b; font-size: 13px;">Tokens Used</td>
                    <td style="padding: 8px 0; color: #0f172a; font-size: 13px; font-weight: 500;">{tokens_str}</td>
                </tr>
                <tr>
                    <td style="padding: 8px 0; color: #64748b; font-size: 13px;">Model</td>
                    <td style="padding: 8px 0; color: #0f172a; font-size: 13px; font-weight: 500;">{model_str}</td>
                </tr>
            </table>

            {error_section}
            {summary_section}
            {log_section}
        </div>

        <div style="padding: 16px 28px; background-color: #f8fafc; border-top: 1px solid #e2e8f0; font-size: 12px; color: #94a3b8; text-align: center;">
            Sent by OpenDuck Automated Task Notification Service
        </div>
    </div>
</body>
</html>"#,
        badge_color = badge_color,
        badge_text = badge_text,
        trigger_type = html_escape(&report.trigger_type),
        job_name = html_escape(&report.job_name),
        job_id = html_escape(&report.job_id),
        session_id = html_escape(&report.session_id),
        started_at = report.started_at.format("%Y-%m-%d %H:%M:%S UTC"),
        finished_at = report.finished_at.format("%Y-%m-%d %H:%M:%S UTC"),
        duration = format_duration(report.duration_seconds),
        tokens_str = tokens_str,
        model_str = html_escape(model_str),
        error_section = error_section,
        summary_section = summary_section,
        log_section = log_section,
    )
}

fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        format!("{mins}m {secs}s")
    } else {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        format!("{hours}h {mins}m")
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
