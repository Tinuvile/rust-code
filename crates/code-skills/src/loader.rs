//! Load custom skill definitions from `.claude/skills/` on disk.
//!
//! Supports Markdown (`.md`) with YAML frontmatter and YAML (`.yaml` / `.yml`).
//! The Markdown body becomes the skill's `content` (system prompt).
//!
//! Ref: src/skills/loadSkillsDir.ts

use std::path::Path;

use anyhow::Result;

use crate::skill::{Skill, SkillSource};

// ── Public API ────────────────────────────────────────────────────────────────

/// Load all skill definitions from `<cwd>/.claude/skills/`.
pub async fn load_skills_dir(cwd: &Path) -> Vec<Skill> {
    let dir = cwd.join(".claude").join("skills");
    load_from_dir(&dir).await.unwrap_or_default()
}

/// Load skills from `~/.claude/skills/` (global user skills).
pub async fn load_global_skills() -> Vec<Skill> {
    let Some(home) = dirs_next::home_dir() else {
        return vec![];
    };
    let dir = home.join(".claude").join("skills");
    load_from_dir(&dir).await.unwrap_or_default()
}

// ── Internals ─────────────────────────────────────────────────────────────────

async fn load_from_dir(dir: &Path) -> Result<Vec<Skill>> {
    let mut skills = Vec::new();

    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "md" => {
                if let Ok(skill) = load_markdown_skill(&path).await {
                    skills.push(skill);
                }
            }
            "yaml" | "yml" => {
                if let Ok(skill) = load_yaml_skill(&path).await {
                    skills.push(skill);
                }
            }
            _ => {}
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

async fn load_markdown_skill(path: &Path) -> Result<Skill> {
    let raw = tokio::fs::read_to_string(path).await?;
    let (frontmatter_str, body) = split_frontmatter(&raw);

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
    let description = yaml_str(&fm, "description").unwrap_or_default();
    let when_to_use = yaml_str(&fm, "when_to_use")
        .or_else(|| yaml_str(&fm, "whenToUse"))
        .unwrap_or_default();
    let model = yaml_str(&fm, "model");
    let argument_hint = yaml_str(&fm, "argument_hint")
        .or_else(|| yaml_str(&fm, "argumentHint"));

    let aliases: Vec<String> = fm
        .get("aliases")
        .and_then(|v| v.as_sequence())
        .map(|seq| seq.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();

    let allowed_tools: Vec<String> = fm
        .get("allowed_tools")
        .or_else(|| fm.get("allowedTools"))
        .and_then(|v| v.as_sequence())
        .map(|seq| seq.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();

    let user_invocable = fm
        .get("user_invocable")
        .or_else(|| fm.get("userInvocable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Ok(Skill {
        name,
        description,
        aliases,
        when_to_use,
        content: body.trim().to_owned(),
        allowed_tools,
        model,
        user_invocable,
        context: crate::skill::SkillContext::Inline,
        source: SkillSource::UserDefined,
        argument_hint,
    })
}

async fn load_yaml_skill(path: &Path) -> Result<Skill> {
    let raw = tokio::fs::read_to_string(path).await?;
    let mut skill: Skill = serde_yaml::from_str(&raw)?;
    skill.source = SkillSource::UserDefined;
    Ok(skill)
}

// ── Frontmatter splitting ─────────────────────────────────────────────────────

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

fn yaml_str(v: &serde_yaml::Value, key: &str) -> Option<String> {
    v.get(key)?.as_str().map(str::to_owned)
}
