// Integration tab — IPC server, Claude Code hooks, MCP server, memory reader.
//
// Each layer has its own block with an enable toggle + one-button action
// (install/copy). The IPC server is the foundation — hooks and MCP forward
// events through it, so we guide the user to enable IPC first.

import { loadSettings, saveSettings } from '../../engine/settings.js';

let current = null;
let mountEl = null;

export async function mount(el) {
  mountEl = el;
  current = window.__companionSettings || (await loadSettings());
  if (!current.integration) {
    current.integration = {
      ipc: { enabled: false, port: 48372 },
      hooks: { installed: false },
      mcp: { enabled: false },
      memory: { enabled: false, paths: ['~/.claude/projects'] },
    };
  }
  render();
}

function render() {
  mountEl.innerHTML = '';
  mountEl.append(
    renderIpcBlock(),
    renderHooksBlock(),
    renderMcpBlock(),
    renderMemoryBlock(),
    renderRestartBanner(),
  );
}

function renderIpcBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Local IPC server';
  block.append(h);

  const desc = document.createElement('p');
  desc.className = 'muted';
  desc.textContent =
    'Runs an HTTP server on 127.0.0.1 that the hook CLI and MCP server talk to. ' +
    'Required for hooks and MCP to work. Loopback only — never exposed to the network.';
  block.append(desc);

  const row = document.createElement('label');
  row.className = 'checkbox-row';
  const checkbox = document.createElement('input');
  checkbox.type = 'checkbox';
  checkbox.checked = !!current.integration.ipc.enabled;
  checkbox.addEventListener('change', async () => {
    current.integration.ipc.enabled = checkbox.checked;
    try { await saveSettings(current); } catch (e) { console.warn('[integration] save', e); }
  });
  const label = document.createElement('span');
  label.textContent = 'Run the IPC server at startup';
  row.append(checkbox, label);
  block.append(row);

  const portRow = document.createElement('div');
  portRow.className = 'field';
  const portLabel = document.createElement('label');
  portLabel.className = 'field-label';
  portLabel.textContent = 'Port';
  const portInput = document.createElement('input');
  portInput.type = 'number';
  portInput.min = '1024';
  portInput.max = '65535';
  portInput.value = current.integration.ipc.port;
  portInput.className = 'port-input';
  portInput.addEventListener('change', async () => {
    const p = parseInt(portInput.value, 10);
    if (!Number.isFinite(p) || p < 1024 || p > 65535) {
      portInput.value = current.integration.ipc.port;
      return;
    }
    current.integration.ipc.port = p;
    try { await saveSettings(current); } catch (e) { console.warn('[integration] save', e); }
  });
  portRow.append(portLabel, portInput);
  block.append(portRow);

  return block;
}

function renderHooksBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Claude Code hooks';
  block.append(h);

  const desc = document.createElement('p');
  desc.className = 'muted';
  desc.textContent =
    'Writes a hooks block to ~/.claude/settings.json wiring Claude Code events ' +
    '(PreToolUse, PostToolUse, Notification, Stop) to the companion. Pane animates as Claude works.';
  block.append(desc);

  const status = document.createElement('div');
  status.className = 'hotkey-display';
  status.textContent = current.integration.hooks.installed ? 'Installed' : 'Not installed';
  status.style.display = 'inline-block';
  status.style.marginBottom = '10px';

  const actions = document.createElement('div');
  actions.className = 'hotkey-wrap';
  actions.append(status);

  const installBtn = document.createElement('button');
  installBtn.type = 'button';
  installBtn.className = 'btn-secondary';
  installBtn.textContent = current.integration.hooks.installed ? 'Reinstall' : 'Install hooks';
  installBtn.addEventListener('click', async () => {
    const tauri = window.__TAURI__;
    if (!tauri?.core?.invoke) return;
    installBtn.disabled = true;
    try {
      const path = await tauri.core.invoke('install_claude_hooks');
      current.integration.hooks.installed = true;
      await saveSettings(current);
      status.textContent = `Installed \u2192 ${path}`;
      installBtn.textContent = 'Reinstall';
    } catch (e) {
      status.textContent = `Failed: ${e}`;
    } finally {
      installBtn.disabled = false;
    }
  });
  actions.append(installBtn);

  if (current.integration.hooks.installed) {
    const uninstallBtn = document.createElement('button');
    uninstallBtn.type = 'button';
    uninstallBtn.className = 'btn-secondary btn-quiet';
    uninstallBtn.textContent = 'Uninstall';
    uninstallBtn.addEventListener('click', async () => {
      const tauri = window.__TAURI__;
      if (!tauri?.core?.invoke) return;
      uninstallBtn.disabled = true;
      try {
        await tauri.core.invoke('uninstall_claude_hooks');
        current.integration.hooks.installed = false;
        await saveSettings(current);
        render();
      } catch (e) {
        status.textContent = `Failed: ${e}`;
      } finally {
        uninstallBtn.disabled = false;
      }
    });
    actions.append(uninstallBtn);
  }

  block.append(actions);

  // Diagnostic buttons — fire a fake event directly to the reaction pipeline,
  // skipping Claude Code + IPC entirely. If Pane animates here, the JS side
  // is wired; if real hooks don't work, the problem is upstream (Claude
  // Code settings, IPC server, bridge binary path).
  const testRow = document.createElement('div');
  testRow.className = 'hotkey-wrap';
  testRow.style.marginTop = '8px';
  const testLabel = document.createElement('span');
  testLabel.className = 'muted';
  testLabel.style.fontSize = '11px';
  testLabel.style.marginRight = '4px';
  testLabel.textContent = 'Test:';
  testRow.append(testLabel);
  for (const kind of ['PreToolUse', 'PostToolUse', 'Notification', 'Stop']) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn-secondary btn-quiet';
    btn.textContent = kind;
    btn.addEventListener('click', async () => {
      const tauri = window.__TAURI__;
      if (!tauri?.core?.invoke) return;
      try { await tauri.core.invoke('send_test_hook_event', { kind }); }
      catch (e) { console.warn('[integration] test event', e); }
    });
    testRow.append(btn);
  }
  block.append(testRow);

  const hint = document.createElement('p');
  hint.className = 'muted hint';
  hint.textContent = 'Restart Claude Code after installing for real hooks. Test buttons fire the reaction pipeline directly (no Claude Code needed).';
  block.append(hint);

  return block;
}

function renderMcpBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'MCP server';
  block.append(h);

  const desc = document.createElement('p');
  desc.className = 'muted';
  desc.textContent =
    'Lets Claude (the model) speak through Pane deliberately via three tools: ' +
    'companion_say, companion_react, companion_show_status.';
  block.append(desc);

  const status = document.createElement('div');
  status.className = 'hotkey-display';
  status.textContent = current.integration.mcp.enabled ? 'Installed' : 'Not installed';
  status.style.display = 'inline-block';
  status.style.marginBottom = '10px';

  const actions = document.createElement('div');
  actions.className = 'hotkey-wrap';
  actions.append(status);

  const installBtn = document.createElement('button');
  installBtn.type = 'button';
  installBtn.className = 'btn-secondary';
  installBtn.textContent = current.integration.mcp.enabled ? 'Reinstall' : 'Install MCP';
  installBtn.addEventListener('click', async () => {
    const tauri = window.__TAURI__;
    if (!tauri?.core?.invoke) return;
    installBtn.disabled = true;
    try {
      const path = await tauri.core.invoke('install_mcp_config');
      current.integration.mcp.enabled = true;
      await saveSettings(current);
      status.textContent = `Installed \u2192 ${path}`;
      installBtn.textContent = 'Reinstall';
      render();
    } catch (e) {
      status.textContent = `Failed: ${e}`;
    } finally {
      installBtn.disabled = false;
    }
  });
  actions.append(installBtn);

  if (current.integration.mcp.enabled) {
    const uninstallBtn = document.createElement('button');
    uninstallBtn.type = 'button';
    uninstallBtn.className = 'btn-secondary btn-quiet';
    uninstallBtn.textContent = 'Uninstall';
    uninstallBtn.addEventListener('click', async () => {
      const tauri = window.__TAURI__;
      if (!tauri?.core?.invoke) return;
      uninstallBtn.disabled = true;
      try {
        await tauri.core.invoke('uninstall_mcp_config');
        current.integration.mcp.enabled = false;
        await saveSettings(current);
        render();
      } catch (e) {
        status.textContent = `Failed: ${e}`;
      } finally {
        uninstallBtn.disabled = false;
      }
    });
    actions.append(uninstallBtn);
  }

  // Copy config as a fallback for users who want to drop it into a
  // different location (project .mcp.json, custom config, etc).
  const copyBtn = document.createElement('button');
  copyBtn.type = 'button';
  copyBtn.className = 'btn-secondary btn-quiet';
  copyBtn.textContent = 'Copy config';
  copyBtn.addEventListener('click', async () => {
    const tauri = window.__TAURI__;
    if (!tauri?.core?.invoke) return;
    try {
      const snippet = await tauri.core.invoke('mcp_config_json');
      // Go through Rust — navigator.clipboard is unreliable in Tauri webviews.
      await tauri.core.invoke('copy_to_clipboard', { text: snippet });
      copyBtn.textContent = 'Copied!';
      setTimeout(() => (copyBtn.textContent = 'Copy config'), 1800);
    } catch (e) {
      copyBtn.textContent = `Failed`;
      console.warn('[integration] copy', e);
      setTimeout(() => (copyBtn.textContent = 'Copy config'), 2500);
    }
  });
  actions.append(copyBtn);
  block.append(actions);

  const hint = document.createElement('p');
  hint.className = 'muted hint';
  hint.textContent = 'Install writes to ~/.claude.json. Restart Claude Code for the MCP to become available.';
  block.append(hint);

  return block;
}

function renderMemoryBlock() {
  const block = document.createElement('div');
  block.className = 'block';
  const h = document.createElement('h3');
  h.textContent = 'Memory reader';
  block.append(h);

  const desc = document.createElement('p');
  desc.className = 'muted';
  desc.textContent =
    'Read-only: scans ~/.claude/projects/*/memory/*.md and CLAUDE.md files for bullet-facts. ' +
    'Pane occasionally references them ("Still on the bloodeye health PWA?") instead of generic idle speech.';
  block.append(desc);

  const row = document.createElement('label');
  row.className = 'checkbox-row';
  const checkbox = document.createElement('input');
  checkbox.type = 'checkbox';
  checkbox.checked = !!current.integration.memory.enabled;
  checkbox.addEventListener('change', async () => {
    current.integration.memory.enabled = checkbox.checked;
    try { await saveSettings(current); } catch (e) { console.warn('[integration] save', e); }
  });
  const label = document.createElement('span');
  label.textContent = 'Let Pane reference my memory files';
  row.append(checkbox, label);
  block.append(row);

  return block;
}

function renderRestartBanner() {
  const banner = document.createElement('div');
  banner.className = 'banner';
  banner.innerHTML =
    '<strong>IPC toggle requires restart:</strong> Starting/stopping the HTTP server ' +
    'mid-session is deferred to a later iteration. Quit and relaunch the companion ' +
    'after enabling the IPC server for the first time.';
  return banner;
}
