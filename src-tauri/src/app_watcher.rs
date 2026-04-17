//! Frontmost-app watcher for Phase-6 app awareness.
//!
//! Polls the frontmost app every 2s on a dedicated thread. When the bundle
//! id changes AND desktop mode is active AND the user has opted into
//! appAwareness, emits a `frontmost_changed` Tauri event. The JS side
//! (app_comments.js via behavior.js) decides what to do with it — comment,
//! throttle, ignore.
//!
//! The gate logic lives in Rust rather than JS so the event isn't fired at
//! all when the user isn't opted in (privacy-first: no bundle IDs leaked to
//! the event bus unnecessarily).

use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::{ModeState, SettingsState};

/// Remembers the last bundle ID we emitted so consecutive identical
/// values (same app frontmost for multiple ticks) don't flood the event bus.
pub struct LastFrontmost(pub Mutex<Option<String>>);

pub fn spawn(app: AppHandle) {
    app.manage(LastFrontmost(Mutex::new(None)));

    std::thread::Builder::new()
        .name("app-watcher".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(2000));

                // Gate: must be desktop mode + user opted in. Read fresh
                // every tick so toggles take effect quickly.
                let (enabled, allowlisted): (bool, bool) = {
                    let settings = match app.try_state::<SettingsState>()
                        .and_then(|s| s.0.lock().ok().map(|g| g.clone()))
                    {
                        Some(s) => s,
                        None => continue,
                    };
                    let mode = app.try_state::<ModeState>()
                        .and_then(|s| s.0.lock().ok().map(|g| g.clone()))
                        .unwrap_or_else(|| "claudeOnly".into());
                    let is_desktop = mode == "desktop";
                    let opted_in = settings.app_awareness.enabled;
                    (is_desktop && opted_in, true)
                };
                let _ = allowlisted;
                if !enabled { continue; }

                #[cfg(target_os = "macos")]
                let current = crate::frontmost_bundle_id();
                #[cfg(not(target_os = "macos"))]
                let current: Option<String> = None;

                let Some(current) = current else { continue; };

                // Skip our own process and Claude — they're handled by the
                // hook/MCP path. The app-awareness feature is specifically
                // for OTHER apps the user is running.
                if current.starts_with("dev.ben4mn.claude-companion") { continue; }
                if current.starts_with("com.anthropic.claudefordesktop") { continue; }

                // Dedup: only emit when the frontmost bundle actually changes.
                let changed = {
                    let last_state = app.state::<LastFrontmost>();
                    let mut guard = match last_state.0.lock() {
                        Ok(g) => g,
                        Err(_) => continue,
                    };
                    let changed = guard.as_deref() != Some(current.as_str());
                    if changed {
                        *guard = Some(current.clone());
                    }
                    changed
                };
                if !changed { continue; }

                let _ = app.emit("frontmost_changed", &current);
            }
        })
        .expect("spawn app-watcher thread");
}
