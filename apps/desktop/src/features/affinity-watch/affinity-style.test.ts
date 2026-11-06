import { describe, expect, it } from "vitest";

import { AFFINITY_STROKE_WIDTH, AFFINITY_SWATCH_WIDTH, affinityLineStyle } from "./affinity-style";

// Vanilla weapons offer up to 13 affinities.
const LINES = Array.from({ length: 13 }, (_, index) => affinityLineStyle(index));
const dashes = LINES.flatMap(({ dash }) => (dash ? [dash.split(" ").map(Number)] : []));

describe("affinity line style", () => {
  it("gives each of 13 lines its own dash pattern", () => {
    expect(new Set(LINES.map((style) => style.dash)).size).toBe(13);
  });

  it("gives each of 13 lines its own colour and dash pair", () => {
    expect(new Set(LINES.map((style) => `${style.color}|${style.dash}`)).size).toBe(13);
  });

  it("separates neighbouring lines by colour and by dash", () => {
    for (let index = 1; index < LINES.length; index += 1) {
      expect(LINES[index].color).not.toBe(LINES[index - 1].color);
      expect(LINES[index].dash).not.toBe(LINES[index - 1].dash);
    }
  });

  it("leaves every gap visible after the round caps", () => {
    // Round caps extend each dash by half the stroke width at both ends.
    for (const pattern of dashes) {
      const gaps = pattern.filter((_, index) => index % 2 === 1);
      for (const gap of gaps) expect(gap - AFFINITY_STROKE_WIDTH).toBeGreaterThanOrEqual(3.5);
    }
  });

  it("fits one full period of every pattern in the legend swatch", () => {
    for (const pattern of dashes) {
      const period = pattern.reduce((sum, length) => sum + length, 0);
      expect(period).toBeLessThanOrEqual(AFFINITY_SWATCH_WIDTH - AFFINITY_STROKE_WIDTH);
    }
  });
});
