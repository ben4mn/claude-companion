// Activity catalogue + pure helper functions.
//
// Pulled out of behavior.js so we can unit-test the decision logic
// (pool filtering, quiet-hours detection, preset → config mapping,
// delay scaling, chattiness probability) without spinning up a DOM
// or Tauri runtime. behavior.js consumes what this module exports.

/** Activity catalogue — names, CSS classes, durations (ms), optional
 *  speech-pack keys. `speech` names a key in speech.json's `activity`
 *  map; the actual text is looked up at speak time. */
export const ACTIVITIES = [
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

const CALM_POOL = ['stand', 'sleep', 'nod', 'stretch', 'look', 'think'];
const PLAYFUL_POOL = null; // null = full roster

/** Named presets. The Animation tab's radio group binds to these keys; the
 *  Advanced drawer exposes the raw fields under preset "custom". */
export const PRESETS = Object.freeze({
  calm: {
    activityFrequency: 0.5,
    walkSpeed: 0.7,
    speechChattiness: 0.2,
    activityPool: CALM_POOL,
  },
  normal: {
    activityFrequency: 1.0,
    walkSpeed: 1.0,
    speechChattiness: 0.5,
    activityPool: null,
  },
  playful: {
    activityFrequency: 1.5,
    walkSpeed: 1.2,
    speechChattiness: 0.7,
    activityPool: PLAYFUL_POOL,
  },
});

/** Take an AnimationSettings-shaped object and return the effective config
 *  the behavior engine should use. "custom" preset uses the raw fields as-is;
 *  named presets override them with the preset's canonical values.
 *
 *  Unknown preset names fall back to "normal" — we'd rather keep the app
 *  usable than crash on a stale config field. */
export function resolvePresetConfig(input = {}) {
  const { preset = 'normal' } = input;
  if (preset === 'custom') {
    return {
      preset: 'custom',
      activityFrequency: input.activityFrequency ?? 1.0,
      walkSpeed: input.walkSpeed ?? 1.0,
      speechChattiness: input.speechChattiness ?? 0.5,
      activityPool: input.activityPool ?? null,
      quietHours: input.quietHours ?? { enabled: false, from: '22:00', to: '07:00' },
    };
  }
  const p = PRESETS[preset] ?? PRESETS.normal;
  return {
    preset,
    activityFrequency: p.activityFrequency,
    walkSpeed: p.walkSpeed,
    speechChattiness: p.speechChattiness,
    activityPool: p.activityPool,
    quietHours: input.quietHours ?? { enabled: false, from: '22:00', to: '07:00' },
  };
}

/** Returns the subset of `activities` whose names appear in `pool`. `null`
 *  or `undefined` means "no filter" (full roster). Unknown pool entries are
 *  silently dropped — a stale config should never crash the engine. */
export function filterActivities(activities, pool) {
  if (pool == null) return activities.slice();
  const set = new Set(pool);
  return activities.filter((a) => set.has(a.name));
}

/** Scale a base delay by the activity-frequency multiplier. The multiplier
 *  means *how frequent* — events per unit time — so the delay is the inverse:
 *  - multiplier > 1 → more frequent → shorter delays
 *  - multiplier < 1 → less frequent → longer delays
 *  Never returns 0 or negative — a 0ms setTimeout loop would spin the event loop. */
export function scaleDelay(baseMs, multiplier) {
  const safe = Number.isFinite(multiplier) && multiplier > 0 ? multiplier : 1;
  const scaled = Math.round(baseMs / safe);
  return Math.max(250, scaled);
}

/** "HH:MM" → minutes past midnight. */
function parseHHMM(s) {
  if (typeof s !== 'string') return null;
  const [h, m] = s.split(':').map((x) => Number.parseInt(x, 10));
  if (!Number.isFinite(h) || !Number.isFinite(m)) return null;
  return h * 60 + m;
}

export function isInQuietHours(now, quietHours) {
  if (!quietHours?.enabled) return false;
  const from = parseHHMM(quietHours.from);
  const to = parseHHMM(quietHours.to);
  if (from == null || to == null) return false;
  const nowMins = now.getHours() * 60 + now.getMinutes();
  // Overnight window (e.g. 22:00 → 07:00) crosses midnight.
  if (from > to) return nowMins >= from || nowMins < to;
  return nowMins >= from && nowMins < to;
}

/** The reduced roster used during quiet hours: only low-energy activities. */
export function quietHoursPool(activities) {
  const allow = new Set(['stand', 'sleep', 'nod', 'stretch', 'look']);
  return activities.filter((a) => allow.has(a.name));
}

export function shouldSpeak(chattiness, rng = Math.random) {
  const c = Number.isFinite(chattiness) ? chattiness : 0.5;
  if (c <= 0) return false;
  if (c >= 1) return true;
  return rng() < c;
}

export function pickActivity(activities, config, now = new Date(), rng = Math.random) {
  const resolved = resolvePresetConfig(config);
  let pool = filterActivities(activities, resolved.activityPool);
  if (isInQuietHours(now, resolved.quietHours)) {
    // Intersect the quiet-hours allow-list with whatever the config pool was.
    const quiet = quietHoursPool(activities);
    const names = new Set(quiet.map((a) => a.name));
    const intersected = pool.filter((a) => names.has(a.name));
    pool = intersected.length ? intersected : quiet;
  }
  if (pool.length === 0) {
    // If the user filtered everything out, fall back to `stand` — Pane
    // always has *something* to do.
    return activities.find((a) => a.name === 'stand') ?? activities[0];
  }
  return pool[Math.floor(rng() * pool.length)];
}
