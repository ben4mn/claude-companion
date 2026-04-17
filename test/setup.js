// Vitest setup: provide a mockable window.__TAURI__ surface.
//
// JS modules in src/ use the `window.__TAURI__.core.invoke` / `.event.emit` /
// `.event.listen` entry points. Tests install handlers per-suite via
// installTauriMock() from ./tauri-mock.js; this file just guarantees the
// global exists so module-level access doesn't throw on import.
//
// NOTE: intentionally minimal. Each test is responsible for resetting/
// installing its own handlers — we don't auto-reset here because some tests
// span multiple `describe` blocks and share state.

if (typeof window !== 'undefined' && !window.__TAURI__) {
  window.__TAURI__ = {
    core: { invoke: async () => undefined },
    event: { emit: async () => undefined, listen: async () => () => {} },
  };
}
