import { describe, it, expect, beforeEach } from 'vitest';
import { installTauriMock } from '../../test/tauri-mock.js';
import { loadSettings, saveSettings, onSettingsChange, DEFAULTS } from './settings.js';

describe('src/engine/settings.js', () => {
  beforeEach(() => {
    // Reset Tauri global between tests so stale handlers don't leak. The
    // settings module reads window.__TAURI__ on every call (no module-level
    // caching) so a plain reset is enough.
    delete window.__TAURI__;
  });

  it('loadSettings calls settings_all and returns the payload', async () => {
    const fake = { tray: { visible: false }, mode: { mode: 'desktop' } };
    const mock = installTauriMock({ settings_all: async () => fake });
    const out = await loadSettings();
    expect(out).toEqual(fake);
    expect(mock.calls.settings_all).toEqual([undefined]);
  });

  it('saveSettings calls settings_save with the full payload under `settings` key', async () => {
    const mock = installTauriMock({ settings_save: async () => undefined });
    const next = { tray: { visible: false } };
    await saveSettings(next);
    expect(mock.calls.settings_save).toEqual([{ settings: next }]);
  });

  it('onSettingsChange subscribes to settings_changed and fires callback with payload', async () => {
    const mock = installTauriMock();
    const seen = [];
    const unsub = await onSettingsChange((s) => seen.push(s));
    mock.fire('settings_changed', { mode: { mode: 'desktop' } });
    expect(seen).toEqual([{ mode: { mode: 'desktop' } }]);
    if (typeof unsub === 'function') unsub();
    mock.fire('settings_changed', { mode: { mode: 'claudeOnly' } });
    expect(seen.length).toBe(1);
  });

  it('loadSettings falls back to defaults when Tauri invoke returns undefined', async () => {
    // Minimal setup.js fallback: invoke returns undefined. loadSettings
    // should recognize that and return DEFAULTS so the UI can still render.
    const out = await loadSettings();
    expect(out).toBeDefined();
    expect(out.tray.visible).toBe(true);
    expect(out.mode.mode).toBe('claudeOnly');
  });

  it('DEFAULTS constant matches the Rust defaults contract', () => {
    expect(DEFAULTS.tray.visible).toBe(true);
    expect(DEFAULTS.mode.mode).toBe('claudeOnly');
    expect(DEFAULTS.companion.activePack).toBe('pane');
    expect(DEFAULTS.animation.preset).toBe('normal');
    expect(DEFAULTS.integration.hooks.installed).toBe(false);
    expect(DEFAULTS.integration.mcp.enabled).toBe(false);
    expect(DEFAULTS.appAwareness.enabled).toBe(false);
  });
});
