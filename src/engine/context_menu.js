// Right-click menu wiring for the companion.
//
// When the user right-clicks the companion body, we suppress the browser's
// default context menu and ask the Rust side to pop up a native NSMenu (via
// the `show_companion_menu` IPC command). The menu items — Settings / Hide /
// Quit — are the same escape hatches the tray exposes, so the app stays
// usable even when the tray icon is disabled.
//
// Pulled out of behavior.js so the wiring is independently unit-testable
// (no need for a live Tauri runtime — we just assert `invoke` was called
// with the right command name).

export function attachCompanionContextMenu(el, invoke) {
  if (!el) return;
  el.addEventListener('contextmenu', (e) => {
    // Always prevent the browser's default menu: even if Tauri isn't ready
    // yet, a half-rendered default menu looks worse than nothing.
    e.preventDefault();
    if (typeof invoke !== 'function') return;
    try {
      const ret = invoke('show_companion_menu');
      // invoke returns a Promise — swallow rejections so a menu failure
      // doesn't surface as an unhandled promise rejection.
      if (ret && typeof ret.catch === 'function') ret.catch(() => {});
    } catch (_) {
      // Same principle — user right-clicked, we tried, we're done.
    }
  });
}
