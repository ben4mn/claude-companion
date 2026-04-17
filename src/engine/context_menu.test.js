import { describe, it, expect, beforeEach, vi } from 'vitest';
import { attachCompanionContextMenu } from './context_menu.js';

describe('attachCompanionContextMenu', () => {
  let el;

  beforeEach(() => {
    el = document.createElement('div');
    el.id = 'footer-mascot';
    document.body.append(el);
  });

  it('invokes show_companion_menu on right-click', () => {
    const invoke = vi.fn();
    attachCompanionContextMenu(el, invoke);
    const evt = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    el.dispatchEvent(evt);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('show_companion_menu');
  });

  it('preventDefaults the browser context menu so only the native popup shows', () => {
    const invoke = vi.fn();
    attachCompanionContextMenu(el, invoke);
    const evt = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    el.dispatchEvent(evt);
    expect(evt.defaultPrevented).toBe(true);
  });

  it('is a no-op when invoke is null (Tauri not ready)', () => {
    // Pane may fire contextmenu before Tauri initializes; the handler must
    // not throw in that window.
    attachCompanionContextMenu(el, null);
    const evt = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    expect(() => el.dispatchEvent(evt)).not.toThrow();
  });

  it('does not attach a listener when el is null', () => {
    expect(() => attachCompanionContextMenu(null, vi.fn())).not.toThrow();
  });
});
