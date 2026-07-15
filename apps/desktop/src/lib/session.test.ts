import { describe, expect, it } from "vitest";

import { normalizeOptimizeRequest } from "./session";
import { defaultRequest } from "./state";

describe("request normalization properties", () => {
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
});
