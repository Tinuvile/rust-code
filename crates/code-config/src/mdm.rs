//! MDM (Mobile Device Management) settings readers.
//!
//! On macOS: reads from managed preference plists via `plutil`.
//! On Windows: reads from the registry (HKLM and HKCU).
//! On Linux: no MDM equivalent — returns None.
//!
//! Ref: src/utils/settings/mdm/rawRead.ts
//! Ref: src/utils/settings/mdm/settings.ts

#[cfg(target_os = "macos")]
use std::time::Duration;

use crate::settings::SettingsJson;

// ── Platform constants ────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
const MDM_SUBPROCESS_TIMEOUT: Duration = Duration::from_millis(5_000);

/// macOS plist paths (in priority order), mirroring the TypeScript constants.
///
/// Ref: src/utils/settings/mdm/constants.ts getMacOSPlistPaths
#[cfg(target_os = "macos")]
const MACOS_PLIST_PATHS: &[(&str, &str)] = &[
    (
        "/Library/Managed Preferences/com.anthropic.claudecode.plist",
        "system-managed",
    ),
    (
        "~/Library/Managed Preferences/com.anthropic.claudecode.plist",
        "user-managed",
    ),
    (
        "~/Library/Preferences/com.anthropic.claudecode.plist",
        "user-prefs",
    ),
];

/// Windows registry key paths.
///
/// Ref: src/utils/settings/mdm/constants.ts
#[cfg(target_os = "windows")]
const WINDOWS_REG_KEY_HKLM: &str =
    r"SOFTWARE\Policies\Anthropic\ClaudeCode";
#[cfg(target_os = "windows")]
const WINDOWS_REG_KEY_HKCU: &str =
    r"SOFTWARE\Anthropic\ClaudeCode";
#[cfg(target_os = "windows")]
const WINDOWS_REG_VALUE_NAME: &str = "ManagedSettings";

// ── Raw read result ───────────────────────────────────────────────────────────

/// Raw subprocess output from the platform MDM reader.
#[derive(Debug, Default)]
pub struct MdmRawResult {
    /// (stdout, label) tuples for each plist source tried (macOS).
    pub plist_stdouts: Vec<(String, String)>,
    /// Raw JSON string from HKLM (Windows).
    pub hklm_stdout: Option<String>,
    /// Raw JSON string from HKCU (Windows).
    pub hkcu_stdout: Option<String>,
}

// ── macOS implementation ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
async fn read_macos_mdm() -> MdmRawResult {
    use tokio::process::Command;

    let home = std::env::var("HOME").unwrap_or_default();
    let mut result = MdmRawResult::default();

    for (raw_path, label) in MACOS_PLIST_PATHS {
        let path = raw_path.replacen('~', &home, 1);
        if !std::path::Path::new(&path).exists() {
            continue;
        }
        match tokio::time::timeout(MDM_SUBPROCESS_TIMEOUT, async {
            Command::new("/usr/bin/plutil")
                .args(["-convert", "json", "-o", "-", &path])
                .output()
                .await
        })
        .await
        {
            Ok(Ok(output)) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                if !stdout.is_empty() {
                    result.plist_stdouts.push((stdout, label.to_string()));
                    break; // First winner wins
                }
            }
            _ => continue,
        }
    }
    result
}

// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn read_windows_reg_key(root: winreg::RegKey, value_name: &str) -> Option<String> {
    root.get_value::<String, _>(value_name).ok()
}

#[cfg(target_os = "windows")]
async fn read_windows_mdm() -> MdmRawResult {
    use winreg::{enums::*, RegKey};

    let mut result = MdmRawResult::default();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    if let Ok(key) = hklm.open_subkey(WINDOWS_REG_KEY_HKLM) {
        result.hklm_stdout = read_windows_reg_key(key, WINDOWS_REG_VALUE_NAME);
    }
    if let Ok(key) = hkcu.open_subkey(WINDOWS_REG_KEY_HKCU) {
        result.hkcu_stdout = read_windows_reg_key(key, WINDOWS_REG_VALUE_NAME);
    }
    result
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Read raw MDM data from the platform.
///
/// Returns immediately with empty results on Linux.
pub async fn read_mdm_raw() -> MdmRawResult {
    #[cfg(target_os = "macos")]
    {
        read_macos_mdm().await
    }
    #[cfg(target_os = "windows")]
    {
        read_windows_mdm().await
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        MdmRawResult::default()
    }
}

/// Parse raw MDM output into a `SettingsJson`.
///
/// On macOS: first plist stdout wins (already priority-sorted).
/// On Windows: HKLM takes priority over HKCU.
///
/// Ref: src/utils/settings/mdm/settings.ts getMdmSettings
pub fn parse_mdm_settings(raw: &MdmRawResult) -> Option<SettingsJson> {
    // macOS: first plist entry
    if let Some((stdout, _label)) = raw.plist_stdouts.first() {
        return serde_json::from_str(stdout).ok();
    }
    // Windows: HKLM wins over HKCU
    if let Some(ref s) = raw.hklm_stdout {
        if let Ok(settings) = serde_json::from_str(s) {
            return Some(settings);
        }
    }
    if let Some(ref s) = raw.hkcu_stdout {
        if let Ok(settings) = serde_json::from_str(s) {
            return Some(settings);
        }
    }
    None
}

/// High-level helper: read and parse MDM settings in one call.
pub async fn get_mdm_settings() -> Option<SettingsJson> {
    let raw = read_mdm_raw().await;
    parse_mdm_settings(&raw)
}
