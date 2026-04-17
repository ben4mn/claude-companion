// Animation tab — binds UI controls to settings.animation and saves on change.
//
// Layout:
//   Preset radio (calm / normal / playful / custom)
//   Advanced drawer (only visible when preset === "custom"):
//     - activityFrequency slider
//     - walkSpeed slider
//     - speechChattiness slider
//   Quiet hours (toggle + from/to — always visible, independent of preset)
//
// Saving strategy: every interactive control calls saveSettings(fullSettings)
// with the current in-memory blob. The Rust side emits settings_changed,
// which the behavior engine listens for — so the companion's behavior
// updates live, without reopening the settings window.

import { loadSettings, saveSettings } from '../../engine/settings.js';
import { PRESETS } from '../../engine/activity-config.js';

let current = null;
let mountEl = null;

function field(labelText, control) {
  const row = document.createElement('div');
  row.className = 'field';
  const label = document.createElement('label');
  label.className = 'field-label';
  label.textContent = labelText;
  row.append(label, control);
  return row;
}

function slider({ id, min, max, step, value, suffix, onChange }) {
  const wrap = document.createElement('div');
  wrap.className = 'slider-wrap';
  const input = document.createElement('input');
  input.type = 'range';
  input.id = id;
  input.min = min;
  input.max = max;
  input.step = step;
  input.value = value;
  const readout = document.createElement('span');
  readout.className = 'slider-readout';
  readout.textContent = `${Number(value).toFixed(2)}${suffix ?? ''}`;
  input.addEventListener('input', () => {
    readout.textContent = `${Number(input.value).toFixed(2)}${suffix ?? ''}`;
    onChange(Number(input.value));
  });
  wrap.append(input, readout);
  return wrap;
}

function presetRadio(current, onChange) {
  const wrap = document.createElement('div');
  wrap.className = 'radio-group';
  const options = ['calm', 'normal', 'playful', 'custom'];
  for (const name of options) {
    const id = `preset-${name}`;
    const label = document.createElement('label');
    label.className = 'radio-option';
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = 'preset';
    input.id = id;
    input.value = name;
    input.checked = current === name;
    input.addEventListener('change', () => {
      if (input.checked) onChange(name);
    });
    const span = document.createElement('span');
    span.textContent = name[0].toUpperCase() + name.slice(1);
    label.append(input, span);
    wrap.append(label);
  }
  return wrap;
}

function render() {
  mountEl.innerHTML = '';

  const anim = current.animation;

  // Preset radio
  const presetBlock = document.createElement('div');
  presetBlock.className = 'block';
  const presetLabel = document.createElement('div');
  presetLabel.className = 'field-label';
  presetLabel.textContent = 'Preset';
  presetBlock.append(
    presetLabel,
    presetRadio(anim.preset, (name) => {
      anim.preset = name;
      if (name !== 'custom') {
        const p = PRESETS[name];
        if (p) {
          anim.activityFrequency = p.activityFrequency;
          anim.walkSpeed = p.walkSpeed;
          anim.speechChattiness = p.speechChattiness;
          anim.activityPool = p.activityPool;
        }
      }
      persistAndRerender();
    }),
  );
  mountEl.append(presetBlock);

  // Advanced drawer — only for custom preset.
  if (anim.preset === 'custom') {
    const adv = document.createElement('div');
    adv.className = 'block advanced-drawer';
    const h = document.createElement('h3');
    h.textContent = 'Advanced';
    adv.append(h);

    adv.append(field('Activity frequency', slider({
      id: 'activityFrequency',
      min: 0.3, max: 3.0, step: 0.1,
      value: anim.activityFrequency,
      suffix: '\u00d7',
      onChange: (v) => { anim.activityFrequency = v; persist(); },
    })));

    adv.append(field('Walk speed', slider({
      id: 'walkSpeed',
      // Narrower than activityFrequency on purpose: Pane's walk duration
      // is clamped at a 600ms minimum, so values above ~1.3 stop feeling
      // noticeably faster. Below 0.4 he'd crawl at an unreadable pace.
      min: 0.4, max: 1.3, step: 0.05,
      value: anim.walkSpeed,
      suffix: '\u00d7',
      onChange: (v) => { anim.walkSpeed = v; persist(); },
    })));

    adv.append(field('Speech chattiness', slider({
      id: 'speechChattiness',
      min: 0, max: 1, step: 0.05,
      value: anim.speechChattiness,
      onChange: (v) => { anim.speechChattiness = v; persist(); },
    })));

    mountEl.append(adv);
  }

  // Quiet hours — always visible, works with any preset.
  const qhBlock = document.createElement('div');
  qhBlock.className = 'block';
  const qhTitle = document.createElement('h3');
  qhTitle.textContent = 'Quiet hours';
  qhBlock.append(qhTitle);

  const qh = anim.quietHours ?? { enabled: false, from: '22:00', to: '07:00' };
  const qhEnabled = document.createElement('label');
  qhEnabled.className = 'checkbox-row';
  const qhCheckbox = document.createElement('input');
  qhCheckbox.type = 'checkbox';
  qhCheckbox.checked = !!qh.enabled;
  qhCheckbox.addEventListener('change', () => {
    anim.quietHours = { ...qh, enabled: qhCheckbox.checked };
    persistAndRerender();
  });
  const qhText = document.createElement('span');
  qhText.textContent = 'Restrict to calm activities during these hours';
  qhEnabled.append(qhCheckbox, qhText);
  qhBlock.append(qhEnabled);

  if (qh.enabled) {
    const timeRow = document.createElement('div');
    timeRow.className = 'time-row';
    const from = document.createElement('input');
    from.type = 'time';
    from.value = qh.from;
    from.addEventListener('change', () => {
      anim.quietHours = { ...qh, from: from.value };
      persist();
    });
    const to = document.createElement('input');
    to.type = 'time';
    to.value = qh.to;
    to.addEventListener('change', () => {
      anim.quietHours = { ...qh, to: to.value };
      persist();
    });
    const sep = document.createElement('span');
    sep.className = 'muted';
    sep.textContent = 'to';
    timeRow.append(from, sep, to);
    qhBlock.append(timeRow);
  }

  mountEl.append(qhBlock);
}

let saveTimer = null;
function persist() {
  // Debounce rapid slider input so we're not hammering disk I/O. 250ms is
  // fast enough to feel instant when the user lets go.
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveSettings(current).catch((e) => console.warn('[animation] save failed', e));
  }, 250);
}

function persistAndRerender() {
  render();
  persist();
}

export async function mount(el) {
  mountEl = el;
  current = window.__companionSettings || (await loadSettings());
  render();
}
