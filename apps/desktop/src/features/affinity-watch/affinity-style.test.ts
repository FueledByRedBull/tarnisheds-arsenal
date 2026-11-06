import { describe, expect, it } from "vitest";

import { affinityLineStyle } from "./affinity-style";

// Vanilla weapons offer up to 13 affinities.
const LINES = Array.from({ length: 13 }, (_, index) => affinityLineStyle(index));

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
});
