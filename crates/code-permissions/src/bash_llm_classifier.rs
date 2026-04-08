//! LLM-enhanced bash command classifier.
//!
//! When the pattern-matching classifier in `bash_classifier.rs` returns `Write`
//! (ambiguous), this module provides an LLM side-query to perform semantic
//! analysis of the command.  The LLM classifier understands command intent,
//! argument semantics, and common development workflows.
//!
//! ## Two-layer architecture
//!
//! 1. **Fast path** (pattern matching, `bash_classifier.rs`):
//!    - `ReadOnly` → auto-allow immediately
//!    - `Dangerous` → deny immediately
//!    - `Write` → escalate to LLM classifier
//!
//! 2. **Semantic path** (this module, LLM side-query):
//!    - Analyzes command intent with context
//!    - Returns `Allow`, `Ask`, or `Deny` with reasoning
//!    - Fail-safe: any parse failure falls back to `Ask`
//!
//! Ref: src/utils/permissions/bashClassifier.ts
//!      src/tools/BashTool/bashPermissions.ts

use std::path::Path;

use tracing::{debug, warn};

use code_types::message::{ApiMessage, ApiRole, ContentBlock, TextBlock};
use code_types::provider::{LlmProvider, LlmRequest};

// ── Result types ────────────────────────────────────────────────────────────

/// Semantic safety classification from the LLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BashLlmDecision {
    /// Command is safe — auto-allow without prompting.
    Allow,
    /// Command may have side effects but is a normal dev operation — ask user.
    Ask,
    /// Command is dangerous — deny.
    Deny,
}

/// Full result from the LLM bash classifier.
#[derive(Debug, Clone)]
pub struct BashLlmResult {
    /// The classification decision.
    pub decision: BashLlmDecision,
    /// Human-readable reasoning from the LLM.
    pub reasoning: String,
    /// Which category the command falls into.
    pub category: BashCommandCategory,
    /// Token usage.
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Semantic categories for bash commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BashCommandCategory {
    /// File read operations (cat, head, less, etc.)
    FileRead,
    /// File write operations (write, append, create)
    FileWrite,
    /// File delete operations (rm, rmdir, unlink)
    FileDelete,
    /// Git read operations (log, diff, status, branch)
    GitRead,
    /// Git write operations (commit, push, merge, rebase)
    GitWrite,
    /// Package manager read (list, show, info)
    PackageRead,
    /// Package manager write (install, uninstall, update)
    PackageWrite,
    /// Build and test commands (cargo build, npm test, make)
    BuildTest,
    /// Process management (kill, restart services)
    ProcessManagement,
    /// Network operations (curl, wget, ssh)
    Network,
    /// System administration (chmod, chown, systemctl)
    SystemAdmin,
    /// Container operations (docker, kubectl)
    Container,
    /// Code search and navigation (grep, find, rg)
    Search,
    /// Environment inspection (env, printenv, uname)
    EnvironmentInfo,
    /// Other / unrecognized
    Other,
}

impl std::fmt::Display for BashCommandCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::FileRead => "file_read",
            Self::FileWrite => "file_write",
            Self::FileDelete => "file_delete",
            Self::GitRead => "git_read",
            Self::GitWrite => "git_write",
            Self::PackageRead => "package_read",
            Self::PackageWrite => "package_write",
            Self::BuildTest => "build_test",
            Self::ProcessManagement => "process_management",
            Self::Network => "network",
            Self::SystemAdmin => "system_admin",
            Self::Container => "container",
            Self::Search => "search",
            Self::EnvironmentInfo => "environment_info",
            Self::Other => "other",
        };
        f.write_str(s)
    }
}

// ── Bash-specific allow/deny descriptions ───────────────────────────────────

/// Semantic descriptions of bash patterns that should be ALLOWED.
///
/// These are injected into the auto-classifier system prompt when evaluating
/// bash tool calls, giving the LLM richer context for classification.
pub static BASH_ALLOW_DESCRIPTIONS: &[&str] = &[
    "Reading file contents (cat, head, tail, less, bat)",
    "Listing directory contents (ls, ll, dir, tree)",
    "Searching files and content (grep, rg, ag, find, locate, fd)",
    "Checking file metadata (stat, file, wc, du, df)",
    "Diffing files (diff, cmp, comm)",
    "Git read operations (git log, git diff, git status, git show, git branch, git tag)",
    "Environment inspection (env, printenv, uname, hostname, whoami, id)",
    "Process listing (ps, pgrep, top, jobs)",
    "Standard build commands in project directory (cargo build, cargo test, cargo check, npm run build, make, go build)",
    "Standard test commands (cargo test, npm test, pytest, go test, jest, vitest)",
    "Standard lint/format commands (cargo fmt, cargo clippy, eslint, prettier, black, gofmt)",
    "Package info queries (npm list, pip show, cargo metadata, go list)",
    "Type checking (tsc --noEmit, mypy, pyright)",
    "Documentation generation (cargo doc, jsdoc)",
    "Viewing man pages and help (man, --help, -h)",
    "Data processing pipelines (jq, yq, sort, uniq, cut, awk, sed without -i)",
    "Network read-only fetches (curl -s URL without -o/-O, wget -q -O - URL)",
    "Date/time queries (date, cal, uptime)",
];

/// Semantic descriptions of bash patterns that should be DENIED or require asking.
pub static BASH_DENY_DESCRIPTIONS: &[&str] = &[
    "Deleting files outside the project directory (rm with paths outside cwd)",
    "Recursive force deletion (rm -rf with broad paths, especially / or ~)",
    "Git destructive operations (git push --force, git reset --hard, git clean -fd)",
    "Installing system-wide packages (sudo apt install, brew install, pip install --system)",
    "Modifying system configuration files (/etc/*, /boot/*)",
    "Changing file permissions on system directories (chmod on /usr, /etc, /var)",
    "Network operations with side effects (curl -X POST, wget with file output)",
    "Process termination (kill, killall, pkill of system processes)",
    "Service management (systemctl, service start/stop)",
    "Disk operations (dd, mkfs, fdisk, mount)",
    "User management (useradd, userdel, passwd)",
    "Firewall changes (iptables, ufw)",
    "Crontab modifications (crontab -e, writing to /etc/cron*)",
    "SSH/SCP to remote hosts (ssh, scp, rsync to remote)",
    "Running piped-to-shell patterns (curl | bash, wget | sh)",
    "Docker container operations that affect host (docker run with volume mounts to /, --privileged)",
    "Kubectl operations that modify cluster state (kubectl delete, kubectl apply to production)",
    "Environment variable manipulation that could affect other processes",
    "Downloading and executing unknown scripts",
];

// ── Classifier entry point ──────────────────────────────────────────────────

/// Classify a bash command using an LLM side-query.
///
/// This should only be called for commands that the pattern-matching classifier
/// marked as `Write` (ambiguous).  The LLM provides semantic understanding.
///
/// Returns `None` on failure — callers should fall back to `Ask`.
pub async fn classify_bash_llm(
    provider: &dyn LlmProvider,
    model: &str,
    command: &str,
    cwd: &Path,
    project_context: Option<&str>,
) -> Option<BashLlmResult> {
    let system_prompt = build_bash_system_prompt(cwd, project_context);
    let user_prompt = build_bash_user_prompt(command);

    let messages = vec![ApiMessage {
        role: ApiRole::User,
        content: vec![ContentBlock::Text(TextBlock {
            text: user_prompt,
            cache_control: None,
        })],
    }];

    let request = LlmRequest {
        model: model.to_owned(),
        messages,
        max_tokens: 256,
        system: Some(serde_json::Value::String(system_prompt)),
        tools: vec![],
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response = match provider.send(request).await {
        Ok(r) => r,
        Err(e) => {
            warn!("bash LLM classifier query failed: {e}");
            return None;
        }
    };

    let text: String = response
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text(t) = b {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("");

    let result = parse_bash_classifier_response(
        &text,
        response.usage.input_tokens,
        response.usage.output_tokens,
    );

    match &result {
        Some(r) => {
            debug!(
                "bash LLM classifier: {:?} [{}] — {}",
                r.decision, r.category, r.reasoning
            );
        }
        None => {
            warn!("bash LLM classifier: failed to parse response");
        }
    }

    result
}

// ── Prompt construction ─────────────────────────────────────────────────────

fn build_bash_system_prompt(cwd: &Path, project_context: Option<&str>) -> String {
    let mut prompt = String::with_capacity(4096);

    prompt.push_str(
        "You are a bash command safety classifier for an AI coding assistant.\n\
         Analyze the given command and classify it as ALLOW, ASK, or DENY.\n\n\
         ## Classification rules\n\n\
         **ALLOW** — Command is safe to run without user confirmation:\n"
    );
    for desc in BASH_ALLOW_DESCRIPTIONS {
        prompt.push_str(&format!("- {desc}\n"));
    }

    prompt.push_str(
        "\n**DENY** — Command is dangerous and should be blocked:\n"
    );
    for desc in BASH_DENY_DESCRIPTIONS {
        prompt.push_str(&format!("- {desc}\n"));
    }

    prompt.push_str(
        "\n**ASK** — Command modifies local project state and needs user confirmation:\n\
         - Installing project-local packages (npm install, pip install in venv, cargo add)\n\
         - Git write operations within the project (git commit, git push to origin)\n\
         - Creating/modifying files within the project directory\n\
         - Running project scripts that may have side effects\n\
         - Any command whose safety is ambiguous\n\n"
    );

    prompt.push_str(&format!(
        "## Environment\n\n\
         Working directory: {}\n",
        cwd.display()
    ));

    if let Some(ctx) = project_context {
        prompt.push_str(&format!("Project context: {ctx}\n"));
    }

    prompt.push_str(
        "\n## Response format\n\n\
         Respond with XML only:\n\n\
         ```\n\
         <category>one of: file_read, file_write, file_delete, git_read, git_write, \
         package_read, package_write, build_test, process_management, network, \
         system_admin, container, search, environment_info, other</category>\n\
         <reasoning>Brief one-sentence explanation</reasoning>\n\
         <decision>ALLOW|ASK|DENY</decision>\n\
         ```\n\n\
         Be conservative: when in doubt, classify as ASK rather than ALLOW.\n\
         Never ALLOW commands that could have irreversible effects outside the project.\n"
    );

    prompt
}

fn build_bash_user_prompt(command: &str) -> String {
    format!("Classify this bash command:\n\n```bash\n{command}\n```")
}

// ── Response parsing ────────────────────────────────────────────────────────

fn parse_bash_classifier_response(
    text: &str,
    input_tokens: u32,
    output_tokens: u32,
) -> Option<BashLlmResult> {
    let decision_str = extract_xml_tag(text, "decision")?;
    let decision = match decision_str.trim().to_uppercase().as_str() {
        "ALLOW" => BashLlmDecision::Allow,
        "ASK" => BashLlmDecision::Ask,
        "DENY" | "BLOCK" => BashLlmDecision::Deny,
        _ => {
            warn!("bash LLM classifier: unrecognised decision: '{decision_str}'");
            // Fail safe — ask.
            BashLlmDecision::Ask
        }
    };

    let reasoning = extract_xml_tag(text, "reasoning")
        .unwrap_or_else(|| "No reasoning provided".to_owned());

    let category = extract_xml_tag(text, "category")
        .map(|s| parse_category(&s))
        .unwrap_or(BashCommandCategory::Other);

    Some(BashLlmResult {
        decision,
        reasoning,
        category,
        input_tokens,
        output_tokens,
    })
}

fn parse_category(s: &str) -> BashCommandCategory {
    match s.trim().to_lowercase().replace('-', "_").as_str() {
        "file_read" => BashCommandCategory::FileRead,
        "file_write" => BashCommandCategory::FileWrite,
        "file_delete" => BashCommandCategory::FileDelete,
        "git_read" => BashCommandCategory::GitRead,
        "git_write" => BashCommandCategory::GitWrite,
        "package_read" => BashCommandCategory::PackageRead,
        "package_write" => BashCommandCategory::PackageWrite,
        "build_test" | "build" | "test" => BashCommandCategory::BuildTest,
        "process_management" | "process" => BashCommandCategory::ProcessManagement,
        "network" => BashCommandCategory::Network,
        "system_admin" | "system" => BashCommandCategory::SystemAdmin,
        "container" | "docker" | "kubernetes" => BashCommandCategory::Container,
        "search" => BashCommandCategory::Search,
        "environment_info" | "environment" => BashCommandCategory::EnvironmentInfo,
        _ => BashCommandCategory::Other,
    }
}

/// Extract the inner text of a simple XML tag from a string.
fn extract_xml_tag(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let end = text.find(&close)?;
    if end <= start {
        return None;
    }
    let inner = &text[start + open.len()..end];
    Some(inner.trim().to_owned())
}

// ── Integration helper ──────────────────────────────────────────────────────

/// Inject bash-specific descriptions into an `auto_classifier::ClassifierConfig`.
///
/// Called by the evaluator when classifying a Bash tool call in auto mode,
/// so the general auto-classifier also has bash-domain knowledge.
pub fn enrich_classifier_config(
    config: &mut crate::auto_classifier::ClassifierConfig,
) {
    // Append bash allow descriptions.
    let bash_allow_section = format!(
        "\n## Bash-specific allow patterns\n{}",
        BASH_ALLOW_DESCRIPTIONS
            .iter()
            .map(|d| format!("- {d}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let bash_deny_section = format!(
        "\n## Bash-specific deny patterns\n{}",
        BASH_DENY_DESCRIPTIONS
            .iter()
            .map(|d| format!("- {d}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    config.environment_desc = Some(format!(
        "{}{}{}",
        config.environment_desc.as_deref().unwrap_or(""),
        bash_allow_section,
        bash_deny_section,
    ));
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_allow_response() {
        let text = "\
            <category>build_test</category>\n\
            <reasoning>cargo test runs the project test suite within the project directory</reasoning>\n\
            <decision>ALLOW</decision>";
        let result = parse_bash_classifier_response(text, 100, 20).unwrap();
        assert_eq!(result.decision, BashLlmDecision::Allow);
        assert_eq!(result.category, BashCommandCategory::BuildTest);
        assert!(result.reasoning.contains("cargo test"));
    }

    #[test]
    fn parse_ask_response() {
        let text = "\
            <category>package_write</category>\n\
            <reasoning>npm install modifies node_modules and package-lock.json</reasoning>\n\
            <decision>ASK</decision>";
        let result = parse_bash_classifier_response(text, 100, 20).unwrap();
        assert_eq!(result.decision, BashLlmDecision::Ask);
        assert_eq!(result.category, BashCommandCategory::PackageWrite);
    }

    #[test]
    fn parse_deny_response() {
        let text = "\
            <category>system_admin</category>\n\
            <reasoning>sudo rm -rf could destroy system files</reasoning>\n\
            <decision>DENY</decision>";
        let result = parse_bash_classifier_response(text, 100, 20).unwrap();
        assert_eq!(result.decision, BashLlmDecision::Deny);
        assert_eq!(result.category, BashCommandCategory::SystemAdmin);
    }

    #[test]
    fn parse_block_alias() {
        let text = "<category>other</category>\n<reasoning>risky</reasoning>\n<decision>BLOCK</decision>";
        let result = parse_bash_classifier_response(text, 0, 0).unwrap();
        assert_eq!(result.decision, BashLlmDecision::Deny);
    }

    #[test]
    fn parse_unknown_decision_defaults_to_ask() {
        let text = "<category>other</category>\n<reasoning>unclear</reasoning>\n<decision>MAYBE</decision>";
        let result = parse_bash_classifier_response(text, 0, 0).unwrap();
        assert_eq!(result.decision, BashLlmDecision::Ask);
    }

    #[test]
    fn parse_missing_decision_returns_none() {
        let text = "<category>other</category>\n<reasoning>no decision tag here</reasoning>";
        assert!(parse_bash_classifier_response(text, 0, 0).is_none());
    }

    #[test]
    fn parse_category_variants() {
        assert_eq!(parse_category("file_read"), BashCommandCategory::FileRead);
        assert_eq!(parse_category("git_write"), BashCommandCategory::GitWrite);
        assert_eq!(parse_category("build_test"), BashCommandCategory::BuildTest);
        assert_eq!(parse_category("container"), BashCommandCategory::Container);
        assert_eq!(parse_category("NETWORK"), BashCommandCategory::Network);
        assert_eq!(parse_category("unknown"), BashCommandCategory::Other);
    }

    #[test]
    fn system_prompt_includes_descriptions() {
        let prompt = build_bash_system_prompt(Path::new("/home/user/project"), None);
        // Check allow descriptions are present.
        assert!(prompt.contains("Reading file contents"));
        assert!(prompt.contains("Standard build commands"));
        // Check deny descriptions are present.
        assert!(prompt.contains("Recursive force deletion"));
        assert!(prompt.contains("Git destructive operations"));
        // Check structure.
        assert!(prompt.contains("ALLOW"));
        assert!(prompt.contains("ASK"));
        assert!(prompt.contains("DENY"));
        assert!(prompt.contains("/home/user/project"));
    }

    #[test]
    fn system_prompt_includes_project_context() {
        let prompt = build_bash_system_prompt(
            Path::new("/home/user/project"),
            Some("Rust project using Cargo"),
        );
        assert!(prompt.contains("Rust project using Cargo"));
    }

    #[test]
    fn user_prompt_wraps_command() {
        let prompt = build_bash_user_prompt("npm install express");
        assert!(prompt.contains("```bash"));
        assert!(prompt.contains("npm install express"));
    }

    #[test]
    fn enrich_config_adds_bash_descriptions() {
        let mut config = crate::auto_classifier::ClassifierConfig::default();
        enrich_classifier_config(&mut config);
        let env = config.environment_desc.unwrap();
        assert!(env.contains("Bash-specific allow patterns"));
        assert!(env.contains("Bash-specific deny patterns"));
        assert!(env.contains("Reading file contents"));
        assert!(env.contains("Recursive force deletion"));
    }

    #[test]
    fn extract_xml_works() {
        assert_eq!(
            extract_xml_tag("<decision>ALLOW</decision>", "decision"),
            Some("ALLOW".to_owned())
        );
        assert_eq!(
            extract_xml_tag("prefix <reasoning> hello world </reasoning> suffix", "reasoning"),
            Some("hello world".to_owned())
        );
        assert_eq!(extract_xml_tag("no tags", "foo"), None);
    }
}
