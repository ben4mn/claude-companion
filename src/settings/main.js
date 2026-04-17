// Settings window — shell.
//
// Phase 0 shows the tabbed chrome + config-file path only. Per-tab content
// lives in src/settings/tabs/*.js and gets wired in as later phases land:
//   Phase 1 → tabs/animation.js
//   Phase 2 → tabs/general.js (tray toggle + hotkey editors)
//   Phase 3 → tabs/mode.js
//   Phase 4 → tabs/companion.js
//   Phase 5 → tabs/integration.js
//   Phase 6 → tabs/apps.js

import { loadSettings, onSettingsChange } from '../engine/settings.js';
import { mount as mountAnimationTab } from './tabs/animation.js';
import { mount as mountAppsTab } from './tabs/apps.js';
import { mount as mountCompanionTab } from './tabs/companion.js';
import { mount as mountGeneralTab } from './tabs/general.js';
import { mount as mountIntegrationTab } from './tabs/integration.js';
import { mount as mountModeTab } from './tabs/mode.js';

// Per-tab mount registry. Keys must match the `data-tab` attributes in
// index.html. Values are async functions that receive the panel element
// and fill it in. Phase 0 left the panels as static placeholders; each
// subsequent phase lands its tab here.
const TAB_MOUNTS = {
  general: mountGeneralTab,
  animation: mountAnimationTab,
  mode: mountModeTab,
  companion: mountCompanionTab,
  integration: mountIntegrationTab,
  apps: mountAppsTab,
};

function setStatus(text, tone = '') {
  const el = document.getElementById('status');
  if (!el) return;
  el.textContent = text;
  el.className = `status ${tone}`.trim();
}

const mountedTabs = new Set();

async function mountTab(name) {
  if (mountedTabs.has(name)) return;
  const mount = TAB_MOUNTS[name];
  if (!mount) return;
  const panel = document.querySelector(`.panel[data-panel="${name}"]`);
  if (!panel) return;
  // Remove the placeholder copy before the tab fills itself in. We keep the
  // <h2> heading so every tab has a consistent title strip.
  [...panel.querySelectorAll('.muted, .config-path-block')].forEach((n) => n.remove());
  const slot = document.createElement('div');
  slot.className = 'tab-mount';
  panel.append(slot);
  try {
    await mount(slot);
    mountedTabs.add(name);
  } catch (e) {
    console.error(`[settings] mount(${name}) failed`, e);
  }
}

function setupTabs() {
  const tabs = document.querySelectorAll('.tab');
  const panels = document.querySelectorAll('.panel');
  tabs.forEach((tab) => {
    tab.addEventListener('click', () => {
      const name = tab.dataset.tab;
      tabs.forEach((t) => t.setAttribute('aria-selected', t === tab ? 'true' : 'false'));
      panels.forEach((p) => {
        p.setAttribute('aria-hidden', p.dataset.panel === name ? 'false' : 'true');
      });
      mountTab(name);
    });
  });
}

async function hydrate() {
  setStatus('Loading settings…');
  try {
    const settings = await loadSettings();
    // Path of the config file — helpful for debugging + transparency.
    const tauri = window.__TAURI__;
    if (tauri?.core?.invoke) {
      try {
        const path = await tauri.core.invoke('settings_path');
        const pathEl = document.getElementById('settings-path');
        if (pathEl && path) pathEl.textContent = path;
      } catch (e) { /* fine — path is informational only */ }
    }
    window.__companionSettings = settings;
    setStatus('Ready', 'saved');
  } catch (e) {
    console.error('[settings] hydrate failed', e);
    setStatus('Couldn\u2019t load settings', 'error');
  }
}

function subscribeToChanges() {
  onSettingsChange((next) => {
    window.__companionSettings = next;
    setStatus('Saved', 'saved');
    setTimeout(() => setStatus('Ready'), 1500);
  }).catch((e) => console.warn('[settings] change subscription failed', e));
}

async function init() {
  setupTabs();
  subscribeToChanges();
  await hydrate();
  // Eagerly mount the initially-visible tab so the user doesn't have to
  // click away and back to see its controls.
  await mountTab('general');
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
