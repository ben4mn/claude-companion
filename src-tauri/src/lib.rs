use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    LogicalPosition, Manager,
};

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
/// "Claude Companion is a separate app" and "there's just a little overlay
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

/// Report Pane's "ground line" derived from Claude's window: the Y the
/// window's top-left should settle at, and the X range (min, max) Pane
/// can walk within. None when Claude has no visible window.
#[tauri::command]
fn pane_ground(window: tauri::WebviewWindow) -> Result<Option<(f64, f64, f64)>, String> {
    #[cfg(target_os = "macos")]
    {
        let Some((cx, cy, cw, ch)) = find_claude_window() else {
            return Ok(None);
        };
        let size = window.outer_size().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let comp_w = size.width as f64 / scale;
        let comp_h = size.height as f64 / scale;
        let ground_y = cy + ch - comp_h;
        let min_x = cx;
        let max_x = cx + cw - comp_w;
        return Ok(Some((ground_y, min_x, max_x)));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
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
const COMPANION_BUNDLE_ID: &str = "dev.ben4mn.claude-companion";

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

#[cfg(target_os = "macos")]
fn spawn_claude_watcher(app: tauri::AppHandle) {
    use std::time::Duration;
    std::thread::spawn(move || {
        // Margins from Claude's window edge. Tuned so Pane sits comfortably
        // in the bottom-right breathing room, not crammed into the corner.
        let margin_right = 56.0f64;
        let margin_bottom = 56.0f64;

        // Snap to bottom-right of Claude ONLY on the first show of this
        // process's lifetime. After that, JS owns position — any future
        // hide/show cycle leaves Pane wherever the user put him.
        let mut has_positioned_initially = false;
        let mut ticks_since_claude_seen: u32 = 0;

        loop {
            std::thread::sleep(Duration::from_millis(350));

            // Hard freeze while JS reports it's mid-interaction. Without this
            // the grace-period timer can fire mid-drag and yank the window
            // out from under the user's cursor.
            if INTERACTING.load(Ordering::Relaxed) {
                continue;
            }

            if !is_claude_running() {
                // Claude's helper processes can briefly fail the runningApplications
                // check during view transitions. Require a sustained absence
                // (~14s) before quitting so a view switch doesn't silently kill us.
                ticks_since_claude_seen += 1;
                if ticks_since_claude_seen > 40 {
                    app.exit(0);
                    break;
                }
                continue;
            }
            ticks_since_claude_seen = 0;

            let Some(window) = app.get_webview_window("companion") else {
                continue;
            };

            // Visibility rule: show Pane whenever Claude is RUNNING. We
            // explicitly do NOT tie visibility to find_claude_window —
            // it can transiently return None during view transitions, full-
            // screen handoffs, or when no Claude window matches our size
            // filter, which would make Pane flicker. Running == visible.
            let claude_window = find_claude_window();
            let is_visible = window.is_visible().unwrap_or(false);

            // Self-healing: drive visibility off the window's ACTUAL state.
            // If anything ever desyncs (e.g., a silently-failed show/hide),
            // the next tick corrects it.
            if !is_visible {
                if !has_positioned_initially {
                    if let Some((x, y, w, h)) = claude_window {
                        let size = window.outer_size().ok();
                        let scale = window.scale_factor().unwrap_or(1.0);
                        let (comp_w, comp_h) = size
                            .map(|s| (s.width as f64 / scale, s.height as f64 / scale))
                            .unwrap_or((120.0, 160.0));
                        let target_x = (x + w - comp_w - margin_right).round();
                        let target_y = (y + h - comp_h - margin_bottom).round();
                        let _ = window.set_position(LogicalPosition::new(target_x, target_y));
                        has_positioned_initially = true;
                    }
                }
                let _ = window.show();
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            cursor_over,
            pane_follow_cursor,
            pane_set_position,
            pane_ground,
            pane_set_interacting,
            pane_drag_start,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            set_accessory_activation_policy();

            // Tray menu lets the user show/hide/quit the companion without the
            // overlay window ever taking focus.
            let show_item = MenuItem::with_id(app, "show", "Show Companion", true, None::<&str>)?;
            let hide_item = MenuItem::with_id(app, "hide", "Hide Companion", true, None::<&str>)?;
            let pet_item = MenuItem::with_id(app, "pet", "Pet", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&show_item, &hide_item, &pet_item, &quit_item])?;

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
            let _tray = tray_builder
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("companion") {
                            // Reposition onto Claude's window before showing.
                            // If Pane's last position was outside Claude's
                            // current bounds, this guarantees he lands somewhere
                            // the user can actually see — the tray item is the
                            // user's escape hatch from any stuck state.
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
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

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

            // Kick off the Claude Desktop watcher. It runs on a background
            // thread and drives show/hide, positioning, and quit-on-quit.
            #[cfg(target_os = "macos")]
            spawn_claude_watcher(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Companion");
}
