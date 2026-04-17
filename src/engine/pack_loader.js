// Companion pack loader.
//
// A pack is: `packs/<id>/{manifest.json, body.svg, animations.css, speech.json}`.
// The manifest declares `parts: [{ key, label, default }]` that the Companion
// tab exposes as color pickers. At load time we merge the user's saved
// per-pack theme overrides onto the pack's defaults and write the result as
// `--part-<key>` CSS variables on :root — the pack's SVG references those
// variables so recoloring is instant.
//
// Pure helpers (mergeTheme, partCssVarName, themeToCssVars) are unit-tested
// in pack_loader.test.js. The side-effecting `loadPack` wraps them for
// behavior.js.

/** IDs of every pack the gallery knows about. Must match the directory names
 *  under src/packs/. Keep alphabetized after pane (which stays first). */
export const PACK_IDS = ['pane', 'sprite', 'blob', 'ghost', 'cat'];

/** camelCase → kebab-case for CSS var names. "antennaTip" → "antenna-tip". */
function kebab(s) {
  return String(s).replace(/[A-Z]/g, (c) => '-' + c.toLowerCase()).replace(/^-/, '');
}

export function partCssVarName(key) {
  return `--part-${kebab(key)}`;
}

/** Merge a manifest's `parts[].default` colors with the user's per-pack
 *  override map. Keys not declared in the manifest are dropped so stale
 *  overrides from an older pack version can't inject junk vars. */
export function mergeTheme(manifest, overrides) {
  const out = {};
  if (!manifest?.parts || !Array.isArray(manifest.parts)) return out;
  const valid = new Set(manifest.parts.map((p) => p.key));
  for (const part of manifest.parts) out[part.key] = part.default;
  if (overrides && typeof overrides === 'object') {
    for (const [k, v] of Object.entries(overrides)) {
      if (valid.has(k)) out[k] = v;
    }
  }
  return out;
}

/** Convert a merged theme ({ partKey: "#rrggbb" }) into a map of CSS vars
 *  ready for `style.setProperty()`. */
export function themeToCssVars(theme) {
  const out = {};
  for (const [k, v] of Object.entries(theme || {})) {
    out[partCssVarName(k)] = v;
  }
  return out;
}

/** Apply a theme's CSS vars to :root so the pack's SVG picks up the colors. */
export function applyThemeToRoot(theme, root = document.documentElement) {
  const vars = themeToCssVars(theme);
  for (const [name, value] of Object.entries(vars)) {
    root.style.setProperty(name, value);
  }
}

/** Clear any previously-applied --part-* vars from :root so switching packs
 *  doesn't leave orphan values behind. */
export function clearPartVars(root = document.documentElement) {
  const style = root.style;
  for (let i = style.length - 1; i >= 0; i--) {
    const name = style[i];
    if (name?.startsWith('--part-')) {
      style.removeProperty(name);
    }
  }
}

// Cache manifests across load/theme-apply calls so rapid color-picker drags
// don't re-hit the file system every time.
const manifestCache = new Map();

async function getManifest(packId) {
  if (manifestCache.has(packId)) return manifestCache.get(packId);
  const manifest = await fetchJson(`packs/${packId}/manifest.json`);
  manifestCache.set(packId, manifest);
  return manifest;
}

/** Load a pack by id and wire it into the DOM. Returns the loaded manifest +
 *  speech pack so behavior.js can use them.
 *
 *  Side effects:
 *   - swaps the body SVG inside #footer-mascot
 *   - swaps the pack-specific animations stylesheet
 *   - applies the merged theme as CSS vars on :root
 *
 *  Use this on pack switches. For theme-only updates (color picker drags)
 *  use `applyPackTheme` — that avoids the body/stylesheet swap and the
 *  brief empty-DOM flicker that caused the "companion dropped below screen"
 *  glitch when the user was scrubbing color pickers.
 */
export async function loadPack(packId, userThemes = {}) {
  const manifest = await getManifest(packId);
  const speech = manifest?.files?.speech
    ? await fetchJson(`packs/${packId}/${manifest.files.speech}`).catch(() => defaultSpeech())
    : defaultSpeech();

  // Swap the pack-specific animations stylesheet. We tag the link so
  // subsequent loads find and replace it rather than stacking one per swap.
  await swapPackStylesheet(packId, manifest?.files?.animations ?? 'animations.css');

  // Swap the body SVG. Preserve the speech-bubble node so it survives pack
  // switches.
  await swapBodySvg(packId, manifest?.files?.body ?? 'body.svg');

  // Apply theming. Clear previous pack's vars first so stale colors don't
  // bleed through if the new pack declares fewer parts.
  clearPartVars();
  const theme = mergeTheme(manifest, userThemes[packId]);
  applyThemeToRoot(theme);

  return { manifest, speech };
}

/** Apply just the user's theme overrides for the active pack — no DOM
 *  swap, no stylesheet reload. Safe to call from a color-picker drag
 *  handler many times per second.
 */
export async function applyPackTheme(packId, userThemes = {}) {
  const manifest = await getManifest(packId);
  clearPartVars();
  const theme = mergeTheme(manifest, userThemes[packId]);
  applyThemeToRoot(theme);
}

async function fetchJson(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`pack fetch ${url} -> ${res.status}`);
  return await res.json();
}

async function fetchText(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`pack fetch ${url} -> ${res.status}`);
  return await res.text();
}

function defaultSpeech() {
  return { activity: {}, idle: {}, timeOfDay: {}, pet: [], secret: [] };
}

async function swapPackStylesheet(packId, filename) {
  const href = `packs/${packId}/${filename}`;
  const existing = document.querySelector('link[data-pack-stylesheet]');
  if (existing) existing.remove();
  await new Promise((resolve) => {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = href;
    link.setAttribute('data-pack-stylesheet', packId);
    link.addEventListener('load', resolve, { once: true });
    // Don't block forever if the file 404s — behavior engine still runs,
    // just without pack-specific animations.
    link.addEventListener('error', resolve, { once: true });
    document.head.appendChild(link);
  });
}

async function swapBodySvg(packId, filename) {
  const container = document.getElementById('footer-mascot');
  if (!container) return;
  const svg = await fetchText(`packs/${packId}/${filename}`).catch(() => null);
  if (!svg) return;

  // Keep the speech bubble (if it's already a child) — destroying it would
  // drop any in-flight speech animation state.
  const speechEl = container.querySelector('#mascot-speech');
  // Remove everything that isn't the speech bubble.
  for (const child of [...container.children]) {
    if (child !== speechEl) child.remove();
  }

  // Parse the incoming SVG and insert it. setting innerHTML of an SVG
  // element doesn't always parse a root <svg> correctly across browsers —
  // using a template element sidesteps that.
  const template = document.createElement('template');
  template.innerHTML = svg.trim();
  container.append(...template.content.childNodes);
}
