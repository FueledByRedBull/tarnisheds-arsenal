import { describe, expect, it } from "vitest";
import { explainBuild, explainBuildComparison } from "./build-explanation";
import { reproductionReport } from "./reproduction-report";
import { defaultRequest, useDesktopStore } from "./state";
import type { SolvedBuildDto } from "./types";

const row: SolvedBuildDto = {
  weaponId: 1, weaponName: "Uchigatana", affinity: "Keen", isSomber: false, upgrade: 7,
  stats: { strStat: 18, dex: 40, intStat: 9, fai: 8, arc: 8 },
  ar: { physical: 300, magic: 100, fire: 0, lightning: 0, holy: 0, total: 400 },
  aowId: 100, aowName: "Unsheathe", bleedBuildup: 45, bleedBuildupAdd: 0,
  frostBuildup: 0, poisonBuildup: 0, scarletRotBuildup: 0, sleepBuildup: 0,
  madnessBuildup: 0, deathBuildup: 0, aowFirstHitDamage: 300,
  aowFullSequenceDamage: 500, aowRoute: null, score: 400,
};

describe("calculation-derived explanations", () => {
  it("describes lexicographic bleed ranking without adding AR or inventing contribution estimates", () => {
    const text = explainBuild(row, { ...defaultRequest, objective: "max_ar_plus_bleed", lockDex: 40, minStr: 18 }).join(" ");
    expect(text).toContain("bleed buildup first (45.0), then AR (400.0)");
    expect(text).toContain("300.0 physical + 100.0 magic");
    expect(text).toContain("DEX locked at 40");
    expect(text).toContain("STR minimum 18");
    expect(text).toContain("equal displayed scores need not be exact ties");
  });
  it("explains redistribution even when total stat spend is unchanged", () => {
    const candidate = { ...row, stats: { ...row.stats, strStat: 16, dex: 42 }, bleedBuildup: 46 };
    const text = explainBuildComparison(row, candidate, { objective: "max_ar_plus_bleed" });
    expect(text).toContain("+1.0 bleed buildup");
    expect(text).toContain("STR -2, DEX +2");
    expect(text).toContain("Bleed ranks before AR");
  });
  it("does not describe unknown imported properties as damage", () => {
    const imported = { ...row, ar: { ...row.ar, privateMetric: 123 } };
    expect(explainBuild(imported, defaultRequest).join(" ")).not.toContain("privateMetric");
  });
});

describe("reproduction reports", () => {
  const state = () => ({ ...useDesktopStore.getState(), catalog: null,
    request: { ...defaultRequest, lockDex: 40 }, lockedStatMode: false,
    selected: row, resultsStale: false, error: null, catalogError: null });

  it("captures effective inputs and fresh results without copying arbitrary state or request fields", () => {
    const input = state();
    Object.assign(input.request, { personalPath: "private", password: "private" });
    Object.assign(input, { savedBuilds: "private", rawLogs: "private" });
    const text = reproductionReport(input);
    const report = JSON.parse(text);
    expect(report.request.lockDex).toBeNull();
    expect(report.results.selected.weaponName).toBe("Uchigatana");
    expect(report.results.selected.aowFullSequenceDamage).toBeNull();
    expect(text).not.toContain("private");
    expect(report.context).toContain("not a history");
  });
  it("projects nested structures instead of leaking extra imported fields", () => {
    const input = state();
    input.request.filters = { version: 1, entries: [{ dimension: "weapon_family", id: "weapon:1", mode: "include" }] };
    Object.assign(input.request.filters, { privateNote: "nested-sensitive-sentinel" });
    Object.assign(input.request.filters.entries[0], { privateNote: "nested-sensitive-sentinel" });
    input.selected = { ...row, stats: { ...row.stats }, ar: { ...row.ar } };
    Object.assign(input.selected.stats, { note: "nested-sensitive-sentinel" });
    Object.assign(input.selected.ar, { note: "nested-sensitive-sentinel" });
    const text = reproductionReport(input);
    expect(text).not.toContain("sentinel");
  });
  it.each([
    "Unable to open C:\\Users\\Alice Smith\\data.csv",
    "Unable to open \\\\machine\\private\\data.csv",
    "failed /home/alice/data.csv", "file:///Users/alice/data.csv",
    "contact alice@example.com", "Authorization: Bearer test", "token=test",
  ])("omits private-looking error text: %s", error => {
    const report = JSON.parse(reproductionReport({ ...state(), error }));
    expect(report.error).toMatch(/^\[Omitted:/);
  });
  it("retains useful ordinary errors but excludes stale selected results", () => {
    const report = JSON.parse(reproductionReport({ ...state(), resultsStale: true, error: "No legal weapons match the filters." }));
    expect(report.error).toBe("No legal weapons match the filters.");
    expect(report.results.selected).toBeNull();
    expect(report.results.stale).toBe(true);
  });
});
