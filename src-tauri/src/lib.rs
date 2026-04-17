mod app_watcher;
mod dock;
mod hotkeys;
mod ipc;
mod memory;
mod occlusion;
mod settings;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    Emitter, LogicalPosition, Manager,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartManagerExt};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

use crate::hotkeys::{diff_hotkeys, initial_registrations, HotkeyAction};
use crate::settings::Settings;
use crate::tray::{compute_tray_action, should_show_first_disable_warning, TrayAction};

/// The tray icon is stored in app state so we can show/hide it on the fly when
/// the user toggles `tray.visible`. Without this handle we'd have to rebuild
/// the entire tray each time, which is flickery and resets menu state.
pub struct TrayState(pub Mutex<Option<TrayIcon>>);

/// Current run mode, read by the watcher thread every tick. Writes go through
/// `settings_save`. Valid values: "claudeOnly" | "desktop". We use a string
/// here (not an enum) so the JSON shape and the in-memory shape match without
/// a translation layer.
pub struct ModeState(pub Mutex<String>);

/// "Keep Pane visible until this Instant" — bumped when a reaction (MCP or
/// hook) needs the user to actually see Pane, even if he'd normally be
/// occluded. The watcher checks it before applying the usual occlusion hide
/// logic.
pub struct ReactionOverride(pub Mutex<Option<std::time::Instant>>);

/// Shared settings state. Mutex is fine here — writes are rare (user toggles
/// a UI control) and reads happen at startup + on demand, not in hot paths.
pub struct SettingsState(pub Mutex<Settings>);

#[tauri::command]
fn settings_all(state: tauri::State<SettingsState>) -> Result<Settings, String> {
    Ok(state.0.lock().map_err(|e| e.to_string())?.clone())
}

/// Replace the entire settings blob, persist to disk, and emit
/// `settings_changed` so every window picks up the new values.
///
/// Also applies side effects driven by settings changes:
///   - tray visibility toggles the tray icon
///   - hotkey changes re-bind the global shortcut plugin
#[tauri::command]
fn settings_save(
    app: tauri::AppHandle,
    state: tauri::State<SettingsState>,
    tray_state: tauri::State<TrayState>,
    mode_state: tauri::State<ModeState>,
    settings: Settings,
) -> Result<(), String> {
    let path = settings::default_config_path();
    settings::save_to(&path, &settings).map_err(|e| e.to_string())?;

    let previous = {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        let prev = guard.clone();
        *guard = settings.clone();
        prev
    };

    // Tray visibility side effect.
    let action = compute_tray_action(previous.tray.visible, settings.tray.visible);
    if action != TrayAction::None {
        if let Ok(tray_guard) = tray_state.0.lock() {
            if let Some(tray) = tray_guard.as_ref() {
                let _ = tray.set_visible(action == TrayAction::Show);
            }
        }
    }

    // Hotkey rebinds. We still call the plugin even if the user only edited
    // a different subsection — the diff is cheap and a no-op when nothing
    // changed.
    let diff = diff_hotkeys(&previous.hotkeys, &settings.hotkeys);
    if !diff.to_unregister.is_empty() || !diff.to_register.is_empty() {
        apply_hotkey_diff(&app, &diff);
    }

    // Mode side effect — the watcher thread polls this on every tick, so a
    // mode change takes effect within 350ms without any thread restart.
    if previous.mode.mode != settings.mode.mode {
        if let Ok(mut m) = mode_state.0.lock() {
            *m = settings.mode.mode.clone();
        }
    }

    // Autostart side effect: write/remove the LaunchAgent when the user
    // toggles the setting. apply_autostart is a no-op when the desired
    // state already matches the OS state, so this stays cheap on unrelated
    // saves.
    if previous.autostart != settings.autostart {
        apply_autostart(&app, settings.autostart);
    }

    // The warning dialog itself is shown from the JS side (General tab) —
    // Rust just persists the acknowledgement flag the UI writes.
    let _ = should_show_first_disable_warning(&previous.tray, &settings.tray);

    app.emit("settings_changed", &settings).map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_shortcut(accelerator: &str) -> Option<Shortcut> {
    // Normalize the user-facing "Cmd+Shift+P" syntax into the plugin's
    // Shortcut struct. Supports Cmd/Command/Meta (all map to SUPER on macOS),
    // Ctrl, Shift, Alt/Option — plus a single letter, digit, or named key.
    let parts: Vec<&str> = accelerator.split('+').map(str::trim).collect();
    if parts.is_empty() { return None; }
    let mut modifiers = Modifiers::empty();
    let mut code: Option<Code> = None;
    for part in &parts {
        let lower = part.to_ascii_lowercase();
        match lower.as_str() {
            "cmd" | "command" | "meta" | "super" => modifiers |= Modifiers::SUPER,
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            other => {
                code = name_to_code(other);
                if code.is_none() { return None; }
            }
        }
    }
    code.map(|c| Shortcut::new(Some(modifiers), c))
}

fn name_to_code(name: &str) -> Option<Code> {
    // Minimal mapping — every hotkey we ship uses one of these.
    Some(match name {
        "a" => Code::KeyA, "b" => Code::KeyB, "c" => Code::KeyC, "d" => Code::KeyD,
        "e" => Code::KeyE, "f" => Code::KeyF, "g" => Code::KeyG, "h" => Code::KeyH,
        "i" => Code::KeyI, "j" => Code::KeyJ, "k" => Code::KeyK, "l" => Code::KeyL,
        "m" => Code::KeyM, "n" => Code::KeyN, "o" => Code::KeyO, "p" => Code::KeyP,
        "q" => Code::KeyQ, "r" => Code::KeyR, "s" => Code::KeyS, "t" => Code::KeyT,
        "u" => Code::KeyU, "v" => Code::KeyV, "w" => Code::KeyW, "x" => Code::KeyX,
        "y" => Code::KeyY, "z" => Code::KeyZ,
        "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2,
        "3" => Code::Digit3, "4" => Code::Digit4, "5" => Code::Digit5,
        "6" => Code::Digit6, "7" => Code::Digit7, "8" => Code::Digit8,
        "9" => Code::Digit9,
        "," => Code::Comma, "." => Code::Period, "/" => Code::Slash,
        ";" => Code::Semicolon, "'" => Code::Quote, "\\" => Code::Backslash,
        "-" => Code::Minus, "=" => Code::Equal,
        "space" | " " => Code::Space,
        "enter" | "return" => Code::Enter,
        "escape" | "esc" => Code::Escape,
        "tab" => Code::Tab,
        "left" => Code::ArrowLeft, "right" => Code::ArrowRight,
        "up" => Code::ArrowUp, "down" => Code::ArrowDown,
        _ => return None,
    })
}

fn apply_hotkey_diff(app: &tauri::AppHandle, diff: &crate::hotkeys::HotkeyDiff) {
    let gs = app.global_shortcut();
    for acc in &diff.to_unregister {
        if let Some(s) = parse_shortcut(acc) {
            let _ = gs.unregister(s);
        }
    }
    for (_action, acc) in &diff.to_register {
        if let Some(s) = parse_shortcut(acc) {
            let _ = gs.register(s);
        }
    }
}

fn hotkey_action_for(
    settings: &Settings,
    pressed: &Shortcut,
) -> Option<HotkeyAction> {
    // Map a plugin-raised shortcut back to a logical action by string compare
    // against the current settings. Cheap — three comparisons — and keeps us
    // decoupled from the plugin's per-registration handler story.
    let candidates = [
        (HotkeyAction::ShowHide, &settings.hotkeys.show_hide),
        (HotkeyAction::OpenSettings, &settings.hotkeys.open_settings),
        (HotkeyAction::Quit, &settings.hotkeys.quit),
    ];
    for (action, acc) in candidates {
        if let Some(s) = parse_shortcut(acc) {
            if s == *pressed { return Some(action); }
        }
    }
    None
}

#[tauri::command]
fn settings_path() -> String {
    settings::default_config_path().to_string_lossy().into_owned()
}

/// Install the companion's hook bridge into the user's Claude Code settings.
///
/// Writes a `hooks` block to `~/.claude/settings.json` (merging with any
/// existing content, never clobbering unrelated keys) that wires four Claude
/// Code events to our bundled `companion-event` CLI:
///   - PreToolUse  → Pane animates typing
///   - PostToolUse → Pane animates writing
///   - Notification → Pane looks concerned
///   - Stop → Pane waves goodbye
///
/// Returns the absolute path written to (for display in the Settings UI).
#[tauri::command]
fn install_claude_hooks(app: tauri::AppHandle) -> Result<String, String> {
    let Some(home) = dirs::home_dir() else {
        return Err("no HOME directory".into());
    };
    let path = home.join(".claude").join("settings.json");
    let bridge_path = bridge_binary_path(&app)?;

    // Load existing file if any. Don't crash on malformed JSON — we'll start
    // fresh rather than lose their file, but the caller should be cautious.
    let existing = std::fs::read_to_string(&path).ok();
    let mut root: serde_json::Value = existing
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    if !root.is_object() {
        root = serde_json::json!({});
    }

    let cmd = format!("{} --event", bridge_path.to_string_lossy());
    let hooks_block = serde_json::json!({
        "PreToolUse":  [{ "hooks": [{ "type": "command", "command": format!("{} PreToolUse",  cmd) }] }],
        "PostToolUse": [{ "hooks": [{ "type": "command", "command": format!("{} PostToolUse", cmd) }] }],
        "Notification":[{ "hooks": [{ "type": "command", "command": format!("{} Notification", cmd) }] }],
        "Stop":        [{ "hooks": [{ "type": "command", "command": format!("{} Stop",        cmd) }] }],
    });

    root.as_object_mut().unwrap().insert("hooks".into(), hooks_block);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?,
    ).map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().into_owned())
}

/// Uninstall the companion's hooks from Claude Code settings. Removes the
/// top-level `hooks` block if it matches our bridge path; otherwise leaves
/// the user's manual hooks alone.
#[tauri::command]
fn uninstall_claude_hooks() -> Result<String, String> {
    let Some(home) = dirs::home_dir() else {
        return Err("no HOME directory".into());
    };
    let path = home.join(".claude").join("settings.json");
    let Ok(existing) = std::fs::read_to_string(&path) else {
        return Ok("no settings file".into());
    };
    let mut root: serde_json::Value = serde_json::from_str(&existing)
        .unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = root.as_object_mut() {
        obj.remove("hooks");
    }
    std::fs::write(&path, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

fn bridge_binary_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // In a release bundle the companion CLI sits next to the main binary
    // inside the .app's Resources or MacOS folder. In `tauri dev` we're
    // running out of target/debug alongside the dev binary.
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no exe dir")?;
    let path = dir.join("companion-event");
    if path.exists() { return Ok(path); }
    // Fallback for dev: try the target/debug cousin.
    if let Some(app_dir) = app.path().resource_dir().ok() {
        let p = app_dir.join("companion-event");
        if p.exists() { return Ok(p); }
    }
    // Last resort: assume it's on PATH.
    Ok(std::path::PathBuf::from("companion-event"))
}

/// Read all memory files from the user's ~/.claude tree and return a flat
/// list of fact strings. Called on-demand by the frontend — cheap enough to
/// re-invoke occasionally (we cap at 200 facts).
#[tauri::command]
fn memory_lines(state: tauri::State<SettingsState>) -> Vec<String> {
    // Respect the user's opt-in — if they haven't enabled the memory
    // reader, return nothing. This is the entire privacy surface for
    // this feature.
    let enabled = state
        .0
        .lock()
        .ok()
        .map(|g| g.integration.memory.enabled)
        .unwrap_or(false);
    if !enabled { return Vec::new(); }
    memory::scan_all(200)
        .into_iter()
        .map(|f| f.text)
        .collect()
}

/// Generate the MCP config snippet the user pastes into their Claude Code
/// MCP config. Kept for copy-to-clipboard fallback when auto-install isn't
/// wanted.
#[tauri::command]
fn mcp_config_json() -> Result<String, String> {
    let mcp_path = mcp_binary_path()?;
    let snippet = serde_json::json!({
        "mcpServers": {
            "companion": {
                "command": mcp_path.to_string_lossy(),
                "args": []
            }
        }
    });
    serde_json::to_string_pretty(&snippet).map_err(|e| e.to_string())
}

/// Auto-install the MCP server into the user's Claude Code config. Writes
/// to `~/.claude.json` under `mcpServers.companion`, merging with
/// anything already there (never clobbering the user's other servers).
/// Returns the path written to.
#[tauri::command]
fn install_mcp_config() -> Result<String, String> {
    let Some(home) = dirs::home_dir() else {
        return Err("no HOME directory".into());
    };
    let path = home.join(".claude.json");
    let mcp_path = mcp_binary_path()?;

    let existing = std::fs::read_to_string(&path).ok();
    let mut root: serde_json::Value = existing
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !root.is_object() { root = serde_json::json!({}); }

    let obj = root.as_object_mut().unwrap();
    if !obj.get("mcpServers").map(|v| v.is_object()).unwrap_or(false) {
        obj.insert("mcpServers".into(), serde_json::json!({}));
    }
    let servers = obj["mcpServers"].as_object_mut().unwrap();
    // Remove the legacy key so users upgrading from pre-rebrand installs don't
    // end up with two Companion entries registered with Claude Code.
    servers.remove("claude-companion");
    servers.insert(
        "companion".into(),
        serde_json::json!({
            "command": mcp_path.to_string_lossy(),
            "args": []
        }),
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?,
    ).map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().into_owned())
}

/// Remove the companion's MCP entry from Claude Code's user config, leaving
/// any other MCP servers untouched.
#[tauri::command]
fn uninstall_mcp_config() -> Result<String, String> {
    let Some(home) = dirs::home_dir() else {
        return Err("no HOME directory".into());
    };
    let path = home.join(".claude.json");
    let Ok(existing) = std::fs::read_to_string(&path) else {
        return Ok("no config file".into());
    };
    let mut root: serde_json::Value = serde_json::from_str(&existing)
        .unwrap_or_else(|_| serde_json::json!({}));
    if let Some(servers) = root.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        servers.remove("companion");
        servers.remove("claude-companion");
    }
    std::fs::write(&path, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

fn mcp_binary_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no exe dir")?;
    let path = dir.join("companion-mcp");
    if path.exists() { return Ok(path); }
    Ok(std::path::PathBuf::from("companion-mcp"))
}

/// Write `text` to the system clipboard. Used by the Integration tab's
/// copy-config fallback — `navigator.clipboard.writeText` is unreliable
/// inside Tauri webviews (permission/focus quirks).
#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(())
}

/// Called from JS on every incoming hook/MCP event. Pushes Pane's "stay
/// visible" override out by 5 seconds so the user can actually see the
/// speech bubble or animation even if another window is currently occluding
/// Claude. Also calls show() immediately so the reaction isn't invisible
/// until the next watcher tick.
#[tauri::command]
fn request_reaction_window(
    app: tauri::AppHandle,
    override_state: tauri::State<ReactionOverride>,
) -> Result<(), String> {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(5000);
    if let Ok(mut guard) = override_state.0.lock() {
        *guard = Some(until);
    }
    if let Some(w) = app.get_webview_window("companion") {
        let _ = w.show();
    }
    Ok(())
}

/// Emit a fake hook event for end-to-end diagnostic testing — verifies the
/// JS reaction pipeline without needing Claude Code to fire a real hook.
/// `kind` is one of: "PreToolUse" | "PostToolUse" | "Notification" | "Stop".
#[tauri::command]
fn send_test_hook_event(app: tauri::AppHandle, kind: String) -> Result<(), String> {
    let payload = match kind.as_str() {
        "PreToolUse"  => serde_json::json!({ "type": "PreToolUse",  "payload": { "tool": "Bash" } }),
        "PostToolUse" => serde_json::json!({ "type": "PostToolUse", "payload": { "tool": "Edit" } }),
        "Notification"=> serde_json::json!({ "type": "Notification","payload": null }),
        "Stop"        => serde_json::json!({ "type": "Stop",        "payload": null }),
        _             => return Err(format!("unknown test kind: {kind}")),
    };
    app.emit("hook_event", &payload).map_err(|e| e.to_string())
}

/// Show a small native popup menu at the cursor on right-click of the
/// companion. Mirrors the tray menu (Settings / Hide / Quit) so users who've
/// disabled the tray still have a way in.
#[tauri::command]
fn show_companion_menu(app: tauri::AppHandle) -> Result<(), String> {
    let settings_item =
        MenuItem::with_id(&app, "ctx_settings", "Settings\u{2026}", true, None::<&str>)
            .map_err(|e| e.to_string())?;
    let hide_item = MenuItem::with_id(&app, "ctx_hide", "Hide", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit_item = MenuItem::with_id(&app, "ctx_quit", "Quit", true, None::<&str>)
        .map_err(|e| e.to_string())?;

    let menu = Menu::with_items(&app, &[&settings_item, &hide_item, &quit_item])
        .map_err(|e| e.to_string())?;

    if let Some(w) = app.get_webview_window("companion") {
        let _ = w.popup_menu(&menu);
    }
    Ok(())
}

fn handle_ctx_menu_event(app: &tauri::AppHandle, id: &str) {
    match id {
        "ctx_settings" => {
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "ctx_hide" => {
            if let Some(w) = app.get_webview_window("companion") {
                let _ = w.hide();
            }
        }
        "ctx_quit" => app.exit(0),
        _ => {}
    }
}

fn handle_tray_menu_event(app: &tauri::AppHandle, id: &str) {
    match id {
        "show" => {
            if let Some(w) = app.get_webview_window("companion") {
                #[cfg(target_os = "macos")]
                if let Some((x, y, cw, ch)) = find_claude_window() {
                    let size = w.outer_size().ok();
                    let scale = w.scale_factor().unwrap_or(1.0);
                    let (comp_w, comp_h) = size
                        .map(|s| (s.width as f64 / scale, s.height as f64 / scale))
                        .unwrap_or((120.0, 160.0));
                    let target_x = (x + cw - comp_w - 56.0).round();
                    let target_y = (y + ch - comp_h - 56.0).round();
                    let _ = w.set_position(LogicalPosition::new(target_x, target_y));
                }
                let _ = w.show();
            }
        }
        "hide" => {
            if let Some(w) = app.get_webview_window("companion") {
                let _ = w.hide();
            }
        }
        "pet" => {
            if let Some(w) = app.get_webview_window("companion") {
                let _ = w.eval(
                    "window.__pane && window.__pane.say(\
                        ['Oh! Hi.','Hey there.','That tickles.']\
                            [Math.floor(Math.random()*3)])",
                );
            }
        }
        "settings" => {
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

fn handle_hotkey_action(app: &tauri::AppHandle, action: HotkeyAction) {
    match action {
        HotkeyAction::ShowHide => {
            if let Some(w) = app.get_webview_window("companion") {
                let visible = w.is_visible().unwrap_or(false);
                if visible { let _ = w.hide(); }
                else { let _ = w.show(); }
            }
        }
        HotkeyAction::OpenSettings => {
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        HotkeyAction::Quit => app.exit(0),
    }
}

/// Set by JS while Pane is in HELD or FALLING state. The Claude watcher
/// respects this and does not hide / reposition the window — otherwise
/// its grace-period timeout can fire mid-drag and yank Pane back to his
/// "home" spot, which was the bug.
static INTERACTING: AtomicBool = AtomicBool::new(false);

/// Called from JS on mousedown (with true) and after the fall settles
/// (with false). While true, the watcher leaves Pane completely alone.
#[tauri::command]
fn pane_set_interacting(active: bool) {
    INTERACTING.store(active, Ordering::Relaxed);
}

/// On macOS, switch the process to an "accessory" app — no Dock icon, no
/// entry in the app switcher, no menu bar. This is the difference between
/// "Companion is a separate app" and "there's just a little overlay
/// floating on my desktop."
#[cfg(target_os = "macos")]
fn set_accessory_activation_policy() {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    // Safety: called once from the main thread during setup.
    unsafe {
        let mtm = objc2_foundation::MainThreadMarker::new_unchecked();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
}

/// Hit-test: is the global mouse cursor inside the given bbox (window-local
/// logical coords, top-left origin, as given by getBoundingClientRect in JS)?
///
/// Used by the renderer to drive per-pixel click-through: when the cursor is
/// over Pane himself we accept events; otherwise we pass clicks through to
/// whatever app is behind us (Claude Desktop).
/// Return the cursor position in logical pixels with top-left origin — the
/// same coordinate system the browser sees and that Tauri's LogicalPosition
/// expects for set_position. Consolidates the screen-height flip so the
/// JS doesn't have to know about macOS's bottom-origin convention.
#[cfg(target_os = "macos")]
fn cursor_logical() -> Result<(f64, f64), String> {
    use objc2_app_kit::{NSEvent, NSScreen};
    use objc2_foundation::MainThreadMarker;

    let mtm = MainThreadMarker::new().ok_or("not on main thread")?;
    let mouse = NSEvent::mouseLocation();
    let screens = NSScreen::screens(mtm);
    let primary = screens.firstObject().ok_or("no screen")?;
    let screen_height = primary.frame().size.height;
    Ok((mouse.x, screen_height - mouse.y))
}

/// Capture the drag-grab offset at mousedown using Rust's cursor/window
/// coordinate system — the SAME system pane_follow_cursor reads from. This
/// avoids the coord mismatch that happens when JS captures `e.screenX/Y`
/// (CSS-screen pixels) and Rust reads `NSEvent.mouseLocation` (macOS global
/// points with bottom origin flipped): on Retina or multi-monitor setups the
/// two can disagree, making Pane jump by tens of pixels the moment the user
/// picks him up.
///
/// Returns (offset_x, offset_y) — how far the cursor is from the window's
/// top-left, in the same logical-pixel space set_position expects.
#[tauri::command]
fn pane_drag_start(window: tauri::WebviewWindow) -> Result<(f64, f64), String> {
    #[cfg(target_os = "macos")]
    {
        let (mx, my) = cursor_logical()?;
        let win_pos = window.outer_position().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let wx = win_pos.x as f64 / scale;
        let wy = win_pos.y as f64 / scale;
        return Ok((mx - wx, my - wy));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Err("unsupported platform".into())
    }
}

/// Follow the cursor: set the window's top-left so the cursor stays at a
/// fixed offset inside Pane. Used by the JS HELD loop at 60fps.
///
/// Consolidating cursor-read + set-position into one IPC call halves the
/// round-trips per frame and keeps the motion smooth.
#[tauri::command]
fn pane_follow_cursor(
    window: tauri::WebviewWindow,
    offset_x: f64,
    offset_y: f64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let (mx, my) = cursor_logical()?;
        let target_x = mx - offset_x;
        let target_y = my - offset_y;
        window
            .set_position(LogicalPosition::new(target_x, target_y))
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, offset_x, offset_y);
        Err("unsupported platform".into())
    }
}

/// Set the window position in logical pixels. Used by the FALLING gravity
/// animation and the GROUNDED walk animation.
#[tauri::command]
fn pane_set_position(window: tauri::WebviewWindow, x: f64, y: f64) -> Result<(), String> {
    window
        .set_position(LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

/// Report Pane's "ground line": the Y his top-left should settle at, and the
/// X range he can walk within. In `claudeOnly` mode this is derived from
/// Claude's window; in `desktop` mode it's derived from the primary screen's
/// visible frame (bottom Dock walks the screen width). Returns None when
/// there's no valid reference (e.g. Claude missing in claudeOnly mode).
#[tauri::command]
fn pane_ground(
    window: tauri::WebviewWindow,
    mode_state: tauri::State<ModeState>,
) -> Result<Option<(f64, f64, f64)>, String> {
    let mode = mode_state
        .0
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_else(|| "claudeOnly".into());

    #[cfg(target_os = "macos")]
    {
        let size = window.outer_size().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let comp_w = size.width as f64 / scale;
        let comp_h = size.height as f64 / scale;

        match mode.as_str() {
            "desktop" => {
                let Some(vf) = visible_frame() else { return Ok(None); };
                let orientation = read_dock_orientation();
                let pane = dock::PaneSize { w: comp_w, h: comp_h };
                let range = dock::desktop_walk_range(vf, orientation, pane);
                // Bottom Dock → horizontal walk. Side Docks return a
                // degenerate X range so Pane stands anchored at the Dock
                // edge (vertical walking is a later enhancement).
                if range.horizontal {
                    return Ok(Some((range.fixed, range.min, range.max)));
                } else {
                    return Ok(Some((range.fixed, range.fixed, range.fixed)));
                }
            }
            _ => {
                let Some((cx, cy, cw, ch)) = find_claude_window() else {
                    return Ok(None);
                };
                let ground_y = cy + ch - comp_h;
                let min_x = cx;
                let max_x = cx + cw - comp_w;
                return Ok(Some((ground_y, min_x, max_x)));
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, mode);
        Ok(None)
    }
}

#[tauri::command]
fn cursor_over(window: tauri::WebviewWindow, bbox: Vec<f64>) -> Result<bool, String> {
    if bbox.len() != 4 {
        return Err("bbox must be [x, y, width, height]".into());
    }
    let (bx, by, bw, bh) = (bbox[0], bbox[1], bbox[2], bbox[3]);

    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSEvent, NSScreen};
        use objc2_foundation::MainThreadMarker;

        let mtm = MainThreadMarker::new().ok_or("not on main thread")?;
        let mouse = NSEvent::mouseLocation();

        let screens = NSScreen::screens(mtm);
        let primary = screens.firstObject().ok_or("no screen")?;
        let screen_height = primary.frame().size.height;

        let global_x = mouse.x;
        let global_y = screen_height - mouse.y;

        let win_pos = window.outer_position().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let win_x_logical = win_pos.x as f64 / scale;
        let win_y_logical = win_pos.y as f64 / scale;

        let local_x = global_x - win_x_logical;
        let local_y = global_y - win_y_logical;

        return Ok(
            local_x >= bx && local_x <= bx + bw && local_y >= by && local_y <= by + bh,
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, bx, by, bw, bh);
        Ok(false)
    }
}

// ============================================================================
// Claude Desktop watcher: positions Pane bottom-right of Claude's window,
// hides him when Claude is not frontmost, quits when Claude quits.
// ============================================================================

/// Claude Desktop's bundle ID (for NSRunningApplication lookups) and process
/// display name (for CGWindow owner-name match). These are the two handles
/// we get from AppKit and CoreGraphics respectively.
const CLAUDE_BUNDLE_ID: &str = "com.anthropic.claudefordesktop";
const CLAUDE_OWNER_NAME: &str = "Claude";
/// Must match `identifier` in tauri.conf.json. Used so that clicking Pane
/// (which briefly makes us the frontmost app) doesn't trigger a hide.
const COMPANION_BUNDLE_ID: &str = settings::COMPANION_BUNDLE_ID;
/// CGWindow owner-name for this app's process, used to skip Pane's own window
/// when detecting occluders. Must match productName in tauri.conf.json.
const COMPANION_OWNER_NAME: &str = "Companion";

#[cfg(target_os = "macos")]
fn is_claude_running() -> bool {
    use objc2_app_kit::NSWorkspace;
    unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        let apps = ws.runningApplications();
        for app in apps.iter() {
            if let Some(bid) = app.bundleIdentifier() {
                // Match the main app or any Electron helper
                // (com.anthropic.claudefordesktop.helper[.Renderer|.GPU|.Plugin]).
                // Any of these present means Claude is alive.
                if bid.to_string().starts_with(CLAUDE_BUNDLE_ID) {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns which app is frontmost, as an enum we care about: Claude, ourselves,
/// or "something else." Clicking Pane briefly makes the companion frontmost —
/// we treat that the same as Claude-active (keep visible) but don't reposition.
#[cfg(target_os = "macos")]
#[derive(PartialEq, Eq, Clone, Copy)]
enum Frontmost {
    Claude,
    Companion,
    Other,
}


#[cfg(target_os = "macos")]
fn frontmost_app() -> Frontmost {
    use objc2_app_kit::NSWorkspace;
    unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        if let Some(app) = ws.frontmostApplication() {
            if let Some(bid) = app.bundleIdentifier() {
                let s = bid.to_string();
                // Prefix match covers Electron helpers (.helper, .helper.Renderer,
                // .helper.GPU, .helper.Plugin). When Claude's cowork/code view
                // briefly hands focus to a helper, we still want to stay visible.
                if s.starts_with(CLAUDE_BUNDLE_ID) {
                    return Frontmost::Claude;
                }
                if s == COMPANION_BUNDLE_ID {
                    return Frontmost::Companion;
                }
            }
        }
    }
    Frontmost::Other
}

/// Return the frontmost app's raw bundle identifier (or None if no
/// frontmost app / no bundle id). Used by the app-awareness watcher —
/// the enum above only classifies; we need the exact string to look up
/// per-app commentary.
#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        let bid = app.bundleIdentifier()?;
        Some(bid.to_string())
    }
}

/// PID of Claude Desktop's main process (not a helper). Used as a tiebreaker
/// when find_claude_window sees multiple candidate windows — we want the one
/// owned by the main app, not any ancillary process.
#[cfg(target_os = "macos")]
fn claude_main_pid() -> Option<i32> {
    use objc2_app_kit::NSWorkspace;
    unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        let apps = ws.runningApplications();
        for app in apps.iter() {
            if let Some(bid) = app.bundleIdentifier() {
                if bid.to_string() == CLAUDE_BUNDLE_ID {
                    return Some(app.processIdentifier());
                }
            }
        }
    }
    None
}

/// Locate Claude Desktop's main on-screen window via CGWindowListCopyWindowInfo.
/// Returns (x, y, width, height) in macOS screen points (top-left origin).
/// Skips off-screen, minimized, and non-standard-layer windows, and rejects
/// anything smaller than a real app window (tooltips, popovers, tray icons).
///
/// CGWindowListCopyWindowInfo returns windows in z-order (front→back), so the
/// first valid match is usually correct. When Claude has multiple windows we
/// prefer the one owned by the main Claude PID over any other candidate.
#[cfg(target_os = "macos")]
fn find_claude_window() -> Option<(f64, f64, f64, f64)> {
    use core_foundation::array::CFArray;
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        kCGNullWindowID, kCGWindowListOptionOnScreenOnly, CGWindowListCopyWindowInfo,
    };

    let array_ref = unsafe {
        CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)
    };
    if array_ref.is_null() {
        return None;
    }
    let list: CFArray<CFDictionary> =
        unsafe { CFArray::wrap_under_create_rule(array_ref) };

    let owner_key = CFString::from_static_string("kCGWindowOwnerName");
    let pid_key = CFString::from_static_string("kCGWindowOwnerPID");
    let layer_key = CFString::from_static_string("kCGWindowLayer");
    let bounds_key = CFString::from_static_string("kCGWindowBounds");
    let x_key = CFString::from_static_string("X");
    let y_key = CFString::from_static_string("Y");
    let w_key = CFString::from_static_string("Width");
    let h_key = CFString::from_static_string("Height");

    let main_pid = claude_main_pid();
    let mut first_match: Option<(f64, f64, f64, f64)> = None;

    for i in 0..list.len() {
        let Some(dict_ptr) = list.get(i) else { continue };
        let dict: &CFDictionary = &*dict_ptr;

        // Filter by owner name
        let owner_ptr = dict.find(owner_key.as_concrete_TypeRef() as *const _);
        let Some(owner_ptr) = owner_ptr else { continue };
        let owner: CFString =
            unsafe { CFString::wrap_under_get_rule(*owner_ptr as _) };
        if owner.to_string() != CLAUDE_OWNER_NAME {
            continue;
        }

        // Layer 0 = normal app windows
        if let Some(layer_ptr) = dict.find(layer_key.as_concrete_TypeRef() as *const _) {
            let layer: CFNumber = unsafe { CFNumber::wrap_under_get_rule(*layer_ptr as _) };
            if layer.to_i32().unwrap_or(0) != 0 {
                continue;
            }
        }

        // Extract bounds dict
        let bounds_ptr = dict.find(bounds_key.as_concrete_TypeRef() as *const _);
        let Some(bounds_ptr) = bounds_ptr else { continue };
        let bounds: CFDictionary =
            unsafe { CFDictionary::wrap_under_get_rule(*bounds_ptr as _) };

        let get = |k: &CFString| -> Option<f64> {
            let p = bounds.find(k.as_concrete_TypeRef() as *const _)?;
            let num: CFNumber = unsafe { CFNumber::wrap_under_get_rule(*p as _) };
            num.to_f64()
        };

        let (Some(x), Some(y), Some(w), Some(h)) =
            (get(&x_key), get(&y_key), get(&w_key), get(&h_key))
        else {
            continue;
        };

        // Reject small / off-screen things.
        if w < 200.0 || h < 200.0 {
            continue;
        }

        // Prefer main-PID window; remember first otherwise as fallback.
        if let (Some(target_pid), Some(pid_ptr)) =
            (main_pid, dict.find(pid_key.as_concrete_TypeRef() as *const _))
        {
            let pid: CFNumber = unsafe { CFNumber::wrap_under_get_rule(*pid_ptr as _) };
            if pid.to_i32() == Some(target_pid) {
                return Some((x, y, w, h));
            }
        }
        if first_match.is_none() {
            first_match = Some((x, y, w, h));
        }
    }
    first_match
}

/// Scan the on-screen window list (front→back z-order) and collect any
/// non-Claude, non-Pane windows that appear *above* Claude's main window
/// and would therefore occlude Pane if they overlap his bbox. Returns
/// (claude_rect, occluders_above) — caller does the intersection test.
#[cfg(target_os = "macos")]
fn find_claude_and_occluders_above() -> Option<(occlusion::Rect, Vec<occlusion::Rect>)> {
    use core_foundation::array::CFArray;
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        kCGNullWindowID, kCGWindowListOptionOnScreenOnly, CGWindowListCopyWindowInfo,
    };

    let array_ref = unsafe {
        CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)
    };
    if array_ref.is_null() {
        return None;
    }
    let list: CFArray<CFDictionary> = unsafe { CFArray::wrap_under_create_rule(array_ref) };

    let owner_key = CFString::from_static_string("kCGWindowOwnerName");
    let layer_key = CFString::from_static_string("kCGWindowLayer");
    let bounds_key = CFString::from_static_string("kCGWindowBounds");
    let x_key = CFString::from_static_string("X");
    let y_key = CFString::from_static_string("Y");
    let w_key = CFString::from_static_string("Width");
    let h_key = CFString::from_static_string("Height");

    let mut above: Vec<occlusion::Rect> = Vec::new();
    let mut claude_rect: Option<occlusion::Rect> = None;

    for i in 0..list.len() {
        let Some(dict_ptr) = list.get(i) else { continue };
        let dict: &CFDictionary = &*dict_ptr;

        let owner_ptr = dict.find(owner_key.as_concrete_TypeRef() as *const _);
        let owner = owner_ptr
            .map(|p| unsafe { CFString::wrap_under_get_rule(*p as _) }.to_string())
            .unwrap_or_default();

        // Layer 0 = normal app windows. Skip tooltips, menu bar, etc.
        if let Some(layer_ptr) = dict.find(layer_key.as_concrete_TypeRef() as *const _) {
            let layer: CFNumber = unsafe { CFNumber::wrap_under_get_rule(*layer_ptr as _) };
            if layer.to_i32().unwrap_or(0) != 0 {
                continue;
            }
        }

        let bounds_ptr = dict.find(bounds_key.as_concrete_TypeRef() as *const _);
        let Some(bounds_ptr) = bounds_ptr else { continue };
        let bounds: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(*bounds_ptr as _) };
        let get = |k: &CFString| -> Option<f64> {
            let p = bounds.find(k.as_concrete_TypeRef() as *const _)?;
            let num: CFNumber = unsafe { CFNumber::wrap_under_get_rule(*p as _) };
            num.to_f64()
        };
        let (Some(x), Some(y), Some(w), Some(h)) =
            (get(&x_key), get(&y_key), get(&w_key), get(&h_key))
        else { continue };

        if owner == CLAUDE_OWNER_NAME {
            // First Claude window we hit (in z-order) is the candidate main
            // window — anything already in `above` is truly above it.
            if w >= 200.0 && h >= 200.0 {
                claude_rect = Some(occlusion::Rect::new(x, y, w, h));
                break;
            }
            continue;
        }

        // Skip Pane himself — he's obviously "above Claude" but he's not an
        // occluder from his own perspective.
        if owner == COMPANION_OWNER_NAME {
            continue;
        }

        above.push(occlusion::Rect::new(x, y, w, h));
    }

    claude_rect.map(|c| (c, above))
}

/// Primary screen's visible frame (excludes Dock + menu bar), in top-left
/// origin screen points. Returns (x, y, w, h).
#[cfg(target_os = "macos")]
fn visible_frame() -> Option<dock::ScreenRect> {
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;

    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let primary = screens.firstObject()?;
    let full = primary.frame();
    let visible = primary.visibleFrame();
    // Convert NSScreen's bottom-origin coordinates to top-left.
    let screen_height = full.size.height;
    let top_y = screen_height - (visible.origin.y + visible.size.height);
    Some(dock::ScreenRect {
        x: visible.origin.x,
        y: top_y,
        w: visible.size.width,
        h: visible.size.height,
    })
}

/// Read `defaults read com.apple.dock orientation` and parse it. On failure
/// (defaults not found, unexpected output) we default to Bottom, which is
/// the macOS factory default.
fn read_dock_orientation() -> dock::DockOrientation {
    use std::process::Command;
    let output = Command::new("defaults")
        .args(["read", "com.apple.dock", "orientation"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            dock::DockOrientation::parse(&s)
        }
        _ => dock::DockOrientation::Bottom,
    }
}

#[cfg(target_os = "macos")]
fn spawn_claude_watcher(app: tauri::AppHandle) {
    use std::time::Duration;
    std::thread::spawn(move || {
        // Per-mode state — persists across ticks so we only do one-time work
        // (initial positioning) once per mode entry.
        let mut has_positioned_initially_claude = false;
        let mut has_positioned_initially_desktop = false;
        let mut ticks_since_claude_seen: u32 = 0;
        let mut last_mode: String = String::new();

        loop {
            std::thread::sleep(Duration::from_millis(350));

            // Hard freeze while JS reports it's mid-interaction. Without this
            // the grace-period timer can fire mid-drag and yank the window
            // out from under the user's cursor.
            if INTERACTING.load(Ordering::Relaxed) {
                continue;
            }

            let Some(window) = app.get_webview_window("companion") else { continue; };

            // Read current mode fresh each tick so a user toggle in Settings
            // takes effect within one watcher interval. We snapshot into a
            // local so we're not holding the lock during the mode body.
            let mode_now: String = {
                let state = app.state::<ModeState>();
                state.0.lock().ok().map(|g| g.clone()).unwrap_or_else(|| "claudeOnly".into())
            };

            // On mode transition: clear per-mode positioning flags so the new
            // mode gets its own initial snap. Without this, switching from
            // desktop→claudeOnly would leave Pane stranded at the screen edge.
            if mode_now != last_mode {
                has_positioned_initially_claude = false;
                has_positioned_initially_desktop = false;
                last_mode = mode_now.clone();
            }

            match mode_now.as_str() {
                "desktop" => {
                    tick_desktop(&window, &mut has_positioned_initially_desktop);
                }
                _ => {
                    // claudeOnly — and anything unknown, for safety.
                    let quit = tick_claude_only(
                        &app,
                        &window,
                        &mut has_positioned_initially_claude,
                        &mut ticks_since_claude_seen,
                    );
                    if quit { break; }
                }
            }
        }
    });
}

/// One tick of the Claude-only mode state machine. Returns `true` when the
/// watcher should quit the app (Claude has been gone too long).
#[cfg(target_os = "macos")]
fn tick_claude_only(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    has_positioned_initially: &mut bool,
    ticks_since_claude_seen: &mut u32,
) -> bool {
    let margin_right = 56.0f64;
    let margin_bottom = 56.0f64;

    if !is_claude_running() {
        *ticks_since_claude_seen += 1;
        // ~14s grace period — Claude helpers briefly vanish during view
        // transitions. Quit only on sustained absence.
        if *ticks_since_claude_seen > 40 {
            app.exit(0);
            return true;
        }
        return false;
    }
    *ticks_since_claude_seen = 0;

    // First tick after entering this mode: snap Pane to Claude's corner
    // even if he's currently visible. Without this, switching in from
    // desktop mode leaves him stranded at the screen bottom until the next
    // hide/show cycle, which the user never triggers — the bug where
    // "flip off and on" was the only way to get him back.
    if !*has_positioned_initially {
        if let Some((x, y, w, h)) = find_claude_window() {
            let size = window.outer_size().ok();
            let scale = window.scale_factor().unwrap_or(1.0);
            let (comp_w, comp_h) = size
                .map(|s| (s.width as f64 / scale, s.height as f64 / scale))
                .unwrap_or((120.0, 160.0));
            let target_x = (x + w - comp_w - margin_right).round();
            let target_y = (y + h - comp_h - margin_bottom).round();
            let _ = window.set_position(LogicalPosition::new(target_x, target_y));
            *has_positioned_initially = true;
        }
    }

    let is_visible = window.is_visible().unwrap_or(false);

    // Reaction override: when a hook/MCP event just fired, the JS side asks
    // us to keep Pane visible for a few seconds regardless of occlusion.
    // Without this, an MCP `companion_say` fired while the user is in
    // another app would be invisible — Pane would be hidden by the
    // occlusion rule and the user would never see the speech bubble.
    let reaction_active = app
        .try_state::<ReactionOverride>()
        .and_then(|s| s.0.lock().ok().and_then(|g| *g))
        .map(|until| std::time::Instant::now() < until)
        .unwrap_or(false);

    // Occlusion: if any non-Claude, non-Pane window is z-above Claude and
    // overlaps Pane's bbox, hide him until the occluder moves away. The
    // check is the whole point of "Claude-only" mode — he belongs to Claude,
    // not to whatever the user Cmd-Tabbed into.
    let pane_rect = pane_window_rect(window);
    if let (Some(pane), Some((_claude, occluders))) =
        (pane_rect, find_claude_and_occluders_above())
    {
        let occluded = occlusion::is_occluded(pane, &occluders);
        if occluded && !reaction_active {
            if is_visible { let _ = window.hide(); }
            return false;
        }
    }

    if !is_visible {
        let _ = window.show();
    }
    false
}

/// One tick of the Desktop mode state machine. Unlike claudeOnly, this never
/// quits — desktop Pane lives independently of Claude.
#[cfg(target_os = "macos")]
fn tick_desktop(
    window: &tauri::WebviewWindow,
    has_positioned_initially: &mut bool,
) {
    // Snap to the Dock edge on first entry to this mode, regardless of
    // current visibility. Coming from claudeOnly mode, Pane's position is
    // wherever Claude's corner was — we need to move him to the screen
    // edge even if he's already showing.
    if !*has_positioned_initially {
        if let Some(vf) = visible_frame() {
            let orientation = read_dock_orientation();
            let scale = window.scale_factor().unwrap_or(1.0);
            let size = window.outer_size().ok();
            let (comp_w, comp_h) = size
                .map(|s| (s.width as f64 / scale, s.height as f64 / scale))
                .unwrap_or((120.0, 160.0));
            let pane = dock::PaneSize { w: comp_w, h: comp_h };
            let (x, y) = dock::initial_desktop_position(vf, orientation, pane);
            let _ = window.set_position(LogicalPosition::new(x, y));
            *has_positioned_initially = true;
        }
    }

    let is_visible = window.is_visible().unwrap_or(false);
    if !is_visible {
        let _ = window.show();
    }
}

/// Pane's current window rect in logical screen points, for occlusion math.
#[cfg(target_os = "macos")]
fn pane_window_rect(window: &tauri::WebviewWindow) -> Option<occlusion::Rect> {
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let scale = window.scale_factor().unwrap_or(1.0);
    Some(occlusion::Rect::new(
        pos.x as f64 / scale,
        pos.y as f64 / scale,
        size.width as f64 / scale,
        size.height as f64 / scale,
    ))
}

/// Reconcile the autostart plugin's on-disk LaunchAgent state with the user's
/// persisted `autostart` setting. Config is source of truth — if the two
/// disagree, we rewrite the LaunchAgent. Errors are logged and swallowed;
/// failing to register a LaunchAgent is not worth aborting launch over.
fn apply_autostart(app: &tauri::AppHandle, desired: bool) {
    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(false);
    if desired == current { return; }
    let res = if desired { manager.enable() } else { manager.disable() };
    if let Err(e) = res {
        eprintln!("[autostart] failed to {}: {e}", if desired { "enable" } else { "disable" });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    settings::migrate_legacy_config_if_needed();
    let initial_settings = settings::load_from(&settings::default_config_path());

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    // Fire on key-press, not key-release — two events per keypress
                    // otherwise, and we only want one.
                    if event.state() != ShortcutState::Pressed { return; }
                    let state = app.state::<SettingsState>();
                    let settings = state.0.lock().ok().map(|g| g.clone());
                    let Some(settings) = settings else { return; };
                    let Some(action) = hotkey_action_for(&settings, shortcut) else { return; };
                    handle_hotkey_action(app, action);
                })
                .build(),
        )
        .manage(SettingsState(Mutex::new(initial_settings.clone())))
        .manage(TrayState(Mutex::new(None)))
        .manage(ModeState(Mutex::new(initial_settings.mode.mode.clone())))
        .manage(ReactionOverride(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            cursor_over,
            pane_follow_cursor,
            pane_set_position,
            pane_ground,
            pane_set_interacting,
            pane_drag_start,
            settings_all,
            settings_save,
            settings_path,
            show_companion_menu,
            install_claude_hooks,
            uninstall_claude_hooks,
            mcp_config_json,
            install_mcp_config,
            uninstall_mcp_config,
            copy_to_clipboard,
            send_test_hook_event,
            request_reaction_window,
            memory_lines,
        ])
        .on_menu_event(|app, event| {
            // App-level menu handler catches events from BOTH the tray menu
            // and the right-click companion popup. Having one handler keeps
            // the dispatch logic in one place.
            let id = event.id().as_ref();
            match id {
                "show" | "hide" | "pet" | "settings" | "quit" => {
                    handle_tray_menu_event(app, id);
                }
                "ctx_settings" | "ctx_hide" | "ctx_quit" => {
                    handle_ctx_menu_event(app, id);
                }
                _ => {}
            }
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            set_accessory_activation_policy();

            // Reconcile autostart: config is source of truth. A user who
            // disabled autostart in settings on another machine, then
            // restored their config here, should not have a stale LaunchAgent.
            apply_autostart(app.handle(), initial_settings.autostart);

            // Initial hotkey registration based on settings at boot.
            let gs = app.global_shortcut();
            for (_action, acc) in initial_registrations(&initial_settings.hotkeys) {
                if let Some(s) = parse_shortcut(&acc) {
                    let _ = gs.register(s);
                }
            }

            // Tray menu lets the user show/hide/quit the companion without the
            // overlay window ever taking focus.
            let show_item = MenuItem::with_id(app, "show", "Show Companion", true, None::<&str>)?;
            let hide_item = MenuItem::with_id(app, "hide", "Hide Companion", true, None::<&str>)?;
            let pet_item = MenuItem::with_id(app, "pet", "Pet", true, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[&show_item, &hide_item, &pet_item, &settings_item, &quit_item],
            )?;

            // Use the bundle's default app icon for the tray. Without an
            // explicit icon, TrayIconBuilder produces a tray item with no
            // visible image — there's nothing in the menu bar to click,
            // which defeats the point of having a tray.
            let tray_builder = TrayIconBuilder::new();
            let tray_builder = if let Some(icon) = app.default_window_icon() {
                tray_builder.icon(icon.clone())
            } else {
                tray_builder
            };
            // The menu events are dispatched by the app-level on_menu_event.
            // Keeping the tray itself handler-less means one dispatch table,
            // no duplication between tray and popup menus.
            let tray = tray_builder.menu(&menu).build(app)?;
            // Initial visibility follows the persisted setting — no flicker if
            // the user had previously disabled the tray.
            let _ = tray.set_visible(initial_settings.tray.visible);
            if let Some(tray_state) = app.try_state::<TrayState>() {
                if let Ok(mut guard) = tray_state.0.lock() {
                    *guard = Some(tray);
                }
            }

            if let Some(win) = app.get_webview_window("companion") {
                #[cfg(target_os = "macos")]
                {
                    let _ = win.set_visible_on_all_workspaces(true);
                }
                // Always-on-top ensures Pane stays above Claude (and any
                // other window). The alternative — normal-level + pulse-to-
                // front on Claude-frontmost rising edges — is unreliable on
                // macOS: NSWindow can't be reordered relative to a window
                // owned by another process without AX APIs.
                let _ = win.set_always_on_top(true);
                // Start hidden — the Claude watcher decides when to show us.
                let _ = win.hide();
            }

            // The settings window should HIDE on close, not destroy itself.
            // Without this, clicking the red X or hitting ⌘W once tears down
            // the window — subsequent "Settings…" clicks from the tray or
            // right-click menu find no window to show and silently do
            // nothing.
            if let Some(settings_win) = app.get_webview_window("settings") {
                let hide_target = settings_win.clone();
                settings_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hide_target.hide();
                    }
                });
            }

            // Kick off the Claude Desktop watcher. It runs on a background
            // thread and drives show/hide, positioning, and quit-on-quit.
            #[cfg(target_os = "macos")]
            spawn_claude_watcher(app.handle().clone());

            // IPC server for Phase-5 integrations (hook bridge + MCP).
            // Enabled when the user has integration.ipc.enabled == true;
            // at boot we only spawn if already enabled — subsequent toggles
            // would require a restart for v1 (noted in the Integration tab).
            if initial_settings.integration.ipc.enabled {
                ipc::spawn_server(app.handle().clone(), initial_settings.integration.ipc.port);
            }

            // App-awareness watcher for Phase-6. The thread itself is always
            // running; internally it no-ops unless the user has opted in
            // AND is in desktop mode (re-checked every tick).
            app_watcher::spawn(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Companion");
}
