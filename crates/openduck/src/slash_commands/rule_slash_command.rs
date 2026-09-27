use std::path::Path;

pub fn format_active_rules(working_dir: Option<&Path>) -> String {
    let rules = crate::rules::discover_rules(working_dir, &[]);
    if rules.is_empty() {
        let mut output = String::new();
        output.push_str("No rules active.\n\n");
        output.push_str("Rules can be configured in:\n");
        output.push_str("  - ~/.agents/rules/*.md (global rules)\n");
        output.push_str("  - ~/.config/openduck/rules/*.md (global config)\n");
        output.push_str("  - .agents/rules/*.md (project rules)\n");
        output.push_str("  - .openduck/rules/*.md (project config)\n");
        output.push_str("  - AGENTS.md / GEMINI.md / CLAUDE.md\n");
        return output;
    }

    let mut global_rules = Vec::new();
    let mut project_rules = Vec::new();

    for rule in &rules {
        if rule.global {
            global_rules.push(rule);
        } else {
            project_rules.push(rule);
        }
    }

    let mut output = format!("**Active rules ({}):**\n\n", rules.len());
    if !global_rules.is_empty() {
        output.push_str(&format!("**Global Rules ({})**:\n", global_rules.len()));
        for rule in global_rules {
            let desc = if !rule.description.is_empty() {
                format!(" - {}", rule.description)
            } else {
                String::new()
            };
            output.push_str(&format!("- **{}**{}\n", rule.name, desc));
        }
        output.push('\n');
    }

    if !project_rules.is_empty() {
        output.push_str(&format!("**Project Rules ({})**:\n", project_rules.len()));
        for rule in project_rules {
            let desc = if !rule.description.is_empty() {
                format!(" - {}", rule.description)
            } else {
                String::new()
            };
            output.push_str(&format!("- **{}**{}\n", rule.name, desc));
        }
    }

    output.trim_end().to_string()
}
