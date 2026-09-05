import { describe, expect, it } from "vitest";
import { resolveAowSelection } from "./AowSelect";

const buckler = {
  canChangeAow: true,
  nativeSkillName: "Buckler Parry",
  compatibleAows: ["Buckler Parry", "Parry", "No Skill"],
};

describe("weapon skill selection", () => {
  it("defaults Buckler on weapon changes without overriding restored or explicit choices", () => {
    expect(resolveAowSelection(buckler, null, true)).toBe("Buckler Parry");
    expect(resolveAowSelection(buckler, "__match_selected__", true)).toBe("Buckler Parry");
    expect(resolveAowSelection(buckler, "No Skill", true)).toBe("No Skill");
    expect(resolveAowSelection(buckler, null, false)).toBeNull();
  });

  it("locks fixed weapons to their native skill, including Compare match mode", () => {
    const rivers = { canChangeAow: false, nativeSkillName: "Corpse Piler", compatibleAows: ["Corpse Piler"] };
    for (const value of [null, "No Skill", "__match_selected__", "Corpse Piler"]) {
      expect(resolveAowSelection(rivers, value, true)).toBe("Corpse Piler");
    }
  });

  it("never applies an incompatible native skill to an infusion", () => {
    expect(resolveAowSelection({ ...buckler, compatibleAows: ["No Skill"] }, null, true)).toBeNull();
    expect(resolveAowSelection({ ...buckler, compatibleAows: ["No Skill"] }, "Buckler Parry", false)).toBeNull();
  });
});
