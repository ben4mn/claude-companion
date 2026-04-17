// Apps tab — per-app commentary allowlist for desktop mode.
//
// The allowlist is the user's privacy control: Pane only comments on apps
// the user has explicitly approved. We present the built-in comment library
// as a checklist so it's trivial to opt in or out per-app; unknown apps
// (not in our library) can't be added from the UI — we can't usefully
// comment without a canned line anyway.

import { loadSettings, saveSettings } from '../../engine/settings.js';

let current = null;
let mountEl = null;
let commentsLibrary = null;

export async function mount(el) {
  mountEl = el;
  current = window.__companionSettings || (await loadSettings());
  if (!current.appAwareness) {
    current.appAwareness = { enabled: false, allowlist: [], frequencyMs: 45000 };
  }
  if (!commentsLibrary) {
    try {
      // Settings window is at src/settings/ — the data file is at src/data/.
      const res = await fetch('../data/app_comments.json');
      if (res.ok) commentsLibrary = await res.json();
    } catch (_) { commentsLibrary = {}; }
  }
  render();
}

function render() {
  mountEl.innerHTML = '';
  mountEl.append(renderEnableBlock(), renderAllowlistBlock(), renderFrequencyBlock());
}

function renderEnableBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'App awareness';
  block.append(h);

  const desc = document.createElement('p');
  desc.className = 'muted';
  desc.textContent =
    'In desktop mode, Pane occasionally comments on the app you just switched to. ' +
    'Opt-in per-app via the checklist below. Claude-only mode ignores this entirely.';
  block.append(desc);

  const row = document.createElement('label');
  row.className = 'checkbox-row';
  const checkbox = document.createElement('input');
  checkbox.type = 'checkbox';
  checkbox.checked = !!current.appAwareness.enabled;
  checkbox.addEventListener('change', async () => {
    current.appAwareness.enabled = checkbox.checked;
    try { await saveSettings(current); } catch (e) { console.warn('[apps] save', e); }
    render();
  });
  const label = document.createElement('span');
  label.textContent = 'Enable app-aware commentary';
  row.append(checkbox, label);
  block.append(row);

  if (current.mode?.mode !== 'desktop') {
    const banner = document.createElement('div');
    banner.className = 'banner';
    banner.innerHTML =
      '<strong>You\u2019re in Claude-only mode.</strong> ' +
      'App-aware commentary only fires in desktop mode \u2014 switch over in the Mode tab to try it.';
    block.append(banner);
  }

  return block;
}

function renderAllowlistBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Allowlist';
  block.append(h);

  if (!commentsLibrary || Object.keys(commentsLibrary).length === 0) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'No comment library loaded.';
    block.append(p);
    return block;
  }

  const desc = document.createElement('p');
  desc.className = 'muted hint';
  desc.textContent = 'Pane comments on these apps. Uncheck an app to keep him quiet about it.';
  block.append(desc);

  const enabled = !!current.appAwareness.enabled;
  const disabled = !enabled;

  const list = document.createElement('div');
  list.className = 'app-list';

  // Sort bundle IDs alphabetically by their human-friendly display.
  const entries = Object.entries(commentsLibrary)
    .map(([bundleId, comments]) => ({
      bundleId,
      name: displayNameFor(bundleId),
      sample: Array.isArray(comments) ? comments[0] : '',
    }))
    .sort((a, b) => a.name.localeCompare(b.name));

  for (const entry of entries) {
    const row = document.createElement('label');
    row.className = 'app-row';
    if (disabled) row.classList.add('disabled');

    const checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    checkbox.checked = current.appAwareness.allowlist.includes(entry.bundleId);
    checkbox.disabled = disabled;
    checkbox.addEventListener('change', async () => {
      const list = current.appAwareness.allowlist;
      const idx = list.indexOf(entry.bundleId);
      if (checkbox.checked && idx === -1) list.push(entry.bundleId);
      else if (!checkbox.checked && idx !== -1) list.splice(idx, 1);
      try { await saveSettings(current); } catch (e) { console.warn('[apps] save', e); }
    });

    const text = document.createElement('div');
    text.className = 'app-row-text';
    const name = document.createElement('div');
    name.className = 'app-row-name';
    name.textContent = entry.name;
    const sample = document.createElement('div');
    sample.className = 'muted app-row-sample';
    sample.textContent = entry.sample ? `\u201c${entry.sample}\u201d` : '';
    text.append(name, sample);

    row.append(checkbox, text);
    list.append(row);
  }
  block.append(list);

  const actions = document.createElement('div');
  actions.className = 'pack-actions';
  const allBtn = document.createElement('button');
  allBtn.type = 'button';
  allBtn.className = 'btn-secondary btn-quiet';
  allBtn.textContent = 'Select all';
  allBtn.disabled = disabled;
  allBtn.addEventListener('click', async () => {
    current.appAwareness.allowlist = Object.keys(commentsLibrary);
    try { await saveSettings(current); } catch (e) { console.warn('[apps] save', e); }
    render();
  });
  const noneBtn = document.createElement('button');
  noneBtn.type = 'button';
  noneBtn.className = 'btn-secondary btn-quiet';
  noneBtn.textContent = 'Select none';
  noneBtn.disabled = disabled;
  noneBtn.addEventListener('click', async () => {
    current.appAwareness.allowlist = [];
    try { await saveSettings(current); } catch (e) { console.warn('[apps] save', e); }
    render();
  });
  actions.append(allBtn, noneBtn);
  block.append(actions);

  return block;
}

function renderFrequencyBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Frequency';
  block.append(h);

  const row = document.createElement('div');
  row.className = 'field';
  const label = document.createElement('label');
  label.className = 'field-label';
  label.textContent = 'Min delay between comments';
  const input = document.createElement('input');
  input.type = 'number';
  input.min = '5000';
  input.max = '600000';
  input.step = '5000';
  input.value = current.appAwareness.frequencyMs;
  input.className = 'port-input';
  input.addEventListener('change', async () => {
    const ms = parseInt(input.value, 10);
    if (!Number.isFinite(ms) || ms < 5000) {
      input.value = current.appAwareness.frequencyMs;
      return;
    }
    current.appAwareness.frequencyMs = ms;
    try { await saveSettings(current); } catch (e) { console.warn('[apps] save', e); }
  });
  const suffix = document.createElement('span');
  suffix.className = 'muted';
  suffix.style.marginLeft = '8px';
  suffix.textContent = 'ms';
  const wrap = document.createElement('div');
  wrap.style.display = 'flex';
  wrap.style.alignItems = 'center';
  wrap.append(input, suffix);
  row.append(label, wrap);
  block.append(row);

  return block;
}

/** Convert a bundle ID into a human-readable app name. Falls back to the
 *  last dotted segment if we don't have an explicit mapping. */
function displayNameFor(bundleId) {
  const map = {
    'com.apple.Safari': 'Safari',
    'com.google.Chrome': 'Chrome',
    'org.mozilla.firefox': 'Firefox',
    'com.apple.dt.Xcode': 'Xcode',
    'com.microsoft.VSCode': 'VS Code',
    'com.todesktop.230313mzl4w4u92': 'Cursor',
    'com.apple.Terminal': 'Terminal',
    'com.googlecode.iterm2': 'iTerm',
    'com.microsoft.teams2': 'Microsoft Teams',
    'com.tinyspeck.slackmacgap': 'Slack',
    'com.hnc.Discord': 'Discord',
    'com.spotify.client': 'Spotify',
    'com.apple.Music': 'Music',
    'com.apple.finder': 'Finder',
    'com.apple.mail': 'Mail',
    'com.apple.Notes': 'Notes',
    'notion.id': 'Notion',
    'md.obsidian': 'Obsidian',
    'com.figma.Desktop': 'Figma',
    'com.github.GitHubDesktop': 'GitHub Desktop',
    'com.anthropic.claudefordesktop': 'Claude',
  };
  if (map[bundleId]) return map[bundleId];
  const parts = bundleId.split('.');
  return parts[parts.length - 1] || bundleId;
}
