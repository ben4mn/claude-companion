// WebdriverIO config for Tauri 2.x E2E.
//
// Runtime note (macOS):
//   `tauri-driver` is primarily tested on Linux + Windows. On macOS it
//   requires a WebKit WebDriver shim that isn't bundled. Specs below are
//   authored against the standard WebdriverIO API and will run once the
//   macOS driver story stabilizes. For now:
//     - `npm run test:e2e` scaffolds the run but is expected to skip/fail
//       on macOS without the driver binary on PATH.
//     - Manual smoke in the Verification sections of each phase covers
//       what E2E cannot yet automate.
//
// Set TAURI_APP_PATH to the built app bundle if running locally:
//   export TAURI_APP_PATH=./src-tauri/target/debug/claude-companion

export const config: WebdriverIO.Config = {
  runner: 'local',
  framework: 'mocha',
  specs: ['./e2e/**/*.spec.ts'],
  maxInstances: 1,
  capabilities: [
    {
      'tauri:options': {
        application: process.env.TAURI_APP_PATH ?? './src-tauri/target/debug/claude-companion',
      },
      browserName: 'tauri',
    } as any,
  ],
  reporters: ['spec'],
  mochaOpts: { ui: 'bdd', timeout: 60_000 },
  hostname: '127.0.0.1',
  port: 4444,
  logLevel: 'info',
  waitforTimeout: 10_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 3,
};
