//! Permission rule matching — checks whether a tool call is covered by an
//! allow / deny / ask rule from the `ToolPermissionContext`.
//!
//! Ref: src/utils/permissions/shellRuleMatching.ts (matchRule, findMatchingRule)

use code_types::permissions::{
    PermissionBehavior, PermissionDecisionReason, PermissionRule, PermissionRuleSource,
    ToolPermissionContext,
};

use crate::rule_parser::ParsedRule;

/// Result of scanning all rules for a given tool call.
#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub behavior: PermissionBehavior,
    pub matched_rule: PermissionRule,
}

/// Search the `always_allow_rules`, `always_deny_rules`, and `always_ask_rules`
/// in the context for the first rule that matches `(tool_name, content)`.
///
/// Priority: deny > allow > ask (mirrors TypeScript behaviour where deny rules
/// win ties).
///
/// `content` is the primary argument of the tool call — e.g. the bash command
/// string for `Bash`, the file path for `Read`/`FileEdit`, the query for
/// `WebSearch`, etc.  Pass `None` when the tool has no primary argument.
pub fn find_matching_rule(
    ctx: &ToolPermissionContext,
    tool_name: &str,
    content: Option<&str>,
) -> Option<RuleMatch> {
    // Deny rules have the highest priority.
    if let Some(m) = scan_rules(
        &ctx.always_deny_rules,
        PermissionBehavior::Deny,
        tool_name,
        content,
    ) {
        return Some(m);
    }
    // Allow rules next.
    if let Some(m) = scan_rules(
        &ctx.always_allow_rules,
        PermissionBehavior::Allow,
        tool_name,
        content,
    ) {
        return Some(m);
    }
    // Ask rules last.
    scan_rules(
        &ctx.always_ask_rules,
        PermissionBehavior::Ask,
        tool_name,
        content,
    )
}

fn scan_rules(
    map: &code_types::permissions::ToolPermissionRulesBySource,
    behavior: PermissionBehavior,
    tool_name: &str,
    content: Option<&str>,
) -> Option<RuleMatch> {
    // Iterate sources in a stable order (source enum discriminant order).
    let mut sources: Vec<&PermissionRuleSource> = map.keys().collect();
    sources.sort_by_key(|s| source_priority(s));

    for source in sources {
        let rules = &map[source];
        for raw_rule in rules {
            let parsed = ParsedRule::from_str(raw_rule);
            if parsed.matches(tool_name, content) {
                let matched_rule = PermissionRule {
                    source: *source,
                    rule_behavior: behavior,
                    rule_value: code_types::permissions::PermissionRuleValue {
                        tool_name: tool_name.to_owned(),
                        rule_content: content.map(str::to_owned),
                    },
                };
                return Some(RuleMatch { behavior, matched_rule });
            }
        }
    }
    None
}

/// Higher = evaluated later (lower priority wins ties within the same behaviour
/// bucket).  Policy > CLI > command > session > user > project > local.
fn source_priority(source: &PermissionRuleSource) -> u8 {
    match source {
        PermissionRuleSource::PolicySettings => 0,
        PermissionRuleSource::FlagSettings => 1,
        PermissionRuleSource::CliArg => 2,
        PermissionRuleSource::Command => 3,
        PermissionRuleSource::Session => 4,
        PermissionRuleSource::UserSettings => 5,
        PermissionRuleSource::ProjectSettings => 6,
        PermissionRuleSource::LocalSettings => 7,
    }
}

/// Build a `PermissionDecisionReason::Rule` from a `RuleMatch`.
pub fn decision_reason_from_match(rm: &RuleMatch) -> PermissionDecisionReason {
    PermissionDecisionReason::Rule {
        rule: rm.matched_rule.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::permissions::{PermissionMode, ToolPermissionContext};
    use std::collections::HashMap;

    fn ctx_with_allow(rule: &str) -> ToolPermissionContext {
        let mut allow = HashMap::new();
        allow.insert(
            PermissionRuleSource::UserSettings,
            vec![rule.to_owned()],
        );
        ToolPermissionContext {
            mode: PermissionMode::Default,
            always_allow_rules: allow,
            ..Default::default()
        }
    }

    #[test]
    fn allow_rule_matches() {
        let ctx = ctx_with_allow("Bash(git *)");
        let m = find_matching_rule(&ctx, "Bash", Some("git status")).unwrap();
        assert_eq!(m.behavior, PermissionBehavior::Allow);
    }

    #[test]
    fn no_match_returns_none() {
        let ctx = ctx_with_allow("Bash(git *)");
        let m = find_matching_rule(&ctx, "Bash", Some("rm -rf /"));
        assert!(m.is_none());
    }

    #[test]
    fn deny_beats_allow() {
        let mut ctx = ctx_with_allow("Bash");
        let mut deny = HashMap::new();
        deny.insert(
            PermissionRuleSource::PolicySettings,
            vec!["Bash(rm *)".to_owned()],
        );
        ctx.always_deny_rules = deny;
        let m = find_matching_rule(&ctx, "Bash", Some("rm /tmp/foo")).unwrap();
        assert_eq!(m.behavior, PermissionBehavior::Deny);
    }
}
