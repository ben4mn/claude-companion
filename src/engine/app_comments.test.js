import { describe, it, expect } from 'vitest';
import {
  shouldCommentOnApp,
  pickCommentFor,
} from './app_comments.js';

const COMMENTS = {
  'com.apple.Safari': ['Browsing, eh?', 'Research or distraction?'],
  'com.google.Chrome': ['Another tab?'],
  'com.microsoft.VSCode': ['Back to the editor.'],
};

describe('app_comments.shouldCommentOnApp', () => {
  const baseState = {
    bundleId: 'com.apple.Safari',
    mode: 'desktop',
    enabled: true,
    allowlist: ['com.apple.Safari', 'com.google.Chrome'],
    frequencyMs: 45000,
    lastCommentAt: 0,
    now: 100000,
    engaged: false, // Pane is grounded-idle, not HELD/FALLING
    commentsMap: COMMENTS,
  };

  it('returns true for an allowlisted bundle in desktop mode, idle Pane', () => {
    expect(shouldCommentOnApp(baseState)).toBe(true);
  });

  it('returns false in claudeOnly mode', () => {
    expect(shouldCommentOnApp({ ...baseState, mode: 'claudeOnly' })).toBe(false);
  });

  it('returns false when disabled in settings', () => {
    expect(shouldCommentOnApp({ ...baseState, enabled: false })).toBe(false);
  });

  it('returns false for non-allowlisted bundle', () => {
    expect(shouldCommentOnApp({ ...baseState, bundleId: 'com.random.App' })).toBe(false);
  });

  it('returns false when no comments exist for the bundle', () => {
    expect(shouldCommentOnApp({ ...baseState, bundleId: 'com.apple.NoComments',
      allowlist: ['com.apple.NoComments'] })).toBe(false);
  });

  it('returns false within frequency window since last comment', () => {
    // last comment 10s ago, frequency 45s → too soon.
    expect(shouldCommentOnApp({ ...baseState, lastCommentAt: 90000 })).toBe(false);
  });

  it('returns true after frequency window elapses', () => {
    // last 60s ago, frequency 45s → allowed.
    expect(shouldCommentOnApp({ ...baseState, lastCommentAt: 40000 })).toBe(true);
  });

  it('returns false when Pane is engaged (HELD/FALLING)', () => {
    expect(shouldCommentOnApp({ ...baseState, engaged: true })).toBe(false);
  });

  it('returns false on missing or nonstring bundleId', () => {
    expect(shouldCommentOnApp({ ...baseState, bundleId: null })).toBe(false);
    expect(shouldCommentOnApp({ ...baseState, bundleId: '' })).toBe(false);
  });
});

describe('app_comments.pickCommentFor', () => {
  it('returns null for unknown bundle', () => {
    expect(pickCommentFor('com.unknown.App', COMMENTS)).toBeNull();
  });

  it('returns one of the comments for a known bundle', () => {
    const comment = pickCommentFor('com.apple.Safari', COMMENTS);
    expect(COMMENTS['com.apple.Safari']).toContain(comment);
  });

  it('uses the injected rng deterministically', () => {
    // rng=0 → first item; rng=0.99 → last item.
    const first = pickCommentFor('com.apple.Safari', COMMENTS, () => 0);
    const last = pickCommentFor('com.apple.Safari', COMMENTS, () => 0.99);
    expect(first).toBe('Browsing, eh?');
    expect(last).toBe('Research or distraction?');
  });

  it('returns null when the comment list is empty', () => {
    expect(pickCommentFor('com.apple.Empty', { 'com.apple.Empty': [] })).toBeNull();
  });
});
