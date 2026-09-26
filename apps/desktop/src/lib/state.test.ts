import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultRequest, useDesktopStore } from "./state";
import { buildOptimizeRequest, normalizeOptimizeRequest } from "./session";
import { parsePresetText } from "./presets";
import type { CatalogDto, SolvedBuildDto } from "./types";

it.each(["paths", "affinity_watch"] as const)("starting %s clears its previous outcome without hiding other notices", (scope) => {
  useDesktopStore.setState({
    error: "Previous analysis failed",
    notices: [
      { scope, tone: "warning", message: "Previous analysis stopped" },
      { scope: "global", tone: "warning", message: "Saved rows were discarded" },
    ],
  });
  const state = useDesktopStore.getState();
  if (scope === "paths") state.beginPath("new-path");
  else state.beginAffinity("new-affinity");
  expect(useDesktopStore.getState().error).toBeNull();
  expect(useDesktopStore.getState().notices).toEqual([
    { scope: "global", tone: "warning", message: "Saved rows were discarded" },
  ]);
});

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

const routeRow: SolvedBuildDto = {
  ...row,
  weaponTypeName: "Katana",
  requirements: { strStat: 11, dex: 15, intStat: 0, fai: 0, arc: 0 },
  effectiveScaling: { str: 0.2, dex: 1.4, int: 0, fai: 0, arc: 0 },
  aowRoute: {
    routeId: "light", routeLabel: "Light", routePriority: 0, buffActivationActionId: null,
    actions: [{
      actionId: "attack", actionOrder: 0, staminaCost: 10,
      hits: [{
        sheetRow: 1, hitOrder: 0, rawName: "Unsheathe", damage: row.ar, poiseDamage: 10,
        statusBuildup: { bleed: 45, frost: 0, poison: 0, scarletRot: 0, sleep: 0, madness: 0, death: 0 },
        physicalAttackAttribute: "Slash", buffActive: false, warnings: [],
        effects: [{
          effectId: 1, effectName: "Attack", role: "damage", activationTiming: "hit",
          isSupported: true, reason: "", attackPower: row.ar,
          statusBuildup: { bleed: 0, frost: 0, poison: 0, scarletRot: 0, sleep: 0, madness: 0, death: 0 },
        }],
      }],
    }],
    firstHitDamage: 300, totalDamage: row.ar, totalPoiseDamage: 10,
    totalStatusBuildup: { bleed: 45, frost: 0, poison: 0, scarletRot: 0, sleep: 0, madness: 0, death: 0 },
    totalStaminaCost: 10,
  },
};

function restoreCompareRows(rows: unknown[], profile = catalog("vanilla")) {
  const payload = JSON.stringify({
    version: 1, datasetVersion: profile.dataManifest.datasetVersion,
    schemaVersion: profile.dataManifest.schemaVersion, modelVersion: profile.dataManifest.modelVersion, rows,
  });
  vi.stubGlobal("localStorage", { getItem: () => payload });
  try {
    useDesktopStore.setState({ compareBench: [], notices: [] });
    useDesktopStore.getState().setCatalog(profile);
    return useDesktopStore.getState();
  } finally {
    vi.unstubAllGlobals();
  }
}

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

  it.each(["draft edit", "replacement search"])("invalidates analysis ownership and retained results on %s", (action) => {
    const state = useDesktopStore.getState();
    state.setRows([row]);
    state.setCompareTarget(row);
    state.beginPath("old-path");
    state.setActivePathJobId("path-job");
    state.beginAffinity("old-affinity");
    state.setActiveAffinityJobId("affinity-job");
    state.setPaths([{ title: "Selected", solved: row, steps: [] }], "old-path");
    state.setAffinityPayload({ lines: [], breakpoints: [] }, "old-affinity");
    const before = useDesktopStore.getState();
    if (action === "draft edit") state.markResultsStale();
    else state.beginSearch("new-search");
    const after = useDesktopStore.getState();
    expect(after.pathGeneration).toBeGreaterThan(before.pathGeneration);
    expect(after.affinityGeneration).toBeGreaterThan(before.affinityGeneration);
    expect(after).toMatchObject({ resultsStale: true, selected: row, compareTarget: null,
      isPathBusy: false, activePathJobId: null, pathSignature: null, paths: [],
      isAffinityBusy: false, activeAffinityJobId: null, affinitySignature: null, affinityPayload: null });
  });

  it.each(["lockDex", "lockInt", "lockFai", "lockArc"] as const)("preserves an imported %s-only lock through hydration and request construction", (lock) => {
    const preset = parsePresetText(JSON.stringify({
      version: 2, id: "partial-lock", name: "Partial lock", profileId: "vanilla",
      request: { ...defaultRequest, [lock]: 40 }, selectedBuild: null, compareTarget: null, compareBench: [],
      dataVersion: "vanilla:9:dataset:model", createdAt: "2026-01-01T00:00:00.000Z", updatedAt: "2026-01-01T00:00:00.000Z",
    }));
    useDesktopStore.getState().loadBuildPreset(preset);
    const loaded = useDesktopStore.getState();
    const outgoing = buildOptimizeRequest(loaded.catalog, loaded.request, loaded.lockedStatMode);
    expect(loaded.lockedStatMode).toBe(true);
    expect(outgoing[lock]).toBe(40);
    expect(outgoing.lockStr).toBeNull();
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

  it("restores supported comparison rows including complete routes and optional older fields", () => {
    const unnamedSkill = { ...row, aowId: 65535, aowName: null, score: 3.4028235e38 };
    const restored = restoreCompareRows([row, routeRow, unnamedSkill]);
    expect(restored.compareBench).toEqual([row, routeRow, unnamedSkill]);
    expect(restored.notices).toEqual([]);
    const convergence = { ...row, isSomber: true, upgrade: 15 };
    expect(restoreCompareRows([convergence], catalog("convergence")).compareBench).toEqual([convergence]);
    expect(restoreCompareRows([{ ...convergence, upgrade: 16 }], catalog("convergence")).compareBench).toEqual([]);
    const statBounds = { ...row, stats: { strStat: 0, dex: 99, intStat: 0, fai: 99, arc: 0 } };
    expect(restoreCompareRows([statBounds]).compareBench).toEqual([statBounds]);
  });

  it("discards malformed comparison records while retaining valid records and warning", () => {
    const invalid: unknown[] = [
      { ...row, weaponId: -1 }, { ...row, weaponId: 1.5 }, { ...row, weaponId: 2 ** 32 },
      { ...row, isSomber: "false" }, { ...row, upgrade: 25.5 }, { ...row, upgrade: 26 },
      { ...row, isSomber: true, upgrade: 11 }, { ...row, score: 1e100 },
      { ...row, aowId: 65536 }, { ...row, aowId: -1 }, { ...row, aowName: 3 },
      { ...row, stats: { ...row.stats, dex: 100 } }, { ...row, stats: { ...row.stats, dex: -1 } },
      { ...row, stats: { ...row.stats, dex: 4.5 } }, { ...row, ar: { total: 500 } },
      { ...row, ar: { ...row.ar, physical: "500" } }, { ...row, bleedBuildup: "45" },
      { ...row, aowFullSequenceDamage: null }, { ...row, aowRoute: {} },
      { ...routeRow, requirements: { strStat: 1 } }, { ...routeRow, effectiveScaling: { dex: 1 } },
      { ...row, weaponTypeName: 2 },
    ];
    for (const [index, candidate] of invalid.entries()) {
      const restored = restoreCompareRows([candidate, row]);
      expect(restored.compareBench, `malformed record ${index}`).toEqual([row]);
      expect(restored.notices.at(-1), `notice for record ${index}`).toMatchObject({ scope: "global", tone: "warning" });
    }
  });

  it("rejects malformed numbers, arrays and metadata throughout restored route data", () => {
    const mutations: ((value: SolvedBuildDto) => void)[] = [
      (value) => { delete (value as Partial<SolvedBuildDto>).frostBuildup; },
      (value) => { value.aowRoute!.actions = null as never; },
      (value) => { value.aowRoute!.routePriority = -1; },
      (value) => { value.aowRoute!.totalDamage.magic = NaN; },
      (value) => { value.aowRoute!.totalStaminaCost = Infinity; },
      (value) => { value.aowRoute!.buffActivationActionId = 1 as never; },
      (value) => { value.aowRoute!.actions[0].actionOrder = 65536; },
      (value) => { value.aowRoute!.actions[0].staminaCost = "10" as never; },
      (value) => { value.aowRoute!.actions[0].hits[0].sheetRow = 0.5; },
      (value) => { value.aowRoute!.actions[0].hits[0].buffActive = null as never; },
      (value) => { value.aowRoute!.actions[0].hits[0].warnings = [1] as never; },
      (value) => { value.aowRoute!.actions[0].hits[0].statusBuildup = {} as never; },
      (value) => { value.aowRoute!.actions[0].hits[0].effects[0].effectId = 2 ** 32; },
      (value) => { value.aowRoute!.actions[0].hits[0].effects[0].isSupported = "yes" as never; },
      (value) => { value.aowRoute!.actions[0].hits[0].effects[0].attackPower = {} as never; },
    ];
    for (const [index, mutate] of mutations.entries()) {
      const invalid = JSON.parse(JSON.stringify(routeRow)) as SolvedBuildDto;
      mutate(invalid);
      expect(restoreCompareRows([invalid, row]).compareBench, `route mutation ${index}`).toEqual([row]);
    }
  });

  it("requires every serialized DTO field except supported optional metadata", () => {
    function requiredPaths(value: unknown, prefix: string[] = []): string[][] {
      if (typeof value !== "object" || value === null) return [];
      return Object.entries(value).flatMap(([key, child]) => {
        const path = [...prefix, key];
        const optional = prefix.length === 0 && ["weaponTypeName", "requirements", "effectiveScaling"].includes(key);
        const own = Array.isArray(value) || optional ? [] : [path];
        return [...own, ...requiredPaths(child, path)];
      });
    }
    for (const path of requiredPaths(routeRow)) {
      // JSON cloning also separates repeated damage objects so each nested guard is exercised.
      const invalid = JSON.parse(JSON.stringify(routeRow)) as Record<string, unknown>;
      let parent = invalid;
      for (const key of path.slice(0, -1)) parent = parent[key] as Record<string, unknown>;
      delete parent[path.at(-1)!];
      expect(restoreCompareRows([invalid, row]).compareBench, `missing ${path.join(".")}`).toEqual([row]);
    }
  });

  it("rejects nonfinite JSON numbers and keeps the supported eight-row limit", () => {
    const profile = catalog("vanilla");
    const payload = JSON.stringify({
      version: 1, datasetVersion: profile.dataManifest.datasetVersion,
      schemaVersion: profile.dataManifest.schemaVersion, modelVersion: profile.dataManifest.modelVersion,
      rows: [{ ...row, score: "overflow" }, row],
    }).replace('"score":"overflow"', '"score":1e400');
    vi.stubGlobal("localStorage", { getItem: () => payload });
    try {
      useDesktopStore.getState().setCatalog(profile);
      expect(useDesktopStore.getState().compareBench).toEqual([row]);
    } finally {
      vi.unstubAllGlobals();
    }
    const valid = Array.from({ length: 9 }, (_, index) => ({ ...row, weaponId: index + 1 }));
    expect(restoreCompareRows([{}, ...valid]).compareBench).toEqual(valid.slice(0, 8));
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

  it("keeps comparison changes usable in memory when optional persistence fails", () => {
    vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => { throw new Error("storage full"); } });
    try {
      useDesktopStore.getState().setCatalog(catalog("vanilla"));
      useDesktopStore.setState({ compareBench: [], notices: [] });
      useDesktopStore.getState().toggleCompareBench(row);
      expect(useDesktopStore.getState().compareBench).toEqual([row]);
      expect(useDesktopStore.getState().notices.at(-1)).toMatchObject({ tone: "warning" });
      useDesktopStore.getState().patchCompareControls({ weaponName: "Uchigatana" });
      expect(useDesktopStore.getState().compareBench).toEqual([]);
      useDesktopStore.getState().toggleCompareBench(row);
      useDesktopStore.getState().clearCompareBench();
      expect(useDesktopStore.getState().compareBench).toEqual([]);
    } finally {
      vi.unstubAllGlobals();
    }
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
