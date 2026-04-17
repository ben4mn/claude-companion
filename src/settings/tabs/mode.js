// Mode tab — follow Claude window vs desktop-wide.
//
// The radio toggle is live: picking a mode saves immediately, the Rust
// watcher reads the new mode on its next ~350ms tick, and Pane transitions
// without restarting the app. Per-mode explainer copy below each option
// keeps the trade-offs visible without making the user dig through docs.

import { loadSettings, saveSettings } from '../../engine/settings.js';

let current = null;
let mountEl = null;

export async function mount(el) {
  mountEl = el;
  current = window.__companionSettings || (await loadSettings());
  render();
}

function render() {
  mountEl.innerHTML = '';

  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Where should the companion live?';
  block.append(h);

  const options = [
    {
      value: 'claudeOnly',
      label: 'In Claude Code',
      body: 'Walks along the bottom of the Claude window. Hides automatically when another app covers Claude. Quits with Claude.',
    },
    {
      value: 'desktop',
      label: 'On the desktop',
      body: 'Walks along the bottom of your screen, independent of Claude. Stays put even when you switch apps.',
    },
  ];

  for (const opt of options) {
    const card = document.createElement('label');
    card.className = 'mode-card';
    if ((current.mode?.mode ?? 'claudeOnly') === opt.value) card.classList.add('selected');
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = 'mode';
    input.value = opt.value;
    input.checked = (current.mode?.mode ?? 'claudeOnly') === opt.value;
    input.addEventListener('change', async () => {
      if (!input.checked) return;
      current.mode = { ...current.mode, mode: opt.value };
      try { await saveSettings(current); } catch (e) { console.warn('[mode] save', e); }
      render();
    });
    const textWrap = document.createElement('div');
    textWrap.className = 'mode-card-text';
    const title = document.createElement('div');
    title.className = 'mode-card-title';
    title.textContent = opt.label;
    const body = document.createElement('div');
    body.className = 'mode-card-body muted';
    body.textContent = opt.body;
    textWrap.append(title, body);
    card.append(input, textWrap);
    block.append(card);
  }

  mountEl.append(block);

  const caveats = document.createElement('div');
  caveats.className = 'banner';
  caveats.innerHTML =
    '<strong>Heads up:</strong> Desktop mode is an MVP — bottom Dock is fully supported; ' +
    'side-Dock setups leave the companion standing at the Dock edge without walking. ' +
    'Dock auto-hide follow-along and walk-back-after-drop are coming in a later pass.';
  mountEl.append(caveats);
}
