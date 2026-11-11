use chrono::Utc;

use crate::control::tui_management::{QuotaSnapshotRecord, TuiManagementService};

pub async fn handle_quota_slash_command(params_str: &str) -> String {
    let service = TuiManagementService::global().await;
    let parts: Vec<&str> = params_str.split_whitespace().collect();

    match parts.as_slice() {
        [] => show_quota_overview(&service).await,
        ["refresh"] => {
            let _ = service.refresh_quotas_for_agent("grok").await;
            show_quota_overview(&service).await
        }
        ["refresh", agent] => match service.refresh_quotas_for_agent(agent).await {
            Ok(Some(q)) => format!(
                "✅ Refreshed quota for agent **{}**:\n- Status: **{}**\n- Remaining: **{}%**\n- Reset: {}\n",
                agent,
                q.status,
                q.remaining_percent.unwrap_or(0.0),
                q.reset_at.as_deref().unwrap_or("None")
            ),
            Ok(None) => format!("ℹ️ No quota source available for agent **{}**.", agent),
            Err(e) => format!("❌ Failed to refresh quota for agent **{}**: {:#}", agent, e),
        },
        ["reset", agent] => {
            let normalized = agent.trim().to_ascii_lowercase();
            let now = Utc::now();
            let existing = service
                .store
                .find_quota_by_agent(&normalized)
                .await
                .unwrap_or(None);
            let q_id = existing
                .map(|e| e.id)
                .unwrap_or_else(|| format!("quota_{}", uuid::Uuid::new_v4()));
            let record = QuotaSnapshotRecord {
                id: q_id,
                pool_id: format!("pool_{normalized}"),
                account_id: None,
                agent_kind: Some(normalized.clone()),
                model_pattern: Some("*".to_string()),
                quota_type: "weekly".to_string(),
                unit: "percent".to_string(),
                used_amount: 0.0,
                limit_amount: Some(100.0),
                remaining_percent: Some(100.0),
                window_start: Some(now.to_rfc3339()),
                window_end: None,
                reset_at: None,
                source: "manual_slash_command".to_string(),
                status: "valid".to_string(),
                observed_at: now.to_rfc3339(),
            };
            match service.store.upsert_quota(&record).await {
                Ok(_) => {
                    service.emit_event(
                        "quota_reset",
                        None,
                        None,
                        serde_json::json!({ "agent": normalized, "manual": true }),
                    );
                    format!("✅ Quota for agent **{normalized}** has been reset to **100% (valid)**. The agent is now active and ready to be used.")
                }
                Err(e) => format!("❌ Failed to reset quota for agent **{normalized}**: {:#}", e),
            }
        }
        ["block", agent] | ["suspend", agent] => {
            let normalized = agent.trim().to_ascii_lowercase();
            match service
                .record_quota_exhausted(
                    &normalized,
                    None,
                    "weekly",
                    "Manually suspended via slash command",
                    None,
                )
                .await
            {
                Ok(q) => format!(
                    "🚫 Agent **{normalized}** suspended (quota marked exhausted).\n- Resets at: {}\n- The agent will not be used before the limit resets.",
                    q.reset_at.as_deref().unwrap_or("manual reset required")
                ),
                Err(e) => format!("❌ Failed to suspend agent **{normalized}**: {:#}", e),
            }
        }
        ["set", agent, val_str] => {
            let normalized = agent.trim().to_ascii_lowercase();
            let clean_val = val_str.trim_end_matches('%');
            let Ok(pct) = clean_val.parse::<f64>() else {
                return format!(
                    "❌ Invalid percentage '{}'. Example: `/quota set grok 50` or `/quota set grok 100%`",
                    val_str
                );
            };
            let pct = pct.clamp(0.0, 100.0);
            let now = Utc::now();
            let is_exhausted = pct <= 0.0;
            let existing = service
                .store
                .find_quota_by_agent(&normalized)
                .await
                .unwrap_or(None);
            let q_id = existing
                .map(|e| e.id)
                .unwrap_or_else(|| format!("quota_{}", uuid::Uuid::new_v4()));
            let record = QuotaSnapshotRecord {
                id: q_id,
                pool_id: format!("pool_{normalized}"),
                account_id: None,
                agent_kind: Some(normalized.clone()),
                model_pattern: Some("*".to_string()),
                quota_type: "weekly".to_string(),
                unit: "percent".to_string(),
                used_amount: 100.0 - pct,
                limit_amount: Some(100.0),
                remaining_percent: Some(pct),
                window_start: Some(now.to_rfc3339()),
                window_end: None,
                reset_at: if is_exhausted {
                    Some((now + chrono::Duration::days(7)).to_rfc3339())
                } else {
                    None
                },
                source: "manual_slash_command".to_string(),
                status: if is_exhausted {
                    "exhausted".to_string()
                } else {
                    "valid".to_string()
                },
                observed_at: now.to_rfc3339(),
            };
            match service.store.upsert_quota(&record).await {
                Ok(_) => format!(
                    "✅ Quota for agent **{normalized}** set to **{pct}%** (status: **{}**).",
                    record.status
                ),
                Err(e) => format!("❌ Failed to set quota for agent **{normalized}**: {:#}", e),
            }
        }
        _ => format!(
            "**Usage / Quota Management Commands:**\n\n\
             - `/quota` or `/usage` — View current agent quotas, usage & limits\n\
             - `/quota refresh [agent]` — Refresh quota from provider/logs\n\
             - `/quota reset <agent>` — Reset agent quota to 100% valid and unblock\n\
             - `/quota set <agent> <0-100%>` — Set remaining quota percentage\n\
             - `/quota block <agent>` — Manually suspend agent until reset\n"
        ),
    }
}

pub async fn show_quota_overview(service: &TuiManagementService) -> String {
    let _ = service.refresh_quotas_for_agent("grok").await;
    let quotas = service.store.list_quotas(None).await.unwrap_or_default();

    if quotas.is_empty() {
        return "**Agent Quotas & Limits**\n\n\
                *No quota limits recorded.* All agents are currently unrestricted.\n\n\
                **Manage Quotas:**\n\
                - `/quota refresh [agent]` — Refresh from provider\n\
                - `/quota reset <agent>` — Clear limits\n\
                - `/quota set <agent> <pct>` — Set percentage\n\
                - `/quota block <agent>` — Suspend agent\n"
            .to_string();
    }

    let mut out = String::from("**Agent Quotas & Usage Limits**\n\n");
    out.push_str("| Agent | Quota Type | Used / Remaining | Status | Reset Time |\n");
    out.push_str("|---|---|---|---|---|\n");

    for q in &quotas {
        let agent = q.agent_kind.as_deref().unwrap_or(q.pool_id.as_str());
        let remaining = q
            .remaining_percent
            .map(|p| format!("{:.0}%", p))
            .unwrap_or_else(|| "N/A".to_string());
        let used = format!("{:.0}%", q.used_amount);
        let status_badge = if q.status == "exhausted" {
            "🚫 **Exhausted**"
        } else {
            "✅ Valid"
        };
        let reset_str = q.reset_at.as_deref().unwrap_or("—");
        out.push_str(&format!(
            "| **{}** | {} | {} used ({} left) | {} | {} |\n",
            agent, q.quota_type, used, remaining, status_badge, reset_str
        ));
    }

    out.push_str(
        "\n**Manage Quotas:**\n\
         - `/quota reset <agent>` — Reset quota to 100% and unblock\n\
         - `/quota refresh [agent]` — Re-read usage from provider\n\
         - `/quota set <agent> <pct>` — Set remaining quota %\n\
         - `/quota block <agent>` — Mark quota exhausted / suspend agent\n",
    );

    out
}

pub async fn get_short_quota_summary() -> String {
    let service = TuiManagementService::global().await;
    let quotas = service.store.list_quotas(None).await.unwrap_or_default();
    if quotas.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    for q in &quotas {
        let agent = q.agent_kind.as_deref().unwrap_or(q.pool_id.as_str());
        if q.status == "exhausted" {
            let reset_info = q
                .reset_at
                .as_deref()
                .map(|r| format!(" (resets at {r})"))
                .unwrap_or_default();
            lines.push(format!("- **{agent}**: 🚫 **Exhausted**{reset_info}"));
        } else {
            let rem = q
                .remaining_percent
                .map(|p| format!("{:.0}% left", p))
                .unwrap_or_else(|| "valid".to_string());
            lines.push(format!("- **{agent}**: ✅ Valid ({rem})"));
        }
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!(
            "**Agent Quotas & Limits:**\n{}\n*(Use `/quota` to manage)*",
            lines.join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quota_slash_commands() {
        let overview = handle_quota_slash_command("").await;
        assert!(overview.contains("Agent Quotas"));

        let set_res = handle_quota_slash_command("set mock_test_agent 85").await;
        assert!(set_res.contains("mock_test_agent"));
        assert!(set_res.contains("85"));

        let block_res = handle_quota_slash_command("block mock_test_agent").await;
        assert!(block_res.contains("suspended") || block_res.contains("exhausted"));

        let reset_res = handle_quota_slash_command("reset mock_test_agent").await;
        assert!(reset_res.contains("reset"));
        assert!(reset_res.contains("100%"));
    }
}
