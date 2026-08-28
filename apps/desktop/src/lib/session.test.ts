import { describe, expect, it } from "vitest";

import { classMeta, derivedLevel, normalizeOptimizeRequest, scalingLetter } from "./session";
import { defaultRequest } from "./state";

describe("request normalization properties", () => {
  it("rejects unknown starting classes instead of using Vagabond", () => {
    expect(() => classMeta(null, "Unknown Class")).toThrow("Unknown starting class");
  });

  it("keeps target level explicit and derives offensive-point level from fixed utility stats", () => {
    expect(derivedLevel(null, { ...defaultRequest, characterLevel: 150, vig: 60 })).toBe(150);
    expect(derivedLevel(null, {
      ...defaultRequest,
      budgetMode: "offensive_points",
      offensivePointBudget: 40,
      vig: 20,
    })).toBe(57);
  });

  it("exposes the two 1.17 Tarnished Pack class stat lines", () => {
    expect(classMeta(null, "Idus Knight")).toMatchObject({ baseLevel: 7, baseStats: { dex: 15, arc: 6 } });
    expect(classMeta(null, "Heavy Knight")).toMatchObject({ baseLevel: 10, baseStats: { vig: 14, end: 17 } });
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
