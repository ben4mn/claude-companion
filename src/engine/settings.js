// Settings wrapper — JS side.
//
// Reads window.__TAURI__ fresh on every call rather than caching module-level
// handles, so tests can swap the Tauri mock between cases without reloading
// the module and the behavior engine can call these safely before Tauri is
// fully initialized (returns defaults in that case).
//
// Contract with the Rust side (src-tauri/src/settings.rs):
//   - `settings_all()`             → full Settings object (camelCase JSON)
//   - `settings_save({ settings })` → persists, emits `settings_changed`
//   - emitted event `settings_changed` carries the full new Settings
//
// Keep DEFAULTS mirrored with the Rust Default impls — the
// `defaults_are_stable_and_safe` test on the Rust side pins the privacy-
// sensitive booleans; the matching JS test pins the camelCase shape here.

export const DEFAULTS = Object.freeze({
  tray: { visible: true, firstDisableWarningShown: false },
  animation: {
    preset: 'normal',
    activityFrequency: 1.0,
    walkSpeed: 1.0,
    speechChattiness: 0.5,
    quietHours: { enabled: false, from: '22:00', to: '07:00' },
    activityPool: null,
  },
  mode: { mode: 'claudeOnly' },
  companion: { activePack: 'pane', themes: {} },
  integration: {
    ipc: { enabled: false, port: 48372 },
    hooks: { installed: false },
    mcp: { enabled: false },
    memory: { enabled: false, paths: ['~/.claude/projects'] },
  },
  appAwareness: { enabled: false, allowlist: [], frequencyMs: 45000 },
  hotkeys: {
    showHide: 'Cmd+Shift+P',
    openSettings: 'Cmd+Shift+,',
    quit: 'Cmd+Shift+Q',
  },
});

function tauri() {
  return typeof window !== 'undefined' ? window.__TAURI__ : null;
}

export async function loadSettings() {
  const t = tauri();
  if (!t?.core?.invoke) return structuredClone(DEFAULTS);
  try {
    const got = await t.core.invoke('settings_all');
    // The setup.js fallback returns undefined for unknown commands; tests
    // rely on that triggering the defaults path.
    if (got === undefined) return structuredClone(DEFAULTS);
    return got;
  } catch (e) {
    console.warn('[settings] loadSettings failed, falling back to defaults:', e);
    return structuredClone(DEFAULTS);
  }
}

export async function saveSettings(settings) {
  const t = tauri();
  if (!t?.core?.invoke) throw new Error('Tauri not available');
  return await t.core.invoke('settings_save', { settings });
}

export async function onSettingsChange(callback) {
  const t = tauri();
  if (!t?.event?.listen) return () => {};
  const unlisten = await t.event.listen('settings_changed', (evt) => {
    callback(evt?.payload ?? evt);
  });
  return typeof unlisten === 'function' ? unlisten : () => {};
}
