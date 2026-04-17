// Pane behavior engine — state-machine version.
//
// Pane has three states:
//
//   GROUNDED  — walking back and forth on Claude's bottom line. The only
//               state where idle activities run. X is free within Claude's
//               window range; Y is locked to ground_y.
//   HELD      — mouse is down on Pane. Window follows cursor at 60fps via
//               pane_follow_cursor. Arms flail (act-falling class).
//   FALLING   — after release, window accelerates downward until it reaches
//               the ground line, then transitions back to GROUNDED.
//
// All window positioning goes through Rust commands. The OS-native window
// drag is NOT used — we control every pixel so the motion is smooth and
// predictable.

const MASCOT_NAME = 'Pane';

const mascotEl = document.getElementById('footer-mascot');
const speechEl = document.getElementById('mascot-speech');

let speechPack = { activity: {}, idle: {}, timeOfDay: {}, pet: [], secret: [] };
let manifest = null;
let lastSpeechAt = 0;
let currentTimer = null;
let facingLeft = false;

const STATE = { GROUNDED: 'grounded', HELD: 'held', FALLING: 'falling' };
let paneState = STATE.GROUNDED;
let grabOffset = null;          // cursor-to-window-top-left offset captured on mousedown
let fallVelocity = 0;
let cachedScale = 1;
let cachedGround = null;        // last known { y, minX, maxX } from pane_ground
// Incrementing token: every time we bump this, any walk tick holding the
// previous value aborts before its next IPC call. Prevents stale set_position
// calls from landing after the user picks Pane up or the ground changes.
let walkAbort = 0;

// Tauri handles (populated in init)
let invoke = null;
let win = null;

/* --- Activity catalogue --- */
const ACTIVITIES = [
  { name: 'stand',       cls: 'act-stand',       min: 20000, max: 40000 },
  { name: 'look',        cls: 'act-look',        min:  8000, max: 14000 },
  { name: 'wave',        cls: 'act-wave',        min:  4000, max:  6000, speech: 'wave' },
  { name: 'sleep',       cls: 'act-sleep',       min: 30000, max: 60000, speech: 'sleep' },
  { name: 'stretch',     cls: 'act-stretch',     min:  5000, max:  8000 },
  { name: 'nod',         cls: 'act-nod',         min:  4000, max:  6000 },
  { name: 'think',       cls: 'act-think',       min: 15000, max: 25000 },
  { name: 'dance',       cls: 'act-dance',       min:  4000, max:  7000 },
  { name: 'type',        cls: 'act-type',        min: 10000, max: 18000 },
  { name: 'bounce',      cls: 'act-bounce',      min:  3000, max:  5000 },
  { name: 'sweep',       cls: 'act-sweep',       min:  8000, max: 14000, speech: 'sweep' },
  { name: 'phone',       cls: 'act-phone',       min: 10000, max: 20000, speech: 'phone' },
  { name: 'code',        cls: 'act-code',        min: 12000, max: 22000, speech: 'code' },
  { name: 'mop',         cls: 'act-mop',         min:  8000, max: 14000 },
  { name: 'shimmy',      cls: 'act-shimmy',      min:  3000, max:  5000 },
  { name: 'antenna-fix', cls: 'act-antenna-fix', min:  2000, max:  3000 },
];

const rand = (min, max) => Math.floor(Math.random() * (max - min + 1)) + min;
const pick = (arr) => arr[Math.floor(Math.random() * arr.length)];

function clearActivity() {
  if (!mascotEl) return;
  const classes = [...mascotEl.classList];
  for (const c of classes) {
    if (
      c.startsWith('act-') ||
      c === 'walking' ||
      c === 'walk-anticipate' ||
      c === 'walk-arrive' ||
      c === 'dragging'
    ) {
      mascotEl.classList.remove(c);
    }
  }
}

function showSpeech(text, duration = 3000, priority = false) {
  if (!speechEl || !text) return;
  const now = Date.now();
  if (!priority && now - lastSpeechAt < 6000) return;
  lastSpeechAt = now;
  speechEl.textContent = text;
  speechEl.classList.add('visible');
  setTimeout(() => speechEl.classList.remove('visible'), duration);
}

function speechFor(activityName) {
  const key = speechPack.activity?.[activityName];
  if (!key) return null;
  return Array.isArray(key) ? pick(key) : key;
}

/* ============================================================================
 * Ground polling — keep Pane stuck to Claude's bottom edge.
 *
 * Claude's window may be moved or resized by the user at any time. The
 * Rust pane_ground command returns current ground_y + X range; we poll it
 * periodically and, when GROUNDED, nudge Pane back onto the line.
 * ========================================================================== */
async function refreshGround() {
  if (!invoke) return;
  try {
    const g = await invoke('pane_ground');
    if (!g) return;
    const prev = cachedGround;
    const next = { y: g[0], minX: g[1], maxX: g[2] };
    cachedGround = next;
    // If Claude's window moved meaningfully, abort any in-flight walk so it
    // doesn't keep animating toward stale coords — the walk will resume on
    // the next scheduler tick using the fresh ground.
    if (
      prev &&
      (Math.abs(prev.y - next.y) > 2 ||
        Math.abs(prev.minX - next.minX) > 2 ||
        Math.abs(prev.maxX - next.maxX) > 2)
    ) {
      walkAbort++;
    }
  } catch (e) {}
}

async function snapToGround() {
  if (paneState !== STATE.GROUNDED) return;
  await refreshGround();
  if (!cachedGround || !invoke || !win) return;
  // While the walk animation owns X, don't also write X here — that was the
  // source of visible jank when walk and the poll disagreed. The walk tick
  // now re-reads cachedGround.y each frame, so Y stays correct during walks.
  if (mascotEl && mascotEl.classList.contains('walking')) return;
  try {
    const pos = await win.outerPosition();
    const currentX = pos.x / cachedScale;
    const currentY = pos.y / cachedScale;
    // Constrain X to Claude's range and Y to the ground line.
    const clampedX = Math.max(cachedGround.minX, Math.min(cachedGround.maxX, currentX));
    if (Math.abs(clampedX - currentX) > 1 || Math.abs(cachedGround.y - currentY) > 1) {
      await invoke('pane_set_position', { x: clampedX, y: cachedGround.y });
    }
  } catch (e) {}
}

/* ============================================================================
 * Walking — moves the window horizontally along the ground line.
 * ========================================================================== */
async function walk() {
  if (paneState !== STATE.GROUNDED) {
    scheduleNext();
    return;
  }
  await refreshGround();
  if (!cachedGround || !invoke || !win) {
    scheduleNext();
    return;
  }
  const { y: groundY, minX, maxX } = cachedGround;
  if (maxX - minX < 8) {
    // No room to walk
    scheduleNext();
    return;
  }

  let currentX;
  try {
    const pos = await win.outerPosition();
    currentX = pos.x / cachedScale;
  } catch (e) {
    scheduleNext();
    return;
  }

  const targetX = rand(minX, maxX);
  const distance = Math.abs(targetX - currentX);
  const duration = Math.max(600, distance * 10); // ms

  facingLeft = targetX < currentX;
  mascotEl.classList.toggle('face-left', facingLeft);

  mascotEl.classList.add('walk-anticipate');
  await new Promise(r => setTimeout(r, 200));
  mascotEl.classList.remove('walk-anticipate');
  mascotEl.classList.add('walking');

  const startX = currentX;
  const startTime = performance.now();
  // Snapshot the abort token; any bump invalidates this walk cycle.
  const myAbort = walkAbort;

  return new Promise((resolve) => {
    const tick = async (now) => {
      // Bail before issuing IPC if state changed or abort was bumped. Checking
      // BOTH sides of the await is essential — otherwise the in-flight
      // set_position completes after HELD takes over and yanks Pane backward.
      if (paneState !== STATE.GROUNDED || myAbort !== walkAbort) {
        mascotEl.classList.remove('walking');
        resolve();
        return;
      }
      const t = Math.min(1, (now - startTime) / duration);
      // ease-in-out
      const eased = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
      const x = startX + (targetX - startX) * eased;
      // Re-read ground in case Claude moved mid-walk, so we're targeting the
      // current bottom line, not the one walk() snapshotted at start.
      const liveGroundY = cachedGround ? cachedGround.y : groundY;
      try { await invoke('pane_set_position', { x, y: liveGroundY }); } catch (e) {}
      if (paneState !== STATE.GROUNDED || myAbort !== walkAbort) {
        mascotEl.classList.remove('walking');
        resolve();
        return;
      }
      if (t < 1) {
        requestAnimationFrame(tick);
      } else {
        mascotEl.classList.remove('walking');
        mascotEl.classList.add('walk-arrive');
        setTimeout(() => {
          mascotEl.classList.remove('walk-arrive');
          scheduleNext();
          resolve();
        }, 300);
      }
    };
    requestAnimationFrame(tick);
  });
}

/* ============================================================================
 * Activities — the random idle repertoire. Only fires in GROUNDED state.
 * ========================================================================== */
function doActivity() {
  if (paneState !== STATE.GROUNDED) return scheduleNext();
  const activity = pick(ACTIVITIES);
  clearActivity();
  mascotEl.classList.add(activity.cls);

  if (activity.speech) {
    const text = speechFor(activity.speech);
    if (text) showSpeech(text);
  }

  const dur = rand(activity.min, activity.max);
  currentTimer = setTimeout(() => {
    clearActivity();
    scheduleNext();
  }, dur);
}

function scheduleNext() {
  if (currentTimer) clearTimeout(currentTimer);
  if (paneState !== STATE.GROUNDED) return;
  const shouldWalk = Math.random() < 0.35;
  const delay = rand(2000, 4000);
  currentTimer = setTimeout(() => {
    if (paneState !== STATE.GROUNDED) return;
    if (shouldWalk) walk();
    else doActivity();
  }, delay);
}

/* ============================================================================
 * HELD state — window follows cursor at 60fps.
 * ========================================================================== */
async function enterHeld(_e) {
  if (currentTimer) clearTimeout(currentTimer);
  clearActivity();
  // Abort any in-flight walk tick so its pending pane_set_position doesn't
  // land after the held loop takes over and yank Pane backward.
  walkAbort = (walkAbort || 0) + 1;
  paneState = STATE.HELD;
  // act-falling = arms up flailing + panic eyes. Exactly the "yoink!" vibe.
  mascotEl.classList.add('act-falling');
  mascotEl.classList.add('dragging');

  // Freeze the Rust watcher so it can't hide/reposition Pane mid-drag.
  try { await invoke('pane_set_interacting', { active: true }); } catch (e) {}

  // Capture the grab offset in Rust's own cursor/window coord system so the
  // held loop — which also reads the cursor from Rust — has a consistent
  // reference. Using e.screenX/Y can disagree with NSEvent.mouseLocation on
  // Retina or multi-monitor setups, causing a pickup jump.
  try {
    const off = await invoke('pane_drag_start');
    grabOffset = [off[0], off[1]];
  } catch (err) {
    grabOffset = [mascotEl.offsetWidth / 2, mascotEl.offsetHeight / 2];
  }

  heldLoop();
}

async function heldLoop() {
  if (paneState !== STATE.HELD || !invoke || !grabOffset) return;
  try {
    await invoke('pane_follow_cursor', {
      offsetX: grabOffset[0],
      offsetY: grabOffset[1],
    });
  } catch (e) {}
  if (paneState === STATE.HELD) requestAnimationFrame(heldLoop);
}

/* ============================================================================
 * FALLING state — gravity animation down to ground_y.
 * ========================================================================== */
async function enterFalling() {
  paneState = STATE.FALLING;
  mascotEl.classList.remove('dragging');
  // Keep act-falling class — same visual, now the window is actually falling.
  fallVelocity = 0;
  fallStep();
}

async function fallStep() {
  if (paneState !== STATE.FALLING || !invoke || !win) return;
  await refreshGround();
  let currentX, currentY;
  try {
    const pos = await win.outerPosition();
    currentX = pos.x / cachedScale;
    currentY = pos.y / cachedScale;
  } catch (e) {
    enterGrounded();
    return;
  }

  if (!cachedGround) {
    // No Claude window to land on — just settle in place.
    enterGrounded();
    return;
  }
  const groundY = cachedGround.y;

  if (currentY >= groundY) {
    // Snap exactly to ground and settle.
    try { await invoke('pane_set_position', { x: currentX, y: groundY }); } catch (e) {}
    enterGrounded();
    return;
  }

  fallVelocity += 1.6; // gravity accel per frame
  const newY = Math.min(currentY + fallVelocity, groundY);
  try { await invoke('pane_set_position', { x: currentX, y: newY }); } catch (e) {}
  requestAnimationFrame(fallStep);
}

function enterGrounded() {
  paneState = STATE.GROUNDED;
  clearActivity();
  // Squash-and-stretch landing
  mascotEl.classList.add('walk-arrive');
  // Release the watcher freeze — it can resume managing visibility now.
  if (invoke) invoke('pane_set_interacting', { active: false }).catch(() => {});
  setTimeout(() => {
    mascotEl.classList.remove('walk-arrive');
    scheduleNext();
  }, 300);
}

/* ============================================================================
 * Interaction wiring.
 * ========================================================================== */
function setupInteraction() {
  if (!mascotEl) return;

  mascotEl.addEventListener('mousedown', (e) => {
    // Any mousedown on Pane = pick up.
    enterHeld(e).catch(() => {});
  });

  const release = () => {
    if (paneState === STATE.HELD) enterFalling();
  };
  document.addEventListener('mouseup', release);
  // Backup: if the cursor leaves the window with the button somehow released,
  // still fall. (OS drag-area behavior can be flaky; this catches edge cases.)
  window.addEventListener('blur', () => {
    if (paneState === STATE.HELD) enterFalling();
  });
}

/* ============================================================================
 * Click-through polling — per-pixel pass-through when cursor isn't on Pane.
 * Paused during HELD/FALLING so we never cancel our own motion handling.
 * ========================================================================== */
function setupClickThrough() {
  if (!win || !invoke) return;
  let currentIgnore = null;

  const setIgnore = async (state) => {
    if (currentIgnore === state) return;
    currentIgnore = state;
    try { await win.setIgnoreCursorEvents(state); } catch (e) {}
  };

  setIgnore(true);

  const tick = async () => {
    // While we're moving Pane ourselves, keep the window fully interactive
    // so mouse events keep flowing and setPosition isn't interrupted.
    if (paneState === STATE.HELD || paneState === STATE.FALLING) {
      await setIgnore(false);
      return;
    }
    try {
      if (!mascotEl) return;
      const r = mascotEl.getBoundingClientRect();
      const pad = 4;
      const bbox = [r.left - pad, r.top - pad, r.width + pad * 2, r.height + pad * 2];
      const over = await invoke('cursor_over', { bbox });
      setIgnore(!over);
    } catch (e) {}
  };
  setInterval(tick, 40);
}

/* ============================================================================
 * Time-of-day + pack loader (unchanged from previous pass).
 * ========================================================================== */
function timeOfDayGreeting() {
  const hour = new Date().getHours();
  let key = null;
  if (hour >= 23 || hour < 4) key = 'late';
  else if (hour >= 5 && hour < 9) key = 'morning';
  if (!key) return;
  const pool = speechPack.timeOfDay?.[key];
  if (pool?.length) setTimeout(() => showSpeech(pick(pool), 4000, true), 6000);
}

async function loadPack(packId = 'pane') {
  try {
    const manifestRes = await fetch(`packs/${packId}/manifest.json`);
    manifest = await manifestRes.json();
    if (manifest.theme) {
      for (const [k, v] of Object.entries(manifest.theme)) {
        document.documentElement.style.setProperty(k, v);
      }
    }
    if (manifest.files?.speech) {
      const speechRes = await fetch(`packs/${packId}/${manifest.files.speech}`);
      speechPack = await speechRes.json();
    }
  } catch (err) {
    console.warn(`[${MASCOT_NAME}] pack load failed:`, err);
  }
}

async function waitForTauri(timeoutMs = 3000) {
  const start = Date.now();
  while (!window.__TAURI__?.core?.invoke) {
    if (Date.now() - start > timeoutMs) return null;
    await new Promise(r => setTimeout(r, 20));
  }
  return window.__TAURI__;
}

/* ============================================================================
 * Init
 * ========================================================================== */
async function init() {
  if (!mascotEl) return;
  await loadPack('pane');

  const tauri = await waitForTauri();
  invoke = tauri?.core?.invoke ?? null;
  win =
    tauri?.webviewWindow?.getCurrentWebviewWindow?.() ||
    tauri?.window?.getCurrentWindow?.() ||
    tauri?.window?.getCurrent?.() ||
    null;

  if (win?.scaleFactor) {
    try { cachedScale = await win.scaleFactor(); } catch (e) {}
  }

  setupInteraction();
  setupClickThrough();
  timeOfDayGreeting();

  // Continuous ground poll so Pane follows Claude if the user moves or
  // resizes Claude's window. Runs at ~16Hz: fast enough that a window drag
  // looks like smooth following rather than discrete jumps, slow enough
  // that the IPC cost stays negligible.
  setInterval(() => {
    if (paneState === STATE.GROUNDED) snapToGround();
    else refreshGround();
  }, 60);

  // Initial snap to the ground line.
  await snapToGround();
  scheduleNext();

  window.__pane = {
    force(name) {
      clearActivity();
      const act = ACTIVITIES.find(a => a.name === name);
      if (act) mascotEl.classList.add(act.cls);
    },
    walk,
    say: (t) => showSpeech(t, 3000, true),
    state: () => paneState,
    ground: () => cachedGround,
    name: MASCOT_NAME,
  };
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
