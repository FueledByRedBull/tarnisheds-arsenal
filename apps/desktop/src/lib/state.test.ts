import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultRequest, useDesktopStore } from "./state";
import { buildOptimizeRequest, normalizeOptimizeRequest } from "./session";
import type { CatalogDto, SolvedBuildDto } from "./types";

const row: SolvedBuildDto = {
  weaponId: 1,
  weaponName: "Uchigatana",
  affinity: "Keen",
  isSomber: false,
  upgrade: 25,
  stats: { strStat: 18, dex: 40, intStat: 9, fai: 8, arc: 8 },
  ar: { physical: 500, magic: 0, fire: 0, lightning: 0, holy: 0, total: 500 },
  aowId: 100,
  aowName: "Unsheathe",
  bleedBuildup: 45,
  bleedBuildupAdd: 0,
  frostBuildup: 0,
  poisonBuildup: 0,
  scarletRotBuildup: 0,
  sleepBuildup: 0,
  madnessBuildup: 0,
  deathBuildup: 0,
  aowFirstHitDamage: 300,
  aowFullSequenceDamage: 300,
  aowRoute: null,
  score: 500,
};

describe("desktop result lifecycle", () => {
  beforeEach(() => {
    useDesktopStore.setState({
      catalog: null,
      catalogStatus: "loading",
      catalogError: null,
      request: { ...defaultRequest },
      rows: [],
      resultsStale: false,
      selected: null,
      compareTarget: null,
      selectedFingerprint: null,
      paths: [],
      pathSignature: null,
      affinityPayload: null,
      affinitySignature: null,
      notices: [],
      isSearching: false,
    });
  });

  it("retains previous rows and labels them stale when result inputs change", () => {
    const state = useDesktopStore.getState();
    state.setRows([row]);
    state.setCompareTarget(row);
    state.patchRequest({ objective: "max_physical_ar" });

    const changed = useDesktopStore.getState();
    expect(changed.rows).toEqual([row]);
    expect(changed.selected).toEqual(row);
    expect(changed.compareTarget).toBeNull();
    expect(changed.resultsStale).toBe(true);
  });

  it("marks replacement rows current after a successful search", () => {
    const state = useDesktopStore.getState();
    state.setRows([row]);
    state.patchRequest({ twoHanding: true });
    expect(useDesktopStore.getState().resultsStale).toBe(true);

    useDesktopStore.getState().setRows([{ ...row, score: 510 }]);
    expect(useDesktopStore.getState().resultsStale).toBe(false);
  });

  it("keeps multi-filters composable and clears pins for a custom comparison", () => {
    const state = useDesktopStore.getState();
    state.patchRequest({ weaponName: "Uchigatana", affinity: "Keen" });
    state.patchRequest({
      filters: { version: 1, entries: [{ dimension: "weapon_type", id: "weapon-type:katana", mode: "include" }] },
    });
    expect(useDesktopStore.getState().request).toMatchObject({
      weaponTypeKey: null,
      weaponName: "Uchigatana",
      affinity: "Keen",
      aowName: null,
    });

    useDesktopStore.getState().patchRequest({ weaponName: "Uchigatana" });
    expect(useDesktopStore.getState().request.filters.entries).toHaveLength(1);

    useDesktopStore.setState({ selected: row, compareBench: [row] });
    useDesktopStore.getState().patchCompareControls({
      filters: { version: 1, entries: [{ dimension: "weapon_type", id: "weapon-type:katana", mode: "include" }] },
    });
    expect(useDesktopStore.getState().compareBench).toEqual([]);
    expect(useDesktopStore.getState().selected).toEqual(row);

    useDesktopStore.getState().toggleCompareBench(row);
    expect(useDesktopStore.getState()).toMatchObject({
      compareBench: [row],
      compareControls: {
        filters: { version: 1, entries: [] },
        weaponName: null,
        aowName: null,
        matchSelectedAow: true,
        includeSmithing: true,
        includeSomber: true,
      },
      selected: row,
    });
  });

  it("records recoverable catalog loading failures", () => {
    const state = useDesktopStore.getState();
    state.setCatalogFailure("manifest checksum mismatch");
    expect(useDesktopStore.getState()).toMatchObject({
      catalogStatus: "error",
      catalogError: "manifest checksum mismatch",
    });

    useDesktopStore.getState().setCatalogLoading();
    expect(useDesktopStore.getState()).toMatchObject({
      catalogStatus: "loading",
      catalogError: null,
    });
  });

  it("restores pinned rows only for the same profile, schema, dataset and model", () => {
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    });
    try {
      const profile = catalog("vanilla");
      useDesktopStore.setState({ compareBench: [] });
      useDesktopStore.getState().setCatalog(profile);
      useDesktopStore.getState().toggleCompareBench(row);
      useDesktopStore.getState().setCatalog(structuredClone(profile));
      expect(useDesktopStore.getState().compareBench).toEqual([row]);
      for (const change of [{ schemaVersion: 5 }, { datasetVersion: "next" }, { modelVersion: "next" }]) {
        useDesktopStore.getState().setCatalog({ ...profile, dataManifest: { ...profile.dataManifest, ...change } });
        expect(useDesktopStore.getState().compareBench).toEqual([]);
      }
      useDesktopStore.getState().setCatalog(catalog("convergence"));
      expect(useDesktopStore.getState().compareBench).toEqual([]);
      values.set("tarnisheds-arsenal.compareBench.v1.vanilla", JSON.stringify({
        version: 1, datasetVersion: profile.dataManifest.datasetVersion, rows: [row],
      }));
      useDesktopStore.getState().setCatalog(profile);
      expect(useDesktopStore.getState().compareBench).toEqual([]);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("keeps Paths and Affinity Watch horizons independent", () => {
    const state = useDesktopStore.getState();
    state.setPathHorizon(25);
    state.setAffinityHorizon(80);

    expect(useDesktopStore.getState().pathHorizon).toBe(25);
    expect(useDesktopStore.getState().affinityHorizon).toBe(80);

    useDesktopStore.getState().setPathHorizon(12);
    expect(useDesktopStore.getState().affinityHorizon).toBe(80);
  });

  it("switches profiles as one fail-closed state transition", () => {
    const state = useDesktopStore.getState();
    state.setProfiles([catalog("vanilla").dataManifest, catalog("convergence").dataManifest]);
    state.setRows([row]);
    state.setCompareTarget(row);
    state.setWorkspace("compare");
    useDesktopStore.setState({ lockedStatMode: true, request: {
      ...useDesktopStore.getState().request,
      lockStr: 96, lockDex: 15, lockInt: 9, lockFai: 8, lockArc: 8,
    } });
    const before = useDesktopStore.getState();

    before.beginProfileSwitch("convergence");
    const switched = useDesktopStore.getState();

    expect(switched.request.profileId).toBe("convergence");
    expect(switched.lockedStatMode).toBe(false);
    expect(switched.request).toMatchObject({ lockStr: null, lockDex: null, lockInt: null, lockFai: null, lockArc: null });
    expect(switched.request.standardMaxUpgrade).toBe(15);
    expect(switched.request.somberMaxUpgrade).toBe(15);
    expect(switched.request.dlcScaling).toBe(false);
    expect(switched.request.scadutreeLevel).toBe(0);
    expect(switched.activeWorkspace).toBe("rankings");
    expect(switched.catalogStatus).toBe("loading");
    expect(switched.rows).toEqual([]);
    expect(switched.selected).toBeNull();
    expect(switched.compareTarget).toBeNull();
    expect(switched.searchGeneration).toBe(before.searchGeneration + 1);
    expect(switched.pathGeneration).toBe(before.pathGeneration + 1);
    expect(switched.affinityGeneration).toBe(before.affinityGeneration + 1);
  });

  it("normalizes unsupported objectives when a profile catalog arrives", () => {
    useDesktopStore.setState({
      request: { ...defaultRequest, profileId: "convergence", objective: "aow_full_sequence" },
    });

    useDesktopStore.getState().setCatalog(catalog("convergence"));

    expect(useDesktopStore.getState().request).toMatchObject({
      profileId: "convergence",
      objective: "max_ar",
    });
  });

  it("rejects unavailable Convergence controls in every request patch", () => {
    useDesktopStore.getState().setCatalog(catalog("convergence"));
    useDesktopStore.getState().patchRequest({
      standardMaxUpgrade: 25,
      somberMaxUpgrade: 25,
      dlcScaling: true,
      scadutreeLevel: 20,
      somberFilter: "somber_only",
    });

    expect(useDesktopStore.getState().request).toMatchObject({
      standardMaxUpgrade: 15,
      somberMaxUpgrade: 15,
      dlcScaling: false,
      scadutreeLevel: 0,
      somberFilter: "all",
    });
  });

  it("uses exact entered stats and retains +15 caps for a profile without class budgets", () => {
    const profile = catalog("convergence");
    profile.dataManifest.capabilities.classBudget = false;
    profile.classes = [{ name: "Custom stats", baseLevel: 0, baseTotal: 0,
      baseStats: { vig: 0, mnd: 0, end: 0, strStat: 0, dex: 0, intStat: 0, fai: 0, arc: 0 } }];
    useDesktopStore.getState().setCatalog(profile);
    const request = { ...useDesktopStore.getState().request, strStat: 1, dex: 99 };
    const fixed = buildOptimizeRequest(profile, request, false);
    expect(fixed).toMatchObject({ className: "Custom stats", strStat: 1, dex: 99, lockStr: 1, lockDex: 99,
      lockInt: request.intStat, lockFai: request.fai, lockArc: request.arc });
    expect(fixed.characterLevel).toBe(request.vig + request.mnd + request.end + 1 + 99 + request.intStat + request.fai + request.arc);
    useDesktopStore.getState().setWorkspace("paths");
    expect(useDesktopStore.getState().activeWorkspace).toBe("rankings");
    expect(normalizeOptimizeRequest({ ...request, somberMaxUpgrade: 15 }, request, profile.dataManifest.rules).somberMaxUpgrade).toBe(15);
    useDesktopStore.getState().loadBuildPreset({ version: 2, id: "old-profile", name: "Old profile inputs",
      profileId: "convergence", request: { ...request, className: "Samurai", somberMaxUpgrade: 15 },
      selectedBuild: null, compareTarget: null, compareBench: [], dataVersion: "old",
      createdAt: "2026-01-01", updatedAt: "2026-01-01" });
    expect(useDesktopStore.getState().request).toMatchObject({ className: "Custom stats", strStat: 1, dex: 99, somberMaxUpgrade: 15 });
  });
});

function catalog(profileId: string): CatalogDto {
  return {
    weaponCount: 1,
    aowCount: 1,
    weaponNames: ["Uchigatana"],
    weaponTypeKeys: ["katana"],
    classes: [],
    weaponTypeOptions: [{ key: "katana", label: "Katana" }],
    aowNames: ["Unsheathe"],
    affinityNames: ["Standard"],
    objectiveIds: ["max_ar", "max_physical_ar", "max_ar_plus_bleed"],
    somberFilters: ["all"],
    filterDimensions: [],
    dataManifest: {
      schemaVersion: 4,
      datasetVersion: `${profileId}-test`,
      modelVersion: "test-model",
      id: `${profileId}-test`,
      label: profileId,
      appVersion: "1.16.1",
      source: "test",
      generatedAt: "2026-07-16",
      extractorVersion: "test",
      provenance: "test",
      profile: {
        id: profileId,
        displayName: profileId,
        gameVersion: "1.16.1",
        modVersion: profileId === "convergence" ? "test" : null,
      },
      capabilities: {
        classBudget: true,
        weaponArForAmmunition: true,
        weaponAr: true,
        statusBuildup: true,
        weaponPassives: true,
        aowCompatibility: true,
        aowDamage: profileId === "vanilla",
        aowRoutes: profileId === "vanilla",
      },
      rules: {
        standardMaxUpgrade: profileId === "convergence" ? 15 : 25,
        somberMaxUpgrade: profileId === "convergence" ? 15 : 10,
        separateUpgradeCaps: profileId !== "convergence",
        scadutreeScaling: profileId !== "convergence",
        zeroAttackElementUsesWeaponScaling: profileId === "convergence",
        extendedScalingGrades: profileId === "convergence",
        statusBuildupScales: profileId !== "convergence",
      },
    },
  };
}
