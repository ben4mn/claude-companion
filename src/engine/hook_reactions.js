// Hook event + MCP tool → animation mapping.
//
// Pure: takes a normalized event/tool-call shape and returns the intended
// reaction ({ cls, speech, speechDuration }). behavior.js translates the
// reaction into DOM changes. Keeping the mapping pure means we can unit-test
// the decision table without a live Tauri runtime.

/** Map a Claude Code hook event (from the IPC server) to a Pane reaction.
 *  Returns `null` for events we don't care about — the caller treats null
 *  as "do nothing." */
export function reactionFor(event) {
  if (!event || typeof event.type !== 'string') return null;
  const type = event.type;
  const payload = event.payload ?? {};
  const tool = payload.tool;

  switch (type) {
    case 'PreToolUse':
      if (tool === 'Bash') {
        return { cls: 'act-type', speech: null };
      }
      // Everything else (Write, Edit, Read, Glob, Grep, unknown) — just think.
      return { cls: 'act-think', speech: null };

    case 'PostToolUse':
      if (tool === 'Write' || tool === 'Edit') {
        return { cls: 'act-nod', speech: null };
      }
      return null;

    case 'Notification':
      return {
        cls: 'act-think',
        speech: pickOne(['Everything okay?', 'Hmm.', 'Heads up.']),
      };

    case 'Stop':
      return { cls: 'act-wave', speech: pickOne(['Done!', 'All good.', 'Shipshape.']) };

    default:
      return null;
  }
}

/** Map an MCP tool call (from the companion-mcp binary) to a reaction. */
export function reactionForMcpTool(name, args = {}) {
  switch (name) {
    case 'companion_say':
      if (typeof args.text !== 'string' || !args.text.trim()) return null;
      // Auto-scale duration by length so long messages stay visible long
      // enough to read. Roughly 60 chars/sec + 2s grace, clamped to a
      // 2.5s floor and 12s ceiling. Explicit durationMs still wins.
      return {
        cls: null,
        speech: args.text,
        speechDuration: Number.isFinite(args.durationMs)
          ? args.durationMs
          : Math.min(12000, Math.max(2500, 2000 + args.text.length * 50)),
      };
    case 'companion_react': {
      const cls = EMOTION_TO_CLASS[args.emotion] ?? 'act-think';
      return { cls, speech: null };
    }
    case 'companion_show_status':
      // Reserved; acknowledged but no visible reaction for v1.
      return { cls: null, speech: null };
    default:
      return null;
  }
}

const EMOTION_TO_CLASS = {
  happy: 'act-bounce',
  celebrate: 'act-dance',
  think: 'act-think',
  confused: 'act-look',
  concerned: 'act-think',
  wave: 'act-wave',
};

function pickOne(arr) {
  return arr[Math.floor(Math.random() * arr.length)];
}
