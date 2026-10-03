import { describe, expect, it } from "vitest";
import { rankMovements, sortRanked } from "./ranking-view";
import type { SolvedBuildDto } from "./types";

function row(weaponName: string, ar: number, bleed: number, skill: number | null, affinity = "Standard"): SolvedBuildDto {
  return {
    weaponName, affinity, aowName: "Seppuku", ar: { total: ar, physical: ar }, bleedBuildup: bleed,
    aowRoute: skill === null ? null : {}, aowFullSequenceDamage: skill ?? 0, aowFirstHitDamage: skill ?? 0,
  } as unknown as SolvedBuildDto;
}

const names = (entries: Array<{ row: SolvedBuildDto; rank: number }>) => entries.map((entry) => `${entry.rank + 1}:${entry.row.weaponName}`);

describe("sortRanked", () => {
  const rows = [row("A", 600, 80, 900), row("B", 700, 60, null), row("C", 650, 80, 1200), row("D", 700, 50, 800)];

  it("keeps rank order, reversed on request", () => {
    expect(names(sortRanked(rows, "rank", false, "max_ar_plus_bleed", true))).toEqual(["1:A", "2:B", "3:C", "4:D"]);
    expect(names(sortRanked(rows, "rank", true, "max_ar_plus_bleed", true))).toEqual(["4:D", "3:C", "2:B", "1:A"]);
  });

  it("sorts values high first and breaks ties by rank", () => {
    expect(names(sortRanked(rows, "ar", false, "max_ar_plus_bleed", true))).toEqual(["2:B", "4:D", "3:C", "1:A"]);
    expect(names(sortRanked(rows, "ar", true, "max_ar_plus_bleed", true))).toEqual(["1:A", "3:C", "2:B", "4:D"]);
    expect(names(sortRanked(rows, "score", false, "max_ar_plus_bleed", true))).toEqual(["1:A", "3:C", "2:B", "4:D"]);
  });

  it("puts unavailable values last in either direction", () => {
    expect(names(sortRanked(rows, "skill", false, "max_ar", true))).toEqual(["3:C", "1:A", "4:D", "2:B"]);
    expect(names(sortRanked(rows, "skill", true, "max_ar", true))).toEqual(["4:D", "1:A", "3:C", "2:B"]);
    expect(names(sortRanked(rows, "skill", false, "max_ar", false))).toEqual(["1:A", "2:B", "3:C", "4:D"]);
  });
});

describe("rankMovements", () => {
  const before = [row("A", 600, 0, null), row("B", 700, 0, null), row("C", 650, 0, null, "Keen"), row("C", 640, 0, null, "Keen")];

  it("reports places gained, new loadouts and the objective change", () => {
    const after = [row("B", 710, 0, null), row("C", 660, 0, null, "Keen"), row("A", 600, 0, null), row("C", 500, 0, null, "Blood")];
    expect(rankMovements(after, { objective: "max_ar", rows: before }, "max_ar", true)).toEqual([
      { places: 1, metricDelta: 10 },
      { places: 1, metricDelta: 10 },
      { places: -2, metricDelta: 0 },
      { places: null, metricDelta: null },
    ]);
  });

  it("reports no movement when the same loadout is ranked again at each upgrade", () => {
    const at = (upgrade: number, ar: number) => ({ ...row("A", ar, 0, null), upgrade });
    const ranked = [at(3, 300), at(2, 200), at(1, 100)];
    expect(rankMovements(ranked, { objective: "max_ar", rows: ranked }, "max_ar", true))
      .toEqual(ranked.map(() => ({ places: 0, metricDelta: 0 })));
    // A loadout ranked once in both searches is still followed across an upgrade change.
    expect(rankMovements([at(4, 400)], { objective: "max_ar", rows: [at(3, 300)] }, "max_ar", true))
      .toEqual([{ places: 0, metricDelta: 100 }]);
  });

  it("omits value changes when the objective changed and has no baseline before a first search", () => {
    const after = [row("A", 600, 0, null)];
    expect(rankMovements(after, { objective: "max_ar_plus_bleed", rows: before }, "max_ar", true)).toEqual([{ places: 0, metricDelta: null }]);
    expect(rankMovements(after, null, "max_ar", true)).toBeNull();
  });
});
