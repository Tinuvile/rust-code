//! File-path permission validation — ensures that tools cannot access paths
//! outside the working directory or additional allowed directories.
//!
//! Ref: src/utils/permissions/pathValidation.ts
//!      src/utils/permissions/filesystem.ts

use std::path::{Path, PathBuf};

use code_types::permissions::ToolPermissionContext;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Whether a file operation reads or writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOperationType {
    Read,
    Write,
    Delete,
}

/// The outcome of a path access check.
#[derive(Debug, Clone)]
pub enum PathCheckResult {
    /// Path is within an allowed directory.
    Allowed,
    /// Path is outside all allowed directories.
    ///
    /// `blocked_path` is the canonical form of the checked path.
    /// `allowed_dirs` lists the directories that were checked.
    Blocked {
        blocked_path: String,
        allowed_dirs: Vec<String>,
    },
}

impl PathCheckResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PathCheckResult::Allowed)
    }
}

// ── Tilde expansion ───────────────────────────────────────────────────────────

/// Expand a leading `~` to the user's home directory.
///
/// Returns `None` if the home directory cannot be determined.
pub fn expand_tilde(path: &str) -> Option<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs_next::home_dir()?;
        Some(home.join(rest))
    } else if path == "~" {
        dirs_next::home_dir()
    } else {
        Some(PathBuf::from(path))
    }
}

/// Expand tilde and return the path as-is if home is unavailable.
pub fn expand_tilde_lossy(path: &str) -> PathBuf {
    expand_tilde(path).unwrap_or_else(|| PathBuf::from(path))
}

// ── Glob base directory extraction ───────────────────────────────────────────

/// Extract the literal base directory from a glob pattern.
///
/// For example, `/home/user/projects/**/*.rs` → `/home/user/projects`.
///
/// Ref: src/utils/permissions/pathValidation.ts getGlobBaseDirectory
pub fn get_glob_base_directory(pattern: &str) -> &str {
    // Find the first glob metacharacter.
    let meta_pos = pattern.find(|c| matches!(c, '*' | '?' | '[' | '{'));
    match meta_pos {
        None => pattern, // no wildcards — the full string is the path
        Some(pos) => {
            let before = &pattern[..pos];
            // Walk back to the last path separator.
            match before.rfind(|c| c == '/' || c == '\\') {
                Some(sep) => &pattern[..sep],
                None => "",
            }
        }
    }
}

// ── Canonical path resolution ─────────────────────────────────────────────────

/// Resolve `path` to an absolute, canonical path.
///
/// Unlike `std::fs::canonicalize`, this does NOT require the path to exist —
/// it resolves `..` components symbolically and returns an absolute path.
pub fn resolve_path(path: &Path, cwd: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    // Resolve `..` and `.` without requiring the path to exist.
    normalize_path(&abs)
}

fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut components: Vec<std::ffi::OsString> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => { components.pop(); }
            Component::CurDir => {}
            Component::RootDir => {
                components.clear();
                components.push(comp.as_os_str().to_owned());
            }
            other => components.push(other.as_os_str().to_owned()),
        }
    }
    components.iter().collect()
}

// ── Access check ──────────────────────────────────────────────────────────────

/// Check whether `target` is within the set of allowed directories.
///
/// Allowed directories are:
/// 1. The current working directory (`cwd`).
/// 2. Any directories in `ctx.additional_working_directories`.
///
/// `target` may be a glob pattern — in that case its base directory is used.
pub fn check_path_access(
    target: &str,
    _op: FileOperationType,
    cwd: &Path,
    ctx: &ToolPermissionContext,
) -> PathCheckResult {
    // Expand tilde.
    let expanded = expand_tilde_lossy(target);
    // If it looks like a glob, use the base directory.
    let effective_path = if is_glob_pattern(target) {
        let base = get_glob_base_directory(target);
        if base.is_empty() {
            expanded.clone()
        } else {
            expand_tilde_lossy(base)
        }
    } else {
        expanded.clone()
    };

    let resolved = resolve_path(&effective_path, cwd);

    // Build the list of allowed directories.
    let mut allowed_dirs: Vec<PathBuf> = vec![cwd.to_path_buf()];
    for awd in &ctx.additional_working_directories {
        allowed_dirs.push(expand_tilde_lossy(&awd.path));
    }

    // Check containment.
    for dir in &allowed_dirs {
        let dir_resolved = resolve_path(dir, cwd);
        if is_subpath(&resolved, &dir_resolved) {
            return PathCheckResult::Allowed;
        }
    }

    PathCheckResult::Blocked {
        blocked_path: resolved.to_string_lossy().into_owned(),
        allowed_dirs: allowed_dirs
            .iter()
            .map(|d| d.to_string_lossy().into_owned())
            .collect(),
    }
}

/// Returns `true` if `path` is equal to or a subdirectory of `dir`.
fn is_subpath(path: &Path, dir: &Path) -> bool {
    path.starts_with(dir)
}

/// Returns `true` if the string contains glob metacharacters.
fn is_glob_pattern(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// Collect all allowed directory path strings from a context.
pub fn allowed_directories(cwd: &Path, ctx: &ToolPermissionContext) -> Vec<String> {
    let mut dirs = vec![cwd.to_string_lossy().into_owned()];
    for awd in &ctx.additional_working_directories {
        dirs.push(awd.path.clone());
    }
    dirs
}

/// Build a human-readable "blocked" message.
pub fn blocked_message(blocked_path: &str, allowed_dirs: &[String]) -> String {
    let dirs = allowed_dirs.join(", ");
    format!(
        "Path '{blocked_path}' is outside the allowed working director{}: {dirs}",
        if allowed_dirs.len() == 1 { "y" } else { "ies" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::permissions::{PermissionMode, ToolPermissionContext};

    fn default_ctx() -> ToolPermissionContext {
        ToolPermissionContext {
            mode: PermissionMode::Default,
            ..Default::default()
        }
    }

    #[test]
    fn allows_subpath() {
        let cwd = PathBuf::from("/home/user/project");
        let ctx = default_ctx();
        let result = check_path_access("src/main.rs", FileOperationType::Read, &cwd, &ctx);
        assert!(result.is_allowed());
    }

    #[test]
    fn blocks_outside_path() {
        let cwd = PathBuf::from("/home/user/project");
        let ctx = default_ctx();
        let result = check_path_access("/etc/passwd", FileOperationType::Read, &cwd, &ctx);
        assert!(!result.is_allowed());
    }

    #[test]
    fn allows_additional_dir() {
        use code_types::permissions::AdditionalWorkingDirectory;
        let cwd = PathBuf::from("/home/user/project");
        let mut ctx = default_ctx();
        ctx.additional_working_directories = vec![AdditionalWorkingDirectory {
            path: "/tmp/shared".to_owned(),
            source: code_types::permissions::PermissionRuleSource::UserSettings,
        }];
        let result = check_path_access("/tmp/shared/data.csv", FileOperationType::Read, &cwd, &ctx);
        assert!(result.is_allowed());
    }

    #[test]
    fn expand_tilde_works() {
        let home = dirs_next::home_dir();
        if home.is_some() {
            let p = expand_tilde("~/foo/bar").unwrap();
            assert!(p.to_string_lossy().contains("foo/bar"));
        }
    }

    #[test]
    fn glob_base() {
        assert_eq!(get_glob_base_directory("/home/user/**/*.rs"), "/home/user");
        assert_eq!(get_glob_base_directory("/etc/hosts"), "/etc/hosts");
        assert_eq!(get_glob_base_directory("*.rs"), "");
    }
}
