//! Permission rule string parsing.
//!
//! A rule string has the form `"<tool_name>[(<content>)]"` or the legacy
//! `"<tool_name>:<content>"` shorthand.  The content may contain `*` wildcards.
//!
//! Examples:
//!   - `"Bash"`                  → tool-only, any invocation
//!   - `"Bash(git *)"`           → Bash with content matching `git *`
//!   - `"Bash(git diff)"`        → Bash with exact content `git diff`
//!   - `"Read(/home/user/**)"`   → Read with glob path
//!   - `"WebSearch(*claude*)"`   → WebSearch with wildcard content
//!
//! Ref: src/utils/permissions/shellRuleMatching.ts (parsePermissionRule, RuleKind)

use code_types::permissions::PermissionRuleValue;

/// Parsed form of a permission rule's content pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RulePattern {
    /// Match any invocation of the tool (no content restriction).
    Any,
    /// Match only when the content is exactly equal to this string.
    Exact(String),
    /// Match when the content matches this glob pattern.
    /// The pattern may contain `*` (single segment) or `**` (multi-segment).
    Glob(String),
}

impl RulePattern {
    /// Parse a raw content string into the most specific `RulePattern`.
    ///
    /// - Empty → `Any`
    /// - No wildcard chars → `Exact`
    /// - Contains `*` or `?` → `Glob`
    pub fn from_content(content: &str) -> Self {
        if content.is_empty() {
            return RulePattern::Any;
        }
        if content.contains('*') || content.contains('?') {
            RulePattern::Glob(content.to_owned())
        } else {
            RulePattern::Exact(content.to_owned())
        }
    }

    /// Returns `true` if this pattern matches `input`.
    ///
    /// Uses `globset` for glob patterns, exact equality for `Exact`, and
    /// always `true` for `Any`.
    pub fn matches(&self, input: &str) -> bool {
        match self {
            RulePattern::Any => true,
            RulePattern::Exact(s) => s == input,
            RulePattern::Glob(pattern) => {
                let mut builder = globset::GlobBuilder::new(pattern);
                builder.case_insensitive(false);
                match builder.build() {
                    Ok(glob) => glob.compile_matcher().is_match(input),
                    Err(_) => {
                        // Fall back to simple prefix check if the pattern is invalid.
                        input.starts_with(pattern.trim_end_matches('*'))
                    }
                }
            }
        }
    }
}

/// A fully parsed permission rule.
#[derive(Debug, Clone)]
pub struct ParsedRule {
    /// The tool this rule applies to (case-insensitive match).
    pub tool_name: String,
    /// The content pattern to match against the tool's key argument.
    pub pattern: RulePattern,
}

impl ParsedRule {
    /// Parse a `PermissionRuleValue` from settings into a `ParsedRule`.
    pub fn from_rule_value(rv: &PermissionRuleValue) -> Self {
        let pattern = match &rv.rule_content {
            None => RulePattern::Any,
            Some(c) => RulePattern::from_content(c),
        };
        ParsedRule {
            tool_name: rv.tool_name.clone(),
            pattern,
        }
    }

    /// Parse a raw rule string such as `"Bash(git *)"` or `"Read"`.
    ///
    /// Supports two formats:
    /// 1. `"ToolName(content)"` — parenthesis delimiter
    /// 2. `"ToolName:content"`  — legacy colon delimiter
    pub fn from_str(raw: &str) -> Self {
        let raw = raw.trim();
        // Parenthesis form: ToolName(content)
        if let Some(paren_start) = raw.find('(') {
            let tool_name = raw[..paren_start].trim().to_owned();
            let rest = &raw[paren_start + 1..];
            let content = rest.trim_end_matches(')').trim().to_owned();
            return ParsedRule {
                tool_name,
                pattern: RulePattern::from_content(&content),
            };
        }
        // Colon form: ToolName:content (legacy)
        if let Some(colon) = raw.find(':') {
            let tool_name = raw[..colon].trim().to_owned();
            let content = raw[colon + 1..].trim().to_owned();
            return ParsedRule {
                tool_name,
                pattern: RulePattern::from_content(&content),
            };
        }
        // Tool-only form.
        ParsedRule {
            tool_name: raw.to_owned(),
            pattern: RulePattern::Any,
        }
    }

    /// Returns `true` if this rule applies to `tool_name` and the optional
    /// `content` string matches the pattern.
    pub fn matches(&self, tool_name: &str, content: Option<&str>) -> bool {
        if !self.tool_name.eq_ignore_ascii_case(tool_name) {
            return false;
        }
        match content {
            None => matches!(self.pattern, RulePattern::Any),
            Some(c) => self.pattern.matches(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_only() {
        let r = ParsedRule::from_str("Bash");
        assert_eq!(r.tool_name, "Bash");
        assert_eq!(r.pattern, RulePattern::Any);
        assert!(r.matches("Bash", None));
        assert!(r.matches("bash", None)); // case insensitive
    }

    #[test]
    fn parse_paren_glob() {
        let r = ParsedRule::from_str("Bash(git *)");
        assert_eq!(r.tool_name, "Bash");
        assert!(matches!(r.pattern, RulePattern::Glob(_)));
        assert!(r.matches("Bash", Some("git status")));
        assert!(!r.matches("Bash", Some("rm -rf /")));
    }

    #[test]
    fn parse_exact() {
        let r = ParsedRule::from_str("Bash(git diff)");
        assert_eq!(r.pattern, RulePattern::Exact("git diff".to_owned()));
        assert!(r.matches("Bash", Some("git diff")));
        assert!(!r.matches("Bash", Some("git diff --cached")));
    }

    #[test]
    fn parse_colon_legacy() {
        let r = ParsedRule::from_str("Read:/home/user/**");
        assert_eq!(r.tool_name, "Read");
        assert!(matches!(r.pattern, RulePattern::Glob(_)));
    }
}
