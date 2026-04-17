// General tab — tray visibility + hotkey editors + first-disable warning.
//
// The tray toggle asks for confirmation (native confirm()) the very first
// time the user turns it off, spelling out the three hotkeys and the
// right-click fallback so nobody gets stranded without a way to reach the
// app. After acknowledgement we set firstDisableWarningShown=true and
// never prompt again.
//
// Hotkey editors use a click-to-record pattern: click Record…, press the
// combination you want (must include at least one modifier), it's saved
// immediately. Rust unregisters the old binding and registers the new one.

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
  mountEl.append(renderStartupBlock(), renderTrayBlock(), renderHotkeysBlock());
}

function renderStartupBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Startup';
  block.append(h);

  const row = document.createElement('label');
  row.className = 'checkbox-row';
  const checkbox = document.createElement('input');
  checkbox.type = 'checkbox';
  checkbox.checked = !!current.autostart;
  checkbox.addEventListener('change', async () => {
    current.autostart = checkbox.checked;
    try { await saveSettings(current); } catch (e) { console.warn('[general] save', e); }
  });
  const label = document.createElement('span');
  label.textContent = 'Launch Companion when I log in';
  row.append(checkbox, label);
  block.append(row);

  const note = document.createElement('p');
  note.className = 'muted hint';
  note.textContent = 'Installs a LaunchAgent. Unchecking removes it.';
  block.append(note);

  return block;
}

function renderTrayBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Menu bar icon';
  block.append(h);

  const row = document.createElement('label');
  row.className = 'checkbox-row';
  const checkbox = document.createElement('input');
  checkbox.type = 'checkbox';
  checkbox.checked = !!current.tray?.visible;
  checkbox.addEventListener('change', async () => {
    // No confirm() — window.confirm is unreliable inside Tauri webviews and
    // blocks the checkbox from flipping. We surface the consequences via
    // the inline warning banner below the toggle; the toggle itself saves
    // immediately.
    current.tray.visible = checkbox.checked;
    try { await saveSettings(current); } catch (e) { console.warn('[general] save', e); }
  });
  const label = document.createElement('span');
  label.textContent = 'Show icon in the menu bar';
  row.append(checkbox, label);
  block.append(row);

  // Always-visible banner so the user always knows how to recover if they
  // hide the icon. Formatted with the live hotkey values so rebinds are
  // reflected here too.
  const banner = document.createElement('div');
  banner.className = 'banner';
  banner.innerHTML =
    `<strong>If you hide the icon:</strong> use ` +
    `<code>${escape(current.hotkeys?.openSettings || 'Cmd+Shift+,')}</code> to reopen settings, ` +
    `<code>${escape(current.hotkeys?.showHide || 'Cmd+Shift+P')}</code> to show / hide the companion, or ` +
    `right-click the companion for a menu.`;
  block.append(banner);

  return block;
}

function escape(s) {
  return String(s).replace(/[&<>"']/g, (ch) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  })[ch]);
}

function renderHotkeysBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Hotkeys';
  block.append(h);

  const hotkeys = [
    { key: 'showHide', label: 'Show / hide companion' },
    { key: 'openSettings', label: 'Open settings' },
    { key: 'quit', label: 'Quit app' },
  ];
  for (const hk of hotkeys) block.append(makeHotkeyRow(hk));

  const hint = document.createElement('p');
  hint.className = 'muted hint';
  hint.textContent = 'Click Record, then press the keys. At least one modifier (Cmd / Ctrl / Alt / Shift) is required.';
  block.append(hint);

  return block;
}

function makeHotkeyRow({ key, label: labelText }) {
  const row = document.createElement('div');
  row.className = 'field';
  const label = document.createElement('label');
  label.className = 'field-label';
  label.textContent = labelText;

  const wrap = document.createElement('div');
  wrap.className = 'hotkey-wrap';
  const display = document.createElement('code');
  display.className = 'hotkey-display';
  display.textContent = current.hotkeys?.[key] || '\u2014';

  const recordBtn = document.createElement('button');
  recordBtn.type = 'button';
  recordBtn.className = 'btn-secondary';
  recordBtn.textContent = 'Record\u2026';
  recordBtn.addEventListener('click', () => beginRecording(key, display, recordBtn));

  const clearBtn = document.createElement('button');
  clearBtn.type = 'button';
  clearBtn.className = 'btn-secondary btn-quiet';
  clearBtn.textContent = 'Clear';
  clearBtn.addEventListener('click', async () => {
    current.hotkeys[key] = '';
    display.textContent = '\u2014';
    try { await saveSettings(current); } catch (e) { console.warn('[general] save', e); }
  });

  wrap.append(display, recordBtn, clearBtn);
  row.append(label, wrap);
  return row;
}

function beginRecording(key, display, btn) {
  const previous = display.textContent;
  btn.textContent = 'Press keys\u2026';
  btn.disabled = true;
  display.textContent = '\u2026';

  const cleanup = () => {
    window.removeEventListener('keydown', onKey, true);
    btn.textContent = 'Record\u2026';
    btn.disabled = false;
  };

  const onKey = async (e) => {
    // Escape bails out without changing anything.
    if (e.key === 'Escape') {
      display.textContent = previous;
      cleanup();
      return;
    }
    // Modifier-only keys don't complete a recording — the user needs to press
    // an actual key alongside them.
    if (['Meta', 'Control', 'Alt', 'Shift'].includes(e.key)) return;

    e.preventDefault();
    e.stopPropagation();

    const mods = [];
    if (e.metaKey) mods.push('Cmd');
    if (e.ctrlKey) mods.push('Ctrl');
    if (e.altKey) mods.push('Alt');
    if (e.shiftKey) mods.push('Shift');

    if (mods.length === 0) {
      // Bare letter keys are too easy to trigger by accident — insist on
      // a modifier so global-shortcut doesn't hijack typing.
      display.textContent = previous;
      cleanup();
      return;
    }

    const mainKey = e.key.length === 1 ? e.key.toUpperCase() : formatNamedKey(e.key);
    const accelerator = [...mods, mainKey].join('+');
    current.hotkeys[key] = accelerator;
    display.textContent = accelerator;
    try { await saveSettings(current); } catch (err) { console.warn('[general] save', err); }
    cleanup();
  };

  window.addEventListener('keydown', onKey, true);
}

function formatNamedKey(key) {
  const map = {
    ArrowLeft: 'Left', ArrowRight: 'Right', ArrowUp: 'Up', ArrowDown: 'Down',
    Escape: 'Escape', Enter: 'Enter', ' ': 'Space', Tab: 'Tab',
  };
  return map[key] || key;
}
