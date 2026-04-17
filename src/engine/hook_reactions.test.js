import { describe, it, expect } from 'vitest';
import { reactionFor, reactionForMcpTool } from './hook_reactions.js';

describe('hook_reactions — Claude Code hook event mapping', () => {
  it('PreToolUse with Bash → typing animation', () => {
    const r = reactionFor({ type: 'PreToolUse', payload: { tool: 'Bash' } });
    expect(r).toBeTruthy();
    expect(r.cls).toBe('act-type');
  });

  it('PreToolUse with Write/Edit → thinking animation', () => {
    const r = reactionFor({ type: 'PreToolUse', payload: { tool: 'Write' } });
    expect(r.cls).toBe('act-think');
  });

  it('PostToolUse with Write/Edit → nod animation', () => {
    const r = reactionFor({ type: 'PostToolUse', payload: { tool: 'Edit' } });
    expect(r.cls).toBe('act-nod');
  });

  it('Notification → concerned animation + speech', () => {
    const r = reactionFor({ type: 'Notification', payload: {} });
    expect(r.cls).toBeTruthy();
    expect(r.speech).toBeTruthy();
  });

  it('Stop → wave goodbye', () => {
    const r = reactionFor({ type: 'Stop', payload: null });
    expect(r.cls).toBe('act-wave');
  });

  it('unknown event type → null (silent)', () => {
    expect(reactionFor({ type: 'MysteryEvent', payload: null })).toBeNull();
  });

  it('unknown tool in PreToolUse → generic think', () => {
    const r = reactionFor({ type: 'PreToolUse', payload: { tool: 'MysteryTool' } });
    expect(r.cls).toBe('act-think');
  });

  it('missing payload on PreToolUse → generic think, no crash', () => {
    const r = reactionFor({ type: 'PreToolUse' });
    expect(r.cls).toBe('act-think');
  });
});

describe('hook_reactions — MCP tool mapping', () => {
  it('companion_say → speech only, no animation class', () => {
    const r = reactionForMcpTool('companion_say', { text: 'hello' });
    expect(r.speech).toBe('hello');
  });

  it('companion_say honors durationMs', () => {
    const r = reactionForMcpTool('companion_say', { text: 'hi', durationMs: 5000 });
    expect(r.speechDuration).toBe(5000);
  });

  it('companion_react maps each emotion to an animation', () => {
    for (const emo of ['happy', 'celebrate', 'think', 'confused', 'concerned', 'wave']) {
      const r = reactionForMcpTool('companion_react', { emotion: emo });
      expect(r).toBeTruthy();
      expect(r.cls).toMatch(/^act-/);
    }
  });

  it('unknown MCP tool → null', () => {
    expect(reactionForMcpTool('not_a_tool', {})).toBeNull();
  });

  it('unknown emotion falls back to a safe default', () => {
    const r = reactionForMcpTool('companion_react', { emotion: 'bewildered' });
    expect(r).toBeTruthy();
    expect(r.cls).toMatch(/^act-/);
  });
});
