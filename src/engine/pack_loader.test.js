import { describe, it, expect, beforeEach } from 'vitest';
import {
  mergeTheme,
  partCssVarName,
  themeToCssVars,
  PACK_IDS,
} from './pack_loader.js';

describe('pack_loader — pure helpers', () => {
  describe('partCssVarName', () => {
    it('prefixes with --part- and preserves the key as-is', () => {
      expect(partCssVarName('eyes')).toBe('--part-eyes');
      expect(partCssVarName('antenna-tip')).toBe('--part-antenna-tip');
    });

    it('lowercases and hyphenates camelCase keys for CSS compatibility', () => {
      expect(partCssVarName('antennaTip')).toBe('--part-antenna-tip');
    });
  });

  describe('mergeTheme', () => {
    const manifest = {
      id: 'pane',
      parts: [
        { key: 'body', label: 'Body', default: '#b0b0b0' },
        { key: 'eyes', label: 'Eyes', default: '#2a6df0' },
        { key: 'antennaTip', label: 'Antenna tip', default: '#ffcc00' },
      ],
    };

    it('returns defaults when no user overrides', () => {
      const theme = mergeTheme(manifest, {});
      expect(theme).toEqual({
        body: '#b0b0b0',
        eyes: '#2a6df0',
        antennaTip: '#ffcc00',
      });
    });

    it('applies user overrides on top of defaults', () => {
      const theme = mergeTheme(manifest, { eyes: '#ff0000' });
      expect(theme.eyes).toBe('#ff0000');
      expect(theme.body).toBe('#b0b0b0'); // untouched
      expect(theme.antennaTip).toBe('#ffcc00');
    });

    it('silently drops user overrides for parts the pack doesn\u2019t declare', () => {
      // Prevents stale settings (from an old pack version) from injecting
      // garbage CSS vars.
      const theme = mergeTheme(manifest, { eyes: '#ff0000', legs: '#00ff00' });
      expect(theme.eyes).toBe('#ff0000');
      expect(theme.legs).toBeUndefined();
    });

    it('handles missing parts array gracefully', () => {
      expect(mergeTheme({ id: 'nope' }, { eyes: '#f0f' })).toEqual({});
    });

    it('handles null/undefined overrides gracefully', () => {
      expect(mergeTheme(manifest, null).body).toBe('#b0b0b0');
      expect(mergeTheme(manifest, undefined).body).toBe('#b0b0b0');
    });
  });

  describe('themeToCssVars', () => {
    it('maps a merged theme object to --part-* CSS variable pairs', () => {
      const vars = themeToCssVars({ body: '#fff', antennaTip: '#f0f' });
      expect(vars).toEqual({
        '--part-body': '#fff',
        '--part-antenna-tip': '#f0f',
      });
    });

    it('empty theme produces empty vars object', () => {
      expect(themeToCssVars({})).toEqual({});
    });
  });

  describe('PACK_IDS', () => {
    it('includes the known packs for the gallery', () => {
      expect(PACK_IDS).toContain('pane');
      expect(PACK_IDS).toContain('sprite');
      expect(PACK_IDS).toContain('blob');
      expect(PACK_IDS).toContain('ghost');
      expect(PACK_IDS).toContain('cat');
    });
  });

  describe('per-pack theme isolation', () => {
    it('themes for different packs never cross-contaminate', () => {
      // The user's custom eyes color on "pane" should not leak into "sprite"
      // — each pack owns its own theme sub-object in settings.
      const paneManifest = {
        id: 'pane',
        parts: [{ key: 'eyes', default: '#000' }],
      };
      const spriteManifest = {
        id: 'sprite',
        parts: [{ key: 'eyes', default: '#fff' }],
      };
      const userThemes = {
        pane: { eyes: '#f00' },
        sprite: {},
      };
      expect(mergeTheme(paneManifest, userThemes.pane).eyes).toBe('#f00');
      expect(mergeTheme(spriteManifest, userThemes.sprite).eyes).toBe('#fff');
    });
  });
});
