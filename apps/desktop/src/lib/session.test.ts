import { describe, expect, it } from "vitest";

import { classMeta, derivedLevel, EIGHT_STAT_KEYS, normalizeOptimizeRequest, optimalStartingClass, scalingLetter, STARTING_CLASS_METADATA, startingClassLevel } from "./session";
import { defaultRequest } from "./state";

describe("request normalization properties", () => {
  it("rejects unknown starting classes instead of using Vagabond", () => {
    expect(() => classMeta(null, "Unknown Class")).toThrow("Unknown starting class");
  });

  it("derives level from all eight entered stats", () => {
    expect(derivedLevel(null, defaultRequest)).toBe(9);
    expect(derivedLevel(null, { ...defaultRequest, vig: 60 })).toBe(57);
    expect(derivedLevel(null, { ...defaultRequest, dex: 99 })).toBe(93);
  });

  it("finds the lowest-level starting class for the requested stat floors", () => {
    const targets = { vig: 40, mnd: 15, end: 20, strStat: 16, dex: 12, intStat: 70, fai: 0, arc: 0 };
    const optimal = optimalStartingClass(null, targets, "Samurai");
    expect(optimal.name).toBe("Astrologer");
    expect(startingClassLevel(optimal, targets)).toBe(110);
  });

  it("exposes the two 1.17 Tarnished Pack class stat lines", () => {
    expect(classMeta(null, "Idus Knight")).toMatchObject({ baseLevel: 7, baseStats: { dex: 15, arc: 6 } });
    expect(classMeta(null, "Heavy Knight")).toMatchObject({ baseLevel: 10, baseStats: { vig: 14, end: 17 } });
  });

  it("checks every class against the independent total-stats minus 79 level rule", () => {
    for (const targets of [defaultRequest, { ...defaultRequest, intStat: 70, mnd: 38 },
      { ...defaultRequest, strStat: 1, dex: 99, arc: 45 }]) {
      const levels = STARTING_CLASS_METADATA.map((entry) => {
        const level = EIGHT_STAT_KEYS.reduce((sum, key) => sum + Math.max(entry.baseStats[key], targets[key]), 0) - 79;
        expect(startingClassLevel(entry, targets)).toBe(level);
        return level;
      });
      expect(startingClassLevel(optimalStartingClass(null, targets, "Samurai"), targets)).toBe(Math.min(...levels));
    }
  });

  it("clamps legacy and current upgrade values for a structured numeric corpus", () => {
    const corpus = [-10_000, -1, -0.1, 0, 1.9, 10, 25, 26, 10_000, Number.NaN, Number.POSITIVE_INFINITY];
    for (const value of corpus) {
      const normalized = normalizeOptimizeRequest({
        ...defaultRequest,
        standardMaxUpgrade: value,
        somberMaxUpgrade: value,
      }, defaultRequest);
      expect(Number.isInteger(normalized.standardMaxUpgrade)).toBe(true);
      expect(normalized.standardMaxUpgrade).toBeGreaterThanOrEqual(0);
      expect(normalized.standardMaxUpgrade).toBeLessThanOrEqual(25);
      expect(Number.isInteger(normalized.somberMaxUpgrade)).toBe(true);
      expect(normalized.somberMaxUpgrade).toBeGreaterThanOrEqual(0);
      expect(normalized.somberMaxUpgrade).toBeLessThanOrEqual(10);
    }
  });

  it("is idempotent after legacy upgrade migration", () => {
    for (let value = -5; value <= 40; value += 1) {
      const migrated = normalizeOptimizeRequest({ ...defaultRequest, maxUpgrade: value }, defaultRequest);
      expect(normalizeOptimizeRequest(migrated, defaultRequest)).toEqual(migrated);
    }
  });

  it("uses Convergence S+ and S++ grades without changing Vanilla grades", () => {
    expect(scalingLetter(1.99, true)).toBe("S");
    expect(scalingLetter(2.0, true)).toBe("S+");
    expect(scalingLetter(2.249, true)).toBe("S+");
    expect(scalingLetter(2.25, true)).toBe("S++");
    expect(scalingLetter(2.277, true)).toBe("S++");
    expect(scalingLetter(2.277)).toBe("S");
  });
});
