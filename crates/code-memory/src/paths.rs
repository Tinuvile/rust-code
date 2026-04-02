//! Paths to CLAUDE.md files and memdir entries.
//!
//! Ref: src/memdir/memoryPaths.ts

use std::path::{Path, PathBuf};

/// Path to the user-level global CLAUDE.md: `~/.claude/CLAUDE.md`.
pub fn global_claude_md_path() -> Option<PathBuf> {
    dirs_next::home_dir().map(|h| h.join(".claude").join("CLAUDE.md"))
}

/// Path to the user-level memdir directory: `~/.claude/memory/`.
pub fn global_memdir_path() -> Option<PathBuf> {
    dirs_next::home_dir().map(|h| h.join(".claude").join("memory"))
}

/// Walk from `cwd` toward the filesystem root, collecting all `CLAUDE.md` paths.
///
/// The first element (index 0) is the one in `cwd`; subsequent elements are in
/// parent directories.  Stops when it reaches the home directory or root.
pub fn project_claude_md_paths(cwd: &Path) -> Vec<(PathBuf, usize)> {
    let home = dirs_next::home_dir();
    let mut paths = Vec::new();
    let mut current = cwd.to_path_buf();
    let mut depth = 0usize;

    loop {
        let candidate = current.join("CLAUDE.md");
        if candidate.exists() {
            paths.push((candidate, depth));
        }

        // Stop at home directory or filesystem root.
        if home.as_deref() == Some(current.as_path()) {
            break;
        }
        match current.parent() {
            Some(p) if p != current => {
                current = p.to_path_buf();
                depth += 1;
            }
            _ => break,
        }
    }

    paths
}

/// Return all `.md` file paths in the global memdir, if it exists.
pub fn memdir_entries() -> Vec<PathBuf> {
    let dir = match global_memdir_path() {
        Some(d) if d.is_dir() => d,
        _ => return Vec::new(),
    };

    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().map(|e| e == "md").unwrap_or(false) {
                paths.push(p);
            }
        }
    }
    paths.sort();
    paths
}
