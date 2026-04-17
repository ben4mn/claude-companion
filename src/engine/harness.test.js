import { describe, it, expect } from 'vitest';
import { installTauriMock } from '../../test/tauri-mock.js';

describe('test harness sanity', () => {
  it('jsdom is active', () => {
    expect(typeof window).toBe('object');
    expect(typeof document).toBe('object');
  });

  it('installTauriMock wires invoke and records calls', async () => {
    const mock = installTauriMock({
      ping: async () => 'pong',
    });
    const result = await window.__TAURI__.core.invoke('ping', { n: 1 });
    expect(result).toBe('pong');
    expect(mock.calls.ping).toEqual([{ n: 1 }]);
  });

  it('installTauriMock event bus dispatches to listeners', async () => {
    const mock = installTauriMock();
    const seen = [];
    await window.__TAURI__.event.listen('x', (e) => seen.push(e.payload));
    mock.fire('x', 42);
    expect(seen).toEqual([42]);
  });
});
