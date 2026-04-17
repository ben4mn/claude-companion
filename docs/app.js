// Pane's web-demo brain — a trimmed port of src/engine/behavior.js.
// States: grounded (walk the ground line), held (drag), falling (gravity).
// Stage-bound so Pane can't escape his tile.

const stage = document.getElementById('pane-stage');
const mascot = document.getElementById('footer-mascot');
if (!stage || !mascot) throw new Error('stage missing');

const GROUND_INSET = 50;   // matches .stage::after `bottom: 50px`
const MASCOT_W = 96;
const MASCOT_H = 96;
const GRAVITY = 1.4;
const STATE = { GROUNDED: 'grounded', HELD: 'held', FALLING: 'falling' };

let state = STATE.GROUNDED;
let x = 40;                // left px, within stage
let y = 0;                 // distance above ground (0 = on floor)
let vy = 0;
let facingLeft = false;
let walkAbort = 0;
let walkTimer = null;
let idleTimer = null;
let grabDX = 0, grabDY = 0;

// Act classes Pane's real animations.css understands. Kept small — a demo
// doesn't need every idle in the native app's library.
const IDLES = ['act-wave', 'act-think', 'act-dance', 'act-bounce', 'act-stretch', 'act-nod', 'act-look'];

function stageBounds() {
  const r = stage.getBoundingClientRect();
  return {
    w: r.width,
    h: r.height,
    maxX: Math.max(0, r.width - MASCOT_W),
    groundBottom: GROUND_INSET,
  };
}

function render() {
  mascot.style.left = `${x}px`;
  mascot.style.bottom = `${GROUND_INSET + y}px`;
  mascot.classList.toggle('face-left', facingLeft);
}

function clearActivity() {
  for (const c of [...mascot.classList]) {
    if (c.startsWith('act-') || c === 'walking' || c === 'walk-anticipate' || c === 'walk-arrive') {
      mascot.classList.remove(c);
    }
  }
}

function rand(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; }
function pick(arr) { return arr[Math.floor(Math.random() * arr.length)]; }

function scheduleIdle() {
  clearTimeout(idleTimer);
  if (state !== STATE.GROUNDED) return;
  const delay = rand(2200, 4500);
  idleTimer = setTimeout(() => {
    if (state !== STATE.GROUNDED) return;
    Math.random() < 0.55 ? walk() : doIdle();
  }, delay);
}

function doIdle() {
  if (state !== STATE.GROUNDED) return;
  const act = pick(IDLES);
  clearActivity();
  mascot.classList.add(act);
  const dur = rand(2500, 4200);
  idleTimer = setTimeout(() => {
    if (state !== STATE.GROUNDED) { return; }
    clearActivity();
    scheduleIdle();
  }, dur);
}

function walk() {
  if (state !== STATE.GROUNDED) return;
  const b = stageBounds();
  if (b.maxX < 20) { scheduleIdle(); return; }
  const target = rand(0, b.maxX);
  const dist = Math.abs(target - x);
  if (dist < 8) { scheduleIdle(); return; }
  const duration = Math.max(600, dist * 11);
  facingLeft = target < x;
  const myAbort = ++walkAbort;

  clearActivity();
  mascot.classList.add('walk-anticipate');
  setTimeout(() => {
    if (myAbort !== walkAbort || state !== STATE.GROUNDED) return;
    mascot.classList.remove('walk-anticipate');
    mascot.classList.add('walking');
    const startX = x;
    const t0 = performance.now();
    const tick = (now) => {
      if (myAbort !== walkAbort || state !== STATE.GROUNDED) {
        mascot.classList.remove('walking');
        return;
      }
      const t = Math.min(1, (now - t0) / duration);
      const e = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
      x = startX + (target - startX) * e;
      render();
      if (t < 1) {
        requestAnimationFrame(tick);
      } else {
        mascot.classList.remove('walking');
        mascot.classList.add('walk-arrive');
        setTimeout(() => {
          mascot.classList.remove('walk-arrive');
          scheduleIdle();
        }, 280);
      }
    };
    requestAnimationFrame(tick);
  }, 180);
}

/* --- Held --- */
function onPointerDown(e) {
  if (e.button !== undefined && e.button !== 0) return;
  e.preventDefault();
  walkAbort++;
  clearTimeout(idleTimer);
  clearActivity();
  mascot.classList.add('act-falling');
  mascot.classList.add('dragging');
  state = STATE.HELD;
  const rect = mascot.getBoundingClientRect();
  grabDX = e.clientX - rect.left;
  grabDY = e.clientY - rect.top;
  mascot.setPointerCapture?.(e.pointerId);
}

function onPointerMove(e) {
  if (state !== STATE.HELD) return;
  const sr = stage.getBoundingClientRect();
  const rawX = e.clientX - sr.left - grabDX;
  const rawY = e.clientY - sr.top - grabDY;
  const b = stageBounds();
  x = Math.max(0, Math.min(b.maxX, rawX));
  const topFromTop = Math.max(0, Math.min(b.h - MASCOT_H, rawY));
  y = (b.h - MASCOT_H - topFromTop) - GROUND_INSET;
  render();
}

function onPointerUp() {
  if (state !== STATE.HELD) return;
  mascot.classList.remove('dragging');
  state = STATE.FALLING;
  vy = 0;
  requestAnimationFrame(fallStep);
}

/* --- Falling --- */
function fallStep() {
  if (state !== STATE.FALLING) return;
  vy += GRAVITY;
  y = Math.max(0, y - vy);
  render();
  if (y <= 0) {
    y = 0;
    render();
    clearActivity();
    mascot.classList.add('walk-arrive');
    setTimeout(() => { mascot.classList.remove('walk-arrive'); scheduleIdle(); }, 300);
    state = STATE.GROUNDED;
    return;
  }
  requestAnimationFrame(fallStep);
}

/* --- Wire up --- */
mascot.addEventListener('pointerdown', onPointerDown);
window.addEventListener('pointermove', onPointerMove);
window.addEventListener('pointerup', onPointerUp);
window.addEventListener('pointercancel', onPointerUp);
window.addEventListener('blur', onPointerUp);

// Start on the ground, wander after a short beat.
render();
setTimeout(scheduleIdle, 800);
