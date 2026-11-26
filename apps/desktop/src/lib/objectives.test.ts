import { describe, expect, it } from "vitest";

import { metricForObjective, objectiveLabel, objectiveMissingData, objectiveValue, supportedObjectives } from "./objectives";
import { solvedBuild } from "./test-fixtures";
import { OBJECTIVE_IDS, type AowRouteDto, type ProfileCapabilitiesDto } from "./types";

const full: ProfileCapabilitiesDto = { classBudget: true, statusBuildup: true, aowDamage: true, aowRoutes: true };
// Only whether a row has a route matters here.
const route = { routeId: "light" } as AowRouteDto;

describe("objective vocabulary", () => {
  it("labels every objective", () => {
    expect(OBJECTIVE_IDS.map(objectiveLabel)).toEqual(["Max AR", "Max Physical AR", "Bleed, then AR", "AoW First Hit", "AoW Sequence"]);
  });

  it("offers each objective only when the profile has its data", () => {
    expect(supportedObjectives(full)).toEqual([...OBJECTIVE_IDS]);
    expect(supportedObjectives({ ...full, statusBuildup: false })).not.toContain("max_ar_plus_bleed");
    expect(supportedObjectives({ ...full, aowRoutes: false })).toEqual(["max_ar", "max_physical_ar", "max_ar_plus_bleed", "aow_first_hit"]);
    expect(supportedObjectives({ classBudget: false, statusBuildup: false, aowDamage: false, aowRoutes: false })).toEqual(["max_ar", "max_physical_ar"]);
  });

  it("names the missing data as the core reports it", () => {
    expect(objectiveMissingData("max_ar", { ...full, aowDamage: false })).toBeNull();
    expect(objectiveMissingData("max_ar_plus_bleed", { ...full, statusBuildup: false })).toBe("status-buildup data");
    expect(objectiveMissingData("aow_first_hit", { ...full, aowDamage: false })).toBe("verified Ash of War damage data");
    expect(objectiveMissingData("aow_full_sequence", { ...full, aowDamage: false })).toBe("verified Ash of War route data");
    expect(objectiveMissingData("aow_full_sequence", { ...full, aowRoutes: false })).toBe("verified Ash of War route data");
  });

  it("reads each objective's build field and hides unmodeled skill damage", () => {
    const row = solvedBuild({ aowFullSequenceDamage: 600 });
    expect(OBJECTIVE_IDS.map((objective) => objectiveValue(row, objective))).toEqual([500, 500, 45, 300, 600]);
    expect(metricForObjective(row, "aow_first_hit", true)).toBeNull();
    const routed = { ...row, aowRoute: route };
    expect(metricForObjective(routed, "aow_first_hit", false)).toBeNull();
    expect(metricForObjective(routed, "aow_full_sequence", true)).toBe(600);
    expect(metricForObjective(row, "max_ar_plus_bleed", false)).toBe(45);
  });
});
