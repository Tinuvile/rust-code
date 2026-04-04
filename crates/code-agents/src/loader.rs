//! Load custom agent definitions from `.claude/agents/` on disk.
//!
//! Supported formats:
//!   - Markdown (`.md`) with YAML frontmatter — body is the system prompt.
//!   - YAML (`.yaml` / `.yml`) — full AgentDefinition struct.
//!
//! Ref: src/tools/AgentTool/loadAgentsDir.ts

use std::path::{Path, PathBuf};

use anyhow::Result;
use tokio::fs;

use crate::definition::{AgentDefinition, AgentSource};

// ── Public API ────────────────────────────────────────────────────────────────

/// Load all agent definitions from `<cwd>/.claude/agents/`.
///
/// Missing directory is not an error — returns an empty vec.
pub async fn load_agents_dir(cwd: &Path) -> Vec<AgentDefinition> {
    let dir = agents_dir(cwd);
    load_from_dir(&dir).await.unwrap_or_default()
}

/// Load agents from `~/.claude/agents/` (global user agents).
pub async fn load_global_agents() -> Vec<AgentDefinition> {
    let Some(home) = dirs_next::home_dir() else {
        return vec![];
    };
    let dir = home.join(".claude").join("agents");
    load_from_dir(&dir).await.unwrap_or_default()
}

// ── Internals ─────────────────────────────────────────────────────────────────

fn agents_dir(cwd: &Path) -> PathBuf {
    cwd.join(".claude").join("agents")
}

async fn load_from_dir(dir: &Path) -> Result<Vec<AgentDefinition>> {
    let mut agents = Vec::new();

    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "md" => {
                if let Ok(agent) = load_markdown_agent(&path).await {
                    agents.push(agent);
                }
            }
            "yaml" | "yml" => {
                if let Ok(agent) = load_yaml_agent(&path).await {
                    agents.push(agent);
                }
            }
            _ => {}
        }
    }

    agents.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(agents)
}

/// Parse a Markdown file with YAML frontmatter.
///
/// Frontmatter keys: `name`, `description`, `when_to_use`, `tools`, `model`, `color`, `max_turns`.
/// The body (after `---`) becomes the `system_prompt`.
async fn load_markdown_agent(path: &Path) -> Result<AgentDefinition> {
    let raw = fs::read_to_string(path).await?;

    let (frontmatter_str, body) = split_frontmatter(&raw);

    // Parse frontmatter as a loose YAML value.
    let fm: serde_yaml::Value = if frontmatter_str.is_empty() {
        serde_yaml::Value::Null
    } else {
        serde_yaml::from_str(frontmatter_str).unwrap_or(serde_yaml::Value::Null)
    };

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_owned();

    let name = yaml_str(&fm, "name").unwrap_or_else(|| stem.clone());
    let agent_type = yaml_str(&fm, "agent_type")
        .or_else(|| yaml_str(&fm, "agentType"))
        .unwrap_or_else(|| stem.clone());
    let description = yaml_str(&fm, "description").unwrap_or_default();
    let when_to_use = yaml_str(&fm, "when_to_use")
        .or_else(|| yaml_str(&fm, "whenToUse"))
        .unwrap_or_default();
    let model = yaml_str(&fm, "model");
    let color = yaml_str(&fm, "color");
    let max_turns = fm
        .get("max_turns")
        .or_else(|| fm.get("maxTurns"))
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let tools: Vec<String> = fm
        .get("tools")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_else(|| vec!["*".into()]);

    Ok(AgentDefinition {
        agent_type,
        name,
        system_prompt: body.trim().to_owned(),
        description,
        when_to_use,
        tools,
        model,
        color,
        source: AgentSource::UserDefined,
        max_turns,
    })
}

async fn load_yaml_agent(path: &Path) -> Result<AgentDefinition> {
    let raw = fs::read_to_string(path).await?;
    let mut agent: AgentDefinition = serde_yaml::from_str(&raw)?;
    agent.source = AgentSource::UserDefined;
    Ok(agent)
}

// ── Frontmatter splitting ─────────────────────────────────────────────────────

/// Split `---\n<frontmatter>\n---\n<body>` into `(frontmatter, body)`.
///
/// If there is no frontmatter, returns `("", full_text)`.
fn split_frontmatter(text: &str) -> (&str, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return ("", text);
    };
    if let Some(end) = rest.find("\n---\n") {
        (&rest[..end], &rest[end + 5..])
    } else if let Some(end) = rest.find("\n---") {
        (&rest[..end], &rest[end + 4..])
    } else {
        ("", text)
    }
}

fn yaml_str<'a>(v: &'a serde_yaml::Value, key: &str) -> Option<String> {
    v.get(key)?.as_str().map(str::to_owned)
}
