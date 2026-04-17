// Companion tab — gallery + per-part color pickers.
//
// Layout:
//   - Gallery: one card per known pack. Selecting a card sets activePack
//     and saves immediately; the companion window reloads the pack live.
//   - Active pack details: tagline + a color picker per declared part.
//     Changes are debounced-persisted under companion.themes[<id>][<key>].
//   - "Reset to defaults" button per pack clears the overrides for that
//     pack and lets manifest defaults take over.
//
// The manifest for each pack is fetched once and cached to avoid hammering
// the file system on every render.

import { loadSettings, saveSettings } from '../../engine/settings.js';
import { PACK_IDS } from '../../engine/pack_loader.js';

let current = null;
let mountEl = null;
const manifestCache = new Map(); // packId → manifest

async function fetchManifest(packId) {
  if (manifestCache.has(packId)) return manifestCache.get(packId);
  try {
    const res = await fetch(`../packs/${packId}/manifest.json`);
    if (!res.ok) return null;
    const m = await res.json();
    manifestCache.set(packId, m);
    return m;
  } catch (_) {
    return null;
  }
}

export async function mount(el) {
  mountEl = el;
  current = window.__companionSettings || (await loadSettings());
  if (!current.companion) current.companion = { activePack: 'pane', themes: {} };
  if (!current.companion.themes) current.companion.themes = {};
  await render();
}

async function render() {
  mountEl.innerHTML = '';
  mountEl.append(await renderGallery(), await renderActivePackDetails());
}

async function renderGallery() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Choose a companion';
  block.append(h);

  const grid = document.createElement('div');
  grid.className = 'pack-gallery';

  const manifests = await Promise.all(PACK_IDS.map(fetchManifest));
  PACK_IDS.forEach((id, i) => {
    const manifest = manifests[i];
    const card = document.createElement('button');
    card.type = 'button';
    card.className = 'pack-card';
    if ((current.companion.activePack ?? 'pane') === id) card.classList.add('selected');

    // Inline preview swatch: show the pack's default body color as the card
    // background tint so the gallery reads at a glance.
    const tint = manifest?.parts?.find((p) => p.key === 'body')?.default ?? '#666';
    const swatch = document.createElement('div');
    swatch.className = 'pack-card-swatch';
    swatch.style.background = tint;
    card.append(swatch);

    const name = document.createElement('div');
    name.className = 'pack-card-name';
    name.textContent = manifest?.name ?? id;
    card.append(name);

    if (manifest?.tagline) {
      const tag = document.createElement('div');
      tag.className = 'pack-card-tag muted';
      tag.textContent = manifest.tagline;
      card.append(tag);
    }

    card.addEventListener('click', async () => {
      current.companion.activePack = id;
      try { await saveSettings(current); } catch (e) { console.warn('[companion] save', e); }
      await render();
    });
    grid.append(card);
  });

  block.append(grid);
  return block;
}

async function renderActivePackDetails() {
  const block = document.createElement('div');
  block.className = 'block';
  const activeId = current.companion.activePack ?? 'pane';
  const manifest = await fetchManifest(activeId);

  const h = document.createElement('h3');
  h.textContent = `Customize ${manifest?.name ?? activeId}`;
  block.append(h);

  if (!manifest) {
    const warn = document.createElement('p');
    warn.className = 'muted';
    warn.textContent = 'Could not load this companion\u2019s details.';
    block.append(warn);
    return block;
  }

  if (!manifest.parts || manifest.parts.length === 0) {
    const warn = document.createElement('p');
    warn.className = 'muted';
    warn.textContent = 'No customizable parts declared.';
    block.append(warn);
    return block;
  }

  const theme = current.companion.themes[activeId] ?? {};
  for (const part of manifest.parts) {
    block.append(renderPartRow(activeId, part, theme));
  }

  const actions = document.createElement('div');
  actions.className = 'pack-actions';
  const resetBtn = document.createElement('button');
  resetBtn.type = 'button';
  resetBtn.className = 'btn-secondary btn-quiet';
  resetBtn.textContent = 'Reset to defaults';
  resetBtn.addEventListener('click', async () => {
    delete current.companion.themes[activeId];
    try { await saveSettings(current); } catch (e) { console.warn('[companion] save', e); }
    await render();
  });
  actions.append(resetBtn);
  block.append(actions);

  return block;
}

function renderPartRow(packId, part, theme) {
  const row = document.createElement('div');
  row.className = 'field';

  const label = document.createElement('label');
  label.className = 'field-label';
  label.textContent = part.label;

  const wrap = document.createElement('div');
  wrap.className = 'color-wrap';

  const picker = document.createElement('input');
  picker.type = 'color';
  picker.value = theme[part.key] ?? part.default;
  picker.addEventListener('input', () => {
    if (!current.companion.themes[packId]) current.companion.themes[packId] = {};
    current.companion.themes[packId][part.key] = picker.value;
    readout.textContent = picker.value;
    persist();
  });

  const readout = document.createElement('code');
  readout.className = 'color-readout';
  readout.textContent = picker.value;

  const resetBtn = document.createElement('button');
  resetBtn.type = 'button';
  resetBtn.className = 'btn-secondary btn-quiet';
  resetBtn.textContent = 'Default';
  resetBtn.title = `Reset ${part.label} to ${part.default}`;
  resetBtn.addEventListener('click', async () => {
    if (current.companion.themes[packId]) {
      delete current.companion.themes[packId][part.key];
      if (Object.keys(current.companion.themes[packId]).length === 0) {
        delete current.companion.themes[packId];
      }
    }
    picker.value = part.default;
    readout.textContent = part.default;
    try { await saveSettings(current); } catch (e) { console.warn('[companion] save', e); }
  });

  wrap.append(picker, readout, resetBtn);
  row.append(label, wrap);
  return row;
}

let saveTimer = null;
function persist() {
  // Debounce rapid picker input — users drag the color wheel across dozens
  // of values before letting go. 200ms keeps the behavior engine in sync
  // without hammering disk.
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveSettings(current).catch((e) => console.warn('[companion] save', e));
  }, 200);
}
