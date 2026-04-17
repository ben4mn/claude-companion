// App-awareness commentary in desktop mode.
//
// When the user switches to a frontmost app we've got comments for, and
// Pane is idle (not being dragged / falling) in desktop mode, occasionally
// drop a line from `data/app_comments.json`. Gated through `shouldCommentOnApp`
// so the decision logic is unit-testable without any live Tauri plumbing.

/** Decide whether to fire an app-awareness comment.
 *
 *  All inputs are plain values — this function is pure and trivially
 *  testable. Caller (behavior.js) snapshots current state and passes it in.
 *
 *  state: {
 *    bundleId: string — frontmost app's bundle id
 *    mode: 'claudeOnly' | 'desktop'
 *    enabled: bool — user's opt-in
 *    allowlist: string[] — bundle ids the user approved commentary for
 *    frequencyMs: number — min delay between two comments
 *    lastCommentAt: number — Date.now() when last comment fired (0 = never)
 *    now: number — current Date.now()
 *    engaged: bool — true when Pane is being dragged/falling
 *    commentsMap: { [bundleId]: string[] } — available comments
 *  }
 */
export function shouldCommentOnApp(state) {
  const {
    bundleId, mode, enabled, allowlist = [],
    frequencyMs = 45000, lastCommentAt = 0, now = Date.now(),
    engaged = false, commentsMap = {},
  } = state || {};

  if (!enabled) return false;
  if (mode !== 'desktop') return false;
  if (typeof bundleId !== 'string' || !bundleId) return false;
  if (!allowlist.includes(bundleId)) return false;
  if (engaged) return false;

  const pool = commentsMap[bundleId];
  if (!Array.isArray(pool) || pool.length === 0) return false;

  if (now - lastCommentAt < frequencyMs) return false;

  return true;
}

/** Pick a random comment for the given bundle, or null if no comments.
 *  rng is injectable for deterministic testing. */
export function pickCommentFor(bundleId, commentsMap = {}, rng = Math.random) {
  const pool = commentsMap[bundleId];
  if (!Array.isArray(pool) || pool.length === 0) return null;
  const idx = Math.floor(rng() * pool.length);
  return pool[Math.min(idx, pool.length - 1)] ?? null;
}
