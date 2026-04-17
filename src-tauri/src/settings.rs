//! Persistent user settings for Claude Companion.
//!
//! Stored as JSON at the app config dir (macOS:
//! `~/Library/Application Support/dev.ben4mn.claude-companion/config.json`).
//!
//! Design notes:
//! - `Settings` is a flat typed struct with `#[serde(default)]` on every sub-
//!   struct so that a config file from an older version, or one missing entire
//!   sections, still loads — unrecognized keys are ignored, missing keys take
//!   defaults. This is the migration strategy: additive-only schema changes
//!   cost nothing, never require explicit migration code.
//! - `load_from` / `save_to` take an explicit path so unit tests use a temp
//!   dir without touching the real config. The Tauri-facing `load()` / `save()`
//!   wrap them with the app-config-dir path.
//! - A malformed JSON file must never panic the app. We fall back to defaults
//!   and let the next save overwrite the corrupt file — better than refusing
//!   to launch because of one stray comma.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub tray: TraySettings,
    pub animation: AnimationSettings,
    pub mode: ModeSettings,
    pub companion: CompanionSettings,
    pub integration: IntegrationSettings,
    pub app_awareness: AppAwarenessSettings,
    pub hotkeys: HotkeySettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tray: TraySettings::default(),
            animation: AnimationSettings::default(),
            mode: ModeSettings::default(),
            companion: CompanionSettings::default(),
            integration: IntegrationSettings::default(),
            app_awareness: AppAwarenessSettings::default(),
            hotkeys: HotkeySettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TraySettings {
    pub visible: bool,
    pub first_disable_warning_shown: bool,
}
impl Default for TraySettings {
    fn default() -> Self {
        Self { visible: true, first_disable_warning_shown: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AnimationSettings {
    pub preset: String,
    pub activity_frequency: f64,
    pub walk_speed: f64,
    pub speech_chattiness: f64,
    pub quiet_hours: QuietHours,
    pub activity_pool: Option<Vec<String>>, // None = all
}
impl Default for AnimationSettings {
    fn default() -> Self {
        Self {
            preset: "normal".into(),
            activity_frequency: 1.0,
            walk_speed: 1.0,
            speech_chattiness: 0.5,
            quiet_hours: QuietHours::default(),
            activity_pool: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct QuietHours {
    pub enabled: bool,
    pub from: String,
    pub to: String,
}
impl Default for QuietHours {
    fn default() -> Self {
        Self { enabled: false, from: "22:00".into(), to: "07:00".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ModeSettings {
    pub mode: String, // "claudeOnly" | "desktop"
}
impl Default for ModeSettings {
    fn default() -> Self { Self { mode: "claudeOnly".into() } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct CompanionSettings {
    pub active_pack: String,
    pub themes: serde_json::Map<String, serde_json::Value>,
}
impl Default for CompanionSettings {
    fn default() -> Self {
        Self { active_pack: "pane".into(), themes: serde_json::Map::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct IntegrationSettings {
    pub ipc: IpcSettings,
    pub hooks: HookSettings,
    pub mcp: McpSettings,
    pub memory: MemorySettings,
}
impl Default for IntegrationSettings {
    fn default() -> Self {
        Self {
            ipc: IpcSettings::default(),
            hooks: HookSettings::default(),
            mcp: McpSettings::default(),
            memory: MemorySettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct IpcSettings { pub enabled: bool, pub port: u16 }
impl Default for IpcSettings {
    fn default() -> Self { Self { enabled: false, port: 48372 } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct HookSettings { pub installed: bool }
impl Default for HookSettings {
    fn default() -> Self { Self { installed: false } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct McpSettings { pub enabled: bool }
impl Default for McpSettings {
    fn default() -> Self { Self { enabled: false } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct MemorySettings { pub enabled: bool, pub paths: Vec<String> }
impl Default for MemorySettings {
    fn default() -> Self {
        Self { enabled: false, paths: vec!["~/.claude/projects".into()] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppAwarenessSettings {
    pub enabled: bool,
    pub allowlist: Vec<String>,
    pub frequency_ms: u64,
}
impl Default for AppAwarenessSettings {
    fn default() -> Self {
        Self { enabled: false, allowlist: vec![], frequency_ms: 45000 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct HotkeySettings {
    pub show_hide: String,
    pub open_settings: String,
    pub quit: String,
}
impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            show_hide: "Cmd+Shift+P".into(),
            open_settings: "Cmd+Shift+,".into(),
            quit: "Cmd+Shift+Q".into(),
        }
    }
}

// ============================================================================
// File I/O — path-injectable so tests can use tempfiles.
// ============================================================================

pub fn load_from(path: &Path) -> Settings {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Settings::default();
    };
    // Corrupt JSON never panics the app — fall back to defaults and let the
    // next write overwrite the bad file. Losing user-tweaked settings once is
    // much better than refusing to launch.
    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_to(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Returns the real config path (`~/Library/Application Support/.../config.json`
/// on macOS). Separated from `load_from` so tests don't need a full Tauri app.
pub fn default_config_path() -> PathBuf {
    // Mirrors Tauri's app_config_dir() logic but without requiring the AppHandle
    // at setup time — we want this callable from anywhere, including the
    // Claude watcher thread. Bundle ID must match tauri.conf.json identifier.
    let bundle_id = "dev.ben4mn.claude-companion";
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        return PathBuf::from(home)
            .join("Library/Application Support")
            .join(bundle_id)
            .join("config.json");
    }
    #[cfg(not(target_os = "macos"))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        return PathBuf::from(home)
            .join(".config")
            .join(bundle_id)
            .join("config.json");
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_file_returns_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("missing.json");
        let s = load_from(&path);
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.json");
        let mut s = Settings::default();
        s.tray.visible = false;
        s.animation.activity_frequency = 0.3;
        s.mode.mode = "desktop".into();
        save_to(&path, &s).expect("save");
        let loaded = load_from(&path);
        assert_eq!(loaded, s);
    }

    #[test]
    fn save_creates_parent_directory_if_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested/dir/config.json");
        save_to(&path, &Settings::default()).expect("save should create parents");
        assert!(path.exists());
    }

    #[test]
    fn malformed_json_falls_back_to_defaults_without_panic() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("bad.json");
        std::fs::write(&path, "this is not json {").unwrap();
        let s = load_from(&path);
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn partial_config_fills_in_defaults_for_missing_sections() {
        // Additive schema migration: a config file written by v0.1 that lacks
        // any knowledge of Phase-5 integration settings should still load, with
        // the integration section taking default values.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("partial.json");
        std::fs::write(&path, r#"{"tray": {"visible": false}}"#).unwrap();
        let s = load_from(&path);
        assert!(!s.tray.visible);
        assert_eq!(s.animation, AnimationSettings::default());
        assert_eq!(s.integration, IntegrationSettings::default());
    }

    #[test]
    fn serializes_to_camel_case_for_js_consumers() {
        // The JS side of the app expects camelCase keys (matching the rest
        // of the Tauri ecosystem's conventions). Validate by spot-checking
        // a couple of the multi-word fields.
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"appAwareness\""));
        assert!(json.contains("\"activityFrequency\""));
        assert!(json.contains("\"firstDisableWarningShown\""));
        assert!(!json.contains("app_awareness"));
    }

    #[test]
    fn defaults_are_stable_and_safe() {
        // Smoke test: defaults should not enable anything privacy-sensitive
        // (hooks, MCP, memory reader, app-awareness all opt-in).
        let s = Settings::default();
        assert!(!s.integration.hooks.installed);
        assert!(!s.integration.mcp.enabled);
        assert!(!s.integration.memory.enabled);
        assert!(!s.app_awareness.enabled);
        assert!(s.tray.visible, "tray visible by default");
        assert_eq!(s.mode.mode, "claudeOnly");
        assert_eq!(s.companion.active_pack, "pane");
    }
}
