//! Permission rule persistence — write allow/deny/ask rules back to settings files.
//!
//! Applies a `PermissionUpdate` to the settings files on disk.
//!
//! Ref: src/utils/permissions/PermissionUpdate.ts (applyPermissionUpdate)
//!      src/utils/permissions/PermissionRule.ts (persistPermissionRule)

use std::path::{Path, PathBuf};

use code_config::settings::{SettingsJson, SettingsPermissions};
use code_types::permissions::{
    PermissionBehavior, PermissionRuleValue, PermissionUpdate,
    PermissionUpdateDestination,
};

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Resolve the settings file path for the given destination.
///
/// - `UserSettings`    → `~/.claude/settings.json`
/// - `ProjectSettings` → `<cwd>/.claude/settings.json`
/// - `LocalSettings`   → `<cwd>/.claude/settings.local.json`
/// - `Session` / `CliArg` → no file (in-memory only)
pub fn settings_path_for_destination(
    destination: PermissionUpdateDestination,
    cwd: &Path,
) -> Option<PathBuf> {
    match destination {
        PermissionUpdateDestination::UserSettings => {
            dirs_next::home_dir().map(|h| h.join(".claude").join("settings.json"))
        }
        PermissionUpdateDestination::ProjectSettings => {
            Some(cwd.join(".claude").join("settings.json"))
        }
        PermissionUpdateDestination::LocalSettings => {
            Some(cwd.join(".claude").join("settings.local.json"))
        }
        PermissionUpdateDestination::Session | PermissionUpdateDestination::CliArg => None,
    }
}

// ── Rule serialization ────────────────────────────────────────────────────────

/// Serialize a `PermissionRuleValue` to the string form used in settings.json.
///
/// Format: `"ToolName"` or `"ToolName(content)"`.
pub fn rule_value_to_string(rv: &PermissionRuleValue) -> String {
    match &rv.rule_content {
        None => rv.tool_name.clone(),
        Some(c) => format!("{}({})", rv.tool_name, c),
    }
}

// ── Settings mutation ─────────────────────────────────────────────────────────

/// Apply a `PermissionUpdate` to a mutable `SettingsJson`, returning whether
/// the settings were modified.
///
/// This function only mutates the in-memory struct; calling code is responsible
/// for persisting to disk.
pub fn apply_update_to_settings(settings: &mut SettingsJson, update: &PermissionUpdate) -> bool {
    match update {
        PermissionUpdate::AddRules { rules, behavior, .. } => {
            let perms = settings.permissions.get_or_insert_with(Default::default);
            add_rules(perms, rules, *behavior);
            true
        }
        PermissionUpdate::ReplaceRules { rules, behavior, .. } => {
            let perms = settings.permissions.get_or_insert_with(Default::default);
            replace_rules(perms, rules, *behavior);
            true
        }
        PermissionUpdate::RemoveRules { rules, behavior, .. } => {
            let perms = settings.permissions.get_or_insert_with(Default::default);
            remove_rules(perms, rules, *behavior);
            true
        }
        PermissionUpdate::SetMode { mode, .. } => {
            let perms = settings.permissions.get_or_insert_with(Default::default);
            perms.default_mode = Some(*mode);
            true
        }
        PermissionUpdate::AddDirectories { directories, .. } => {
            let perms = settings.permissions.get_or_insert_with(Default::default);
            let existing = perms.additional_directories.get_or_insert_with(Vec::new);
            for dir in directories {
                if !existing.contains(dir) {
                    existing.push(dir.clone());
                }
            }
            true
        }
        PermissionUpdate::RemoveDirectories { directories, .. } => {
            let perms = settings.permissions.get_or_insert_with(Default::default);
            if let Some(existing) = &mut perms.additional_directories {
                existing.retain(|d| !directories.contains(d));
                true
            } else {
                false
            }
        }
    }
}

fn bucket_mut<'a>(
    perms: &'a mut SettingsPermissions,
    behavior: PermissionBehavior,
) -> &'a mut Option<Vec<String>> {
    match behavior {
        PermissionBehavior::Allow => &mut perms.allow,
        PermissionBehavior::Deny => &mut perms.deny,
        PermissionBehavior::Ask => &mut perms.ask,
    }
}

fn add_rules(
    perms: &mut SettingsPermissions,
    rules: &[PermissionRuleValue],
    behavior: PermissionBehavior,
) {
    let bucket = bucket_mut(perms, behavior);
    let list = bucket.get_or_insert_with(Vec::new);
    for rule in rules {
        let s = rule_value_to_string(rule);
        if !list.contains(&s) {
            list.push(s);
        }
    }
}

fn replace_rules(
    perms: &mut SettingsPermissions,
    rules: &[PermissionRuleValue],
    behavior: PermissionBehavior,
) {
    let bucket = bucket_mut(perms, behavior);
    *bucket = Some(rules.iter().map(rule_value_to_string).collect());
}

fn remove_rules(
    perms: &mut SettingsPermissions,
    rules: &[PermissionRuleValue],
    behavior: PermissionBehavior,
) {
    let bucket = bucket_mut(perms, behavior);
    if let Some(list) = bucket {
        let to_remove: Vec<String> = rules.iter().map(rule_value_to_string).collect();
        list.retain(|r| !to_remove.contains(r));
    }
}

// ── File I/O ──────────────────────────────────────────────────────────────────

/// Load, mutate, and atomically save a settings file.
///
/// If the file does not exist, a fresh `SettingsJson` is created.
/// The file is written atomically via a temporary file + rename.
pub async fn persist_update(
    update: &PermissionUpdate,
    cwd: &Path,
) -> anyhow::Result<()> {
    let dest = update_destination(update);
    let Some(path) = settings_path_for_destination(dest, cwd) else {
        // Session / CliArg — nothing to persist.
        return Ok(());
    };

    // Load existing settings (or start fresh).
    let mut settings = match tokio::fs::read(&path).await {
        Ok(bytes) => code_config::settings::parse_settings(&bytes)
            .unwrap_or_default(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SettingsJson::default(),
        Err(e) => return Err(e.into()),
    };

    if !apply_update_to_settings(&mut settings, update) {
        return Ok(()); // nothing changed
    }

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Serialize and write atomically.
    let json = serde_json::to_string_pretty(&settings)?;
    let tmp_path = path.with_extension("json.tmp");
    tokio::fs::write(&tmp_path, json.as_bytes()).await?;
    tokio::fs::rename(&tmp_path, &path).await?;

    tracing::debug!("persisted permission update to {}", path.display());
    Ok(())
}

fn update_destination(update: &PermissionUpdate) -> PermissionUpdateDestination {
    match update {
        PermissionUpdate::AddRules { destination, .. } => *destination,
        PermissionUpdate::ReplaceRules { destination, .. } => *destination,
        PermissionUpdate::RemoveRules { destination, .. } => *destination,
        PermissionUpdate::SetMode { destination, .. } => *destination,
        PermissionUpdate::AddDirectories { destination, .. } => *destination,
        PermissionUpdate::RemoveDirectories { destination, .. } => *destination,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::permissions::{ExternalPermissionMode, PermissionBehavior, PermissionRuleValue, PermissionUpdate, PermissionUpdateDestination};

    fn make_rule(tool: &str, content: Option<&str>) -> PermissionRuleValue {
        PermissionRuleValue {
            tool_name: tool.to_owned(),
            rule_content: content.map(str::to_owned),
        }
    }

    #[test]
    fn add_and_remove_rules() {
        let mut settings = SettingsJson::default();
        let update = PermissionUpdate::AddRules {
            destination: PermissionUpdateDestination::UserSettings,
            rules: vec![make_rule("Bash", Some("git *"))],
            behavior: PermissionBehavior::Allow,
        };
        apply_update_to_settings(&mut settings, &update);
        let allow = settings.permissions.as_ref().unwrap().allow.as_ref().unwrap();
        assert!(allow.contains(&"Bash(git *)".to_owned()));

        let remove = PermissionUpdate::RemoveRules {
            destination: PermissionUpdateDestination::UserSettings,
            rules: vec![make_rule("Bash", Some("git *"))],
            behavior: PermissionBehavior::Allow,
        };
        apply_update_to_settings(&mut settings, &remove);
        let allow = settings.permissions.as_ref().unwrap().allow.as_ref().unwrap();
        assert!(allow.is_empty());
    }

    #[test]
    fn set_mode() {
        let mut settings = SettingsJson::default();
        let update = PermissionUpdate::SetMode {
            destination: PermissionUpdateDestination::ProjectSettings,
            mode: ExternalPermissionMode::AcceptEdits,
        };
        apply_update_to_settings(&mut settings, &update);
        assert_eq!(
            settings.permissions.unwrap().default_mode,
            Some(ExternalPermissionMode::AcceptEdits)
        );
    }
}
