import { describe, expect, it } from "vitest";
import { effectiveWeaponStrength } from "./weapon-handling";

describe("effective weapon Strength", () => {
  it.each([
    [false, false, false, 14],
    [false, true, false, 21],
    [true, false, false, 21],
    [true, true, false, 21],
    [false, false, true, 14],
    [false, true, true, 14],
    [true, false, true, 14],
    [true, true, true, 14],
  ])("user %s, forced %s, disabled %s gives %s", (user, forced, disabled, expected) => {
    expect(effectiveWeaponStrength(14, Boolean(user), {
      forcesTwoHanding: Boolean(forced), disablesTwoHandBonus: Boolean(disabled),
    })).toBe(expected);
  });

  it("preserves the native floor and values above 99", () => {
    const profile = { forcesTwoHanding: false, disablesTwoHandBonus: false };
    expect(effectiveWeaponStrength(15, true, profile)).toBe(22);
    expect(effectiveWeaponStrength(99, true, profile)).toBe(148);
  });

  it("does not guess handling when metadata is missing", () => {
    expect(effectiveWeaponStrength(14, true, null)).toBeNull();
  });
});
