use ignore::overrides::OverrideBuilder;
use std::path::Path;

use super::Rule;

pub struct RuleMatcher;

impl RuleMatcher {
    pub fn is_rule_active_for_paths(
        rule: &Rule,
        working_dir: &Path,
        touched_paths: &[&Path],
    ) -> bool {
        if rule.always_apply {
            return true;
        }

        if rule.globs.is_empty() {
            return false;
        }

        let mut builder = OverrideBuilder::new(working_dir);
        for glob in &rule.globs {
            if let Err(err) = builder.add(glob) {
                tracing::debug!("Failed to compile glob '{}': {}", glob, err);
            }
        }

        let overrides = match builder.build() {
            Ok(o) => o,
            Err(_) => return false,
        };

        for path in touched_paths {
            let candidate = if path.is_absolute() {
                if let Ok(rel) = path.strip_prefix(working_dir) {
                    rel
                } else {
                    path
                }
            } else {
                path
            };

            let is_dir = path.is_dir();
            if overrides.matched(candidate, is_dir).is_whitelist() {
                return true;
            }
        }

        false
    }

    pub fn filter_active_rules<'a>(
        rules: &'a [Rule],
        working_dir: &Path,
        touched_paths: &[&Path],
    ) -> Vec<&'a Rule> {
        rules
            .iter()
            .filter(|rule| Self::is_rule_active_for_paths(rule, working_dir, touched_paths))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_test_rule(name: &str, globs: Vec<&str>, always_apply: bool) -> Rule {
        Rule {
            name: name.to_string(),
            description: String::new(),
            globs: globs.into_iter().map(String::from).collect(),
            always_apply,
            tags: Vec::new(),
            order: 0,
            path: PathBuf::from(format!("/fake/rules/{}.md", name)),
            global: false,
            writable: true,
            content: format!("Content for {}", name),
            properties: HashMap::new(),
        }
    }

    #[test]
    fn test_always_apply_rule_matches_anything() {
        let rule = make_test_rule("always", vec![], true);
        let root = Path::new("/workspace");
        assert!(RuleMatcher::is_rule_active_for_paths(
            &rule,
            root,
            &[Path::new("/workspace/src/lib.rs")]
        ));
        assert!(RuleMatcher::is_rule_active_for_paths(&rule, root, &[]));
    }

    #[test]
    fn test_glob_scoped_rule_matches_file() {
        let rule = make_test_rule("rust", vec!["*.rs", "crates/**/*.rs"], false);
        let root = Path::new("/workspace");
        assert!(RuleMatcher::is_rule_active_for_paths(
            &rule,
            root,
            &[Path::new("/workspace/crates/app/src/main.rs")]
        ));
        assert!(!RuleMatcher::is_rule_active_for_paths(
            &rule,
            root,
            &[Path::new("/workspace/ui/index.html")]
        ));
    }
}
