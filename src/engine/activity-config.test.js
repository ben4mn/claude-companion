import { describe, it, expect } from 'vitest';
import {
  ACTIVITIES,
  PRESETS,
  resolvePresetConfig,
  filterActivities,
  scaleDelay,
  isInQuietHours,
  quietHoursPool,
  shouldSpeak,
  pickActivity,
} from './activity-config.js';

describe('activity-config — pure helpers', () => {
  describe('ACTIVITIES catalogue', () => {
    it('contains the core roster with stable shape', () => {
      const names = ACTIVITIES.map((a) => a.name);
      for (const expected of ['stand', 'sleep', 'wave', 'dance', 'type']) {
        expect(names).toContain(expected);
      }
      for (const a of ACTIVITIES) {
        expect(typeof a.name).toBe('string');
        expect(typeof a.cls).toBe('string');
        expect(a.min).toBeGreaterThan(0);
        expect(a.max).toBeGreaterThanOrEqual(a.min);
      }
    });
  });

  describe('PRESETS', () => {
    it('defines calm, normal, playful', () => {
      expect(PRESETS).toHaveProperty('calm');
      expect(PRESETS).toHaveProperty('normal');
      expect(PRESETS).toHaveProperty('playful');
    });

    it('calm is lower frequency + lower chattiness than playful', () => {
      expect(PRESETS.calm.activityFrequency).toBeLessThan(PRESETS.playful.activityFrequency);
      expect(PRESETS.calm.speechChattiness).toBeLessThan(PRESETS.playful.speechChattiness);
    });

    it('normal preset matches DEFAULTS for frequency and walkSpeed', () => {
      expect(PRESETS.normal.activityFrequency).toBe(1.0);
      expect(PRESETS.normal.walkSpeed).toBe(1.0);
    });
  });

  describe('resolvePresetConfig', () => {
    it('preset "calm" returns the calm preset values', () => {
      const cfg = resolvePresetConfig({ preset: 'calm' });
      expect(cfg.activityFrequency).toBe(PRESETS.calm.activityFrequency);
      expect(cfg.activityPool).toEqual(PRESETS.calm.activityPool);
    });

    it('preset "normal" returns the normal preset values', () => {
      const cfg = resolvePresetConfig({ preset: 'normal' });
      expect(cfg.activityFrequency).toBe(1.0);
      expect(cfg.activityPool).toEqual(PRESETS.normal.activityPool);
    });

    it('preset "custom" uses raw values from the input config', () => {
      const cfg = resolvePresetConfig({
        preset: 'custom',
        activityFrequency: 0.3,
        walkSpeed: 1.7,
        speechChattiness: 0.9,
        activityPool: ['stand', 'sleep'],
      });
      expect(cfg.activityFrequency).toBe(0.3);
      expect(cfg.walkSpeed).toBe(1.7);
      expect(cfg.speechChattiness).toBe(0.9);
      expect(cfg.activityPool).toEqual(['stand', 'sleep']);
    });

    it('unknown preset name falls back to normal rather than crashing', () => {
      const cfg = resolvePresetConfig({ preset: 'bogus' });
      expect(cfg.activityFrequency).toBe(1.0);
    });
  });

  describe('filterActivities', () => {
    it('returns full roster when pool is null/undefined', () => {
      expect(filterActivities(ACTIVITIES, null).length).toBe(ACTIVITIES.length);
      expect(filterActivities(ACTIVITIES, undefined).length).toBe(ACTIVITIES.length);
    });

    it('filters activities by name whitelist', () => {
      const filtered = filterActivities(ACTIVITIES, ['stand', 'sleep']);
      expect(filtered.map((a) => a.name).sort()).toEqual(['sleep', 'stand']);
    });

    it('drops pool entries that aren\u2019t in the catalogue', () => {
      const filtered = filterActivities(ACTIVITIES, ['stand', 'not-a-real-activity']);
      expect(filtered.map((a) => a.name)).toEqual(['stand']);
    });

    it('returns empty array when pool is empty', () => {
      expect(filterActivities(ACTIVITIES, [])).toEqual([]);
    });
  });

  describe('scaleDelay', () => {
    it('multiplier of 1.0 returns the base delay unchanged', () => {
      expect(scaleDelay(3000, 1.0)).toBe(3000);
    });

    it('multiplier below 1.0 (less frequent) lengthens the delay', () => {
      // frequency=0.5 means half as often, so delay doubles.
      expect(scaleDelay(3000, 0.5)).toBeGreaterThan(3000);
      expect(scaleDelay(3000, 0.5)).toBe(6000);
    });

    it('multiplier above 1.0 (more frequent) shortens the delay', () => {
      // frequency=2.0 means twice as often, so delay halves.
      expect(scaleDelay(3000, 2.0)).toBeLessThan(3000);
      expect(scaleDelay(3000, 2.0)).toBe(1500);
    });

    it('a zero or negative multiplier clamps to a small positive baseline', () => {
      // We never want a 0ms setTimeout loop — it would spin the event loop.
      expect(scaleDelay(3000, 0)).toBeGreaterThan(0);
      expect(scaleDelay(3000, -1)).toBeGreaterThan(0);
    });
  });

  describe('isInQuietHours', () => {
    const mkDate = (h, m = 0) => {
      const d = new Date();
      d.setHours(h, m, 0, 0);
      return d;
    };

    it('returns false when quiet hours are disabled', () => {
      const qh = { enabled: false, from: '22:00', to: '07:00' };
      expect(isInQuietHours(mkDate(23), qh)).toBe(false);
    });

    it('detects same-day quiet window (14:00 to 17:00)', () => {
      const qh = { enabled: true, from: '14:00', to: '17:00' };
      expect(isInQuietHours(mkDate(13, 59), qh)).toBe(false);
      expect(isInQuietHours(mkDate(14), qh)).toBe(true);
      expect(isInQuietHours(mkDate(16, 30), qh)).toBe(true);
      expect(isInQuietHours(mkDate(17), qh)).toBe(false);
    });

    it('detects overnight quiet window (22:00 to 07:00)', () => {
      const qh = { enabled: true, from: '22:00', to: '07:00' };
      expect(isInQuietHours(mkDate(21, 59), qh)).toBe(false);
      expect(isInQuietHours(mkDate(22), qh)).toBe(true);
      expect(isInQuietHours(mkDate(2), qh)).toBe(true);
      expect(isInQuietHours(mkDate(6, 59), qh)).toBe(true);
      expect(isInQuietHours(mkDate(7), qh)).toBe(false);
    });
  });

  describe('quietHoursPool', () => {
    it('restricts activities to calm subset (stand, sleep) during quiet hours', () => {
      const pool = quietHoursPool(ACTIVITIES);
      const names = pool.map((a) => a.name);
      expect(names).toContain('stand');
      expect(names).toContain('sleep');
      expect(names).not.toContain('dance');
      expect(names).not.toContain('shimmy');
    });
  });

  describe('shouldSpeak', () => {
    it('returns false when chattiness is 0 regardless of rng', () => {
      expect(shouldSpeak(0, () => 0)).toBe(false);
      expect(shouldSpeak(0, () => 0.99)).toBe(false);
    });

    it('returns true when chattiness is 1 regardless of rng', () => {
      expect(shouldSpeak(1, () => 0.99)).toBe(true);
    });

    it('compares chattiness against the supplied rng', () => {
      // rng returns 0.4. chattiness 0.5 → 0.4 < 0.5 → speak. chattiness 0.3 → quiet.
      expect(shouldSpeak(0.5, () => 0.4)).toBe(true);
      expect(shouldSpeak(0.3, () => 0.4)).toBe(false);
    });
  });

  describe('pickActivity', () => {
    it('only returns activities from the filtered pool', () => {
      const cfg = { preset: 'custom', activityPool: ['stand', 'sleep'], speechChattiness: 0 };
      const rng = () => 0;
      for (let i = 0; i < 10; i++) {
        const act = pickActivity(ACTIVITIES, cfg, new Date(2026, 0, 1, 12), rng);
        expect(['stand', 'sleep']).toContain(act.name);
      }
    });

    it('during quiet hours, restricts to the quiet-hours pool even if config pool is wider', () => {
      const cfg = {
        preset: 'custom',
        activityPool: null, // full roster allowed
        quietHours: { enabled: true, from: '22:00', to: '07:00' },
      };
      const during = new Date();
      during.setHours(23, 0, 0, 0);
      for (let i = 0; i < 10; i++) {
        const act = pickActivity(ACTIVITIES, cfg, during, () => Math.random());
        // Quiet hours should never pick high-energy activities.
        expect(['dance', 'shimmy', 'bounce']).not.toContain(act.name);
      }
    });
  });
});
