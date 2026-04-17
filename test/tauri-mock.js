// Helper to install a mock Tauri surface in tests.
//
// Usage:
//
//   import { installTauriMock } from '../../test/tauri-mock.js';
//   const mock = installTauriMock({
//     settings_get: () => ({ foo: 'bar' }),
//     settings_set: () => undefined,
//   });
//   // ...test runs code that calls invoke(...)
//   expect(mock.calls.settings_set[0]).toEqual({ key: 'foo', value: 'baz' });
//
// The mock records every invoke/emit call in-order so tests can assert both
// the command name and the arguments without needing a full spy framework.

export function installTauriMock(commandHandlers = {}) {
  const calls = {};
  const emits = [];
  const listeners = new Map(); // event name → Set of callbacks

  const invoke = async (cmd, args) => {
    calls[cmd] ??= [];
    calls[cmd].push(args);
    const handler = commandHandlers[cmd];
    if (typeof handler === 'function') return await handler(args);
    return undefined;
  };

  const emit = async (event, payload) => {
    emits.push({ event, payload });
    const set = listeners.get(event);
    if (set) for (const cb of set) cb({ event, payload });
    return undefined;
  };

  const listen = async (event, cb) => {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event).add(cb);
    return () => listeners.get(event)?.delete(cb);
  };

  const fire = (event, payload) => {
    const set = listeners.get(event);
    if (set) for (const cb of set) cb({ event, payload });
  };

  window.__TAURI__ = {
    core: { invoke },
    event: { emit, listen },
  };

  return { calls, emits, listeners, fire };
}
