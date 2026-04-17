// E2E: settings window opens via tray menu and persists across close.
//
// Status: scaffolded. See wdio.conf.ts for the macOS driver caveat — these
// specs will run once tauri-driver on macOS is unblocked. Keeping them in
// the repo so the expected behavior is encoded in code, not just prose.

describe('settings window', () => {
  it('opens via tray menu and renders the tabbed shell', async () => {
    // Clicking tray "Settings…" is a native NSMenu interaction; it can't
    // be driven from within the webview. When running under tauri-driver,
    // we use the $cmd channel to invoke the tray handler directly.
    await browser.execute(() => {
      // @ts-expect-error — tauri-driver injects this global in test builds.
      return window.__TAURI_INTERNALS__?.invoke('tauri_test_click_menu', { id: 'settings' });
    });

    // Switch focus to the settings window.
    const handles = await browser.getWindowHandles();
    await browser.switchToWindow(handles[handles.length - 1]);

    const header = await $('.settings-header h1');
    await expect(header).toHaveText('Claude Companion');

    const tabs = await $$('.tab');
    expect(await tabs.length).toBeGreaterThanOrEqual(6);
  });

  it('closing the settings window does not quit the app', async () => {
    const before = await browser.getWindowHandles();
    await browser.closeWindow();
    // Companion window should still exist after settings closes.
    const after = await browser.getWindowHandles();
    expect(after.length).toBeGreaterThanOrEqual(before.length - 1);
    // App should still be alive — a follow-up invoke proves the backend is up.
    const pong = await browser.execute(() => {
      // @ts-expect-error — Tauri global is runtime-injected.
      return window.__TAURI__?.core?.invoke('settings_path');
    });
    expect(typeof pong).toBe('string');
  });
});
