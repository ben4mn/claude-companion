# Claude Companion

A tiny always-on-top overlay that brings animated **Companions** to the [Claude Desktop](https://claude.com/download) chat app on macOS. Ships with **Pane** — the SVG character from [ben4mn/panestreet](https://github.com/ben4mn/panestreet) — as the reference pack.

Pane lives on the bottom edge of your Claude window, wanders back and forth, reacts to being clicked, and disappears when you switch away from Claude.

## Why

Anthropic doesn't ship a plugin API for putting custom visual components inside the Claude Desktop window — `.mcpb` Desktop Extensions add tools but no UI, and MCP-UI widgets only render inside chat messages. Claude Companion gets the same effect by running as a separate, always-on-top, transparent, click-through Tauri window that tracks Claude's frontmost state and window bounds. Claude Desktop is never touched.

## Status

**v0.1** — scaffold, reference pack, Pane works. Pick him up, throw him around, watch him fall back to the bottom line.

## Install & run (dev)

Requires Rust + Node. macOS only for now.

```bash
git clone https://github.com/ben4mn/claude-companion.git
cd claude-companion
pnpm install        # or npm install
pnpm tauri dev      # builds + launches; first build compiles the Rust tree (~3–5 min)
```

Pane appears bottom-right of your Claude Desktop window. Quit him via the menu-bar tray icon.

## How Pane behaves

**Three states:**

| State | What's happening |
|-------|------------------|
| **Grounded** | Normal life — wanders the bottom edge of Claude's window, cycles idle activities (wave, think, sleep, dance, type, sweep, phone, code, …), occasional speech bubbles. |
| **Held** | You click-and-hold him. Arms flail, eyes go wide. Window follows your cursor at 60fps. Drop him anywhere. |
| **Falling** | You let go. Gravity pulls him down until he lands on Claude's bottom edge, where he resumes Grounded life. |

**Claude-tracking:**

- **Appears** with a bottom-right snap when Pane first launches and Claude is open
- **Hides** when you switch to any other app (Chrome, Finder, etc.) — your screen stays clean
- **Reappears** when you come back to Claude
- **Quits himself** when you quit Claude Desktop
- **Clicking Pane doesn't make him vanish** — the watcher knows when Pane himself is being interacted with

**Stays out of your way:**

- No Dock icon, no app-switcher entry, no menu bar (macOS `Accessory` activation policy)
- Per-pixel click-through — clicks on empty space beside Pane pass through to Claude Desktop beneath
- Always-on-top, transparent, no window chrome

## Architecture

```
claude-companion/
├── src-tauri/              # Rust side — window, watcher, IPC commands
│   ├── src/lib.rs          # All Rust logic in one file
│   ├── Cargo.toml
│   └── tauri.conf.json     # transparent, frameless, always-on-top, focus:false
├── src/
│   ├── index.html          # Pane's SVG (ported from PaneStreet)
│   ├── engine/behavior.js  # State machine: grounded / held / falling
│   ├── styles/base.css     # Pack-agnostic window layout
│   └── packs/pane/         # The reference Companion Pack
│       ├── manifest.json   # id, theme colors, viewBox
│       ├── animations.css  # All act-* keyframes (20+ animations)
│       └── speech.json     # Speech pools
└── README.md
```

**Rust commands (JS-callable):**

- `cursor_over(bbox)` — hit-test global cursor against Pane's DOM bbox. Drives click-through.
- `pane_follow_cursor(offsetX, offsetY)` — move window so cursor stays at a fixed offset on Pane. Used by the `Held` loop.
- `pane_set_position(x, y)` — move window to absolute logical coords. Used by `Falling` gravity and `Grounded` walk.
- `pane_ground()` — returns `(groundY, minX, maxX)` derived from Claude's on-screen window.
- `pane_set_interacting(active)` — pauses the watcher so a drag can't be yanked mid-motion.

**macOS APIs used:**

- `NSApplication.setActivationPolicy(.accessory)` — no Dock icon / app switcher
- `NSWorkspace.sharedWorkspace` — `runningApplications`, `frontmostApplication`
- `NSEvent.mouseLocation` — global cursor polling
- `NSScreen` — primary-screen height for coordinate flip
- `CGWindowListCopyWindowInfo` — Claude Desktop's on-screen window frame (no Accessibility permission required)

## Companion Packs

Every character ships as a folder under `src/packs/<id>/`:

```
packs/pane/
├── manifest.json    # id, name, theme colors, viewBox
├── animations.css   # all act-* keyframes
└── speech.json      # speech pools (activity, idle, time-of-day, pet, secret)
```

v0.1 ships only Pane. The loader reads the manifest, so adding a new character is a matter of dropping a folder here and pointing `loadPack()` at it in `src/engine/behavior.js`. The Companion Pack spec will stabilize in v1.0.

## Roadmap

- **v0.2** — import the remaining PaneStreet animations (moonwalk, watching-build, impressed, falling-arms already present; hiccup / stumble / startled / double-take / yawn wired into the random idle picker)
- **v0.3** — optional bundled MCP server so Claude (the model) can emit structured events Pane reacts to (thinking / done / error), inspired by [claude-buddy](https://github.com/1270011/claude-buddy)
- **v0.4** — Windows + Linux builds, auto-start at login
- **v1.0** — Companion Pack spec frozen, second reference character shipped, `.dmg` + one-click install

## Ported code

Pane's visual assets come straight from [PaneStreet](https://github.com/ben4mn/panestreet):

- SVG body → `src/index.html` (from `PaneStreet/src/index.html:90-148`)
- All `act-*` + `robot-*` keyframes → `src/packs/pane/animations.css` (from `PaneStreet/src/css/main.css:1473-2346`)
- Behavior engine (trimmed and refactored into a state machine) → `src/engine/behavior.js` (from `PaneStreet/src/js/app.js:2670-4060`)

## Legal

- "Claude" and "Claude Desktop" are trademarks of Anthropic PBC. This project is unaffiliated with and unendorsed by Anthropic. It observes Claude Desktop via public macOS APIs (NSWorkspace + CGWindow); it does not modify, reverse engineer, or interact with Claude Desktop internals.
- Pane artwork and animation design © ben4mn, reused from PaneStreet.
- Code in this repository is MIT-licensed. See `LICENSE`.
