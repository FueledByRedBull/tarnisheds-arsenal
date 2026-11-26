import { afterEach, describe, expect, it, vi } from "vitest";

import { api } from "./api";
import { readBuildPreset } from "./presets";
import { carriedHandling, restoreSavedRequest, restoreSessionQuery, startingRequest } from "./restore";
import { STARTING_CLASS_METADATA } from "./session";
import { defaultRequest, useDesktopStore } from "./state";
import { solvedBuild } from "./test-fixtures";
import type { CatalogDto, OptimizeRequestDto } from "./types";

function catalog(profileId = "vanilla"): CatalogDto {
  const convergence = profileId === "convergence";
  return {
    weaponNames: ["Uchigatana"],
    classes: Object.values(STARTING_CLASS_METADATA),
    weaponTypeOptions: [{ key: "katana", label: "Katana" }],
    aowNames: ["Unsheathe"],
    affinityNames: ["Keen", "Standard"],
    objectiveIds: ["max_ar", "max_ar_plus_bleed"],
    somberFilters: ["all"],
    filterDimensions: [{ id: "affinity", label: "Affinity", options: [{ id: "affinity:keen", label: "Keen", count: 1 }] }],
    dataManifest: {
      schemaVersion: 4, datasetVersion: `${profileId}-test`, modelVersion: "test-model", id: `${profileId}-test`,
      label: profileId, appVersion: "1.16.1", source: "test",
      profile: { id: profileId, displayName: profileId, gameVersion: "1.16.1", modVersion: convergence ? "test" : null },
      capabilities: { classBudget: true, statusBuildup: true, aowDamage: !convergence, aowRoutes: !convergence },
      rules: {
        standardMaxUpgrade: convergence ? 15 : 25, somberMaxUpgrade: convergence ? 15 : 10,
        separateUpgradeCaps: !convergence, scadutreeScaling: !convergence,
        scadutreeAttackMultipliers: Array.from({ length: 21 }, (_, level) => 1 + level / 20),
        zeroAttackElementUsesWeaponScaling: convergence, extendedScalingGrades: convergence,
      },
    },
  };
}

const saved: OptimizeRequestDto = { ...defaultRequest, className: "samurai", weaponTypeKey: "katana",
  weaponName: "Uchigatana", affinity: "Keen", aowName: "Unsheathe" };

function restored(request: OptimizeRequestDto, current = catalog()) {
  const outcome = restoreSavedRequest(request, current);
  if (outcome.ok === false) throw new Error(outcome.message);
  return outcome;
}

describe("restoreSavedRequest", () => {
  it("keeps names the current data still has, by key or by older label", () => {
    expect(restored(saved)).toEqual({ ok: true, request: { ...saved, className: "Samurai" }, repairs: [] });
    expect(restored({ ...saved, weaponTypeKey: "Katana" }).repairs).toEqual([]);
  });

  it("loads Convergence classes saved under their former data names", () => {
    const convergence = catalog("convergence");
    convergence.classes = [{ ...STARTING_CLASS_METADATA[0], name: "Stormcaller" }];
    const old = { ...saved, profileId: "convergence", className: "Storm Caller", standardMaxUpgrade: 15, somberMaxUpgrade: 15 };
    expect(restored(old, convergence).request.className).toBe("Stormcaller");
    expect(restoreSavedRequest({ ...old, profileId: "vanilla" }, catalog())).toMatchObject({
      ok: false, reason: "unknown-class", message: expect.stringContaining("Starting class 'Storm Caller'") });
  });

  it("rejects a request for another profile or of the wrong shape", () => {
    expect(restoreSavedRequest({ ...saved, profileId: "convergence" }, catalog())).toMatchObject({ ok: false, reason: "other-profile" });
    expect(restoreSavedRequest({ ...saved, topK: 0 }, catalog())).toMatchObject({ ok: false, reason: "invalid" });
  });

  it("clears a removed weapon type", () => {
    const { request, repairs } = restored({ ...saved, weaponTypeKey: "Hammer" });
    expect(request.weaponTypeKey).toBeNull();
    expect(repairs).toEqual([{ kind: "weapon-type", detail: "weapon type 'Hammer' no longer exists" }]);
  });

  it("clears a removed weapon with the affinity and skill that depended on it", () => {
    const { request, repairs } = restored({ ...saved, weaponName: "Moonveil" });
    expect(request).toMatchObject({ weaponName: null, affinity: null, aowName: null });
    expect(repairs).toEqual([{ kind: "weapon", detail: "weapon 'Moonveil' no longer exists" }]);
  });

  it("clears a removed affinity with its skill, and a removed skill alone", () => {
    expect(restored({ ...saved, affinity: "Bloody" })).toMatchObject({
      request: { weaponName: "Uchigatana", affinity: null, aowName: null },
      repairs: [{ kind: "affinity", detail: "affinity 'Bloody' no longer exists" }],
    });
    expect(restored({ ...saved, aowName: "Old Skill" })).toMatchObject({
      request: { affinity: "Keen", aowName: null },
      repairs: [{ kind: "skill", detail: "skill 'Old Skill' no longer exists" }],
    });
  });

  it("names each profile rule it applies", () => {
    const { request, repairs } = restored({ ...saved, profileId: "convergence", className: "Samurai",
      standardMaxUpgrade: 25, somberMaxUpgrade: 25, dlcScaling: true, scadutreeLevel: 20, somberFilter: "somber_only" }, catalog("convergence"));
    expect(request).toMatchObject({ standardMaxUpgrade: 15, somberMaxUpgrade: 15, dlcScaling: false, scadutreeLevel: 0, somberFilter: "all" });
    expect(repairs.map((repair) => repair.detail)).toEqual([
      "upgrade cap lowered to +15",
      "the Somber filter was cleared, since this profile has one upgrade path",
      "Scadutree blessing was turned off, since this profile has none",
    ]);
  });
});

describe("restoreSessionQuery", () => {
  it("starts from the profile's default class when the saved one is unavailable, and says so", () => {
    const { request, repairs } = restoreSessionQuery({ ...defaultRequest, className: "Missing class", dex: 40, weaponName: "Uchigatana" }, catalog());
    expect(request).toMatchObject({ className: "Samurai", characterLevel: 9, ...STARTING_CLASS_METADATA.find((entry) => entry.name === "Samurai")!.baseStats,
      weaponName: "Uchigatana" });
    expect(repairs).toEqual([{ kind: "class", detail: "starting class 'Missing class' is unavailable, so the query starts from Samurai" }]);
  });

  it("resets stats below the class's starting stats and says so", () => {
    const { request, repairs } = restoreSessionQuery({ ...defaultRequest, dex: 10 }, catalog());
    expect(request.dex).toBe(15);
    expect(repairs).toEqual([{ kind: "stats", detail: "stats below Samurai's starting stats were reset to them" }]);
  });
});

describe("a profile's first query", () => {
  it("starts from the default class at its starting stats", () => {
    const convergence = catalog("convergence");
    convergence.classes = [{ name: "Retainer", baseLevel: 9, baseTotal: 88,
      baseStats: { vig: 15, mnd: 10, end: 11, strStat: 14, dex: 13, intStat: 9, fai: 9, arc: 7 } }];
    expect(startingRequest({ ...defaultRequest, profileId: "convergence" }, convergence)).toMatchObject({
      className: "Retainer", characterLevel: 9, strStat: 14, dex: 13, standardMaxUpgrade: 15 });
  });

  it("keeps only how weapons are held and upgraded, the blessing and how results are listed", () => {
    const edited = { ...defaultRequest, twoHanding: true, exactUpgrade: true, dlcScaling: true, scadutreeLevel: 12,
      resultGrouping: "loadout" as const, topK: 80, dex: 40, weaponName: "Uchigatana", objective: "max_ar_plus_bleed" as const };
    expect(carriedHandling(edited)).toEqual({ twoHanding: true, exactUpgrade: true, dlcScaling: true, scadutreeLevel: 12,
      resultGrouping: "loadout", topK: 80 });
  });
});

// Written by the released build to local storage; the keys and shapes must keep reading the same.
const stored: Record<string, string> = {
  "tarnisheds-arsenal.query.v1.vanilla": String.raw`{"version":1,"request":{"profileId":"vanilla","className":"Samurai","characterLevel":9,"vig":12,"mnd":11,"end":13,"strStat":12,"dex":30,"intStat":9,"fai":8,"arc":8,"minStr":0,"minDex":20,"minInt":0,"minFai":0,"minArc":0,"lockStr":null,"lockDex":null,"lockInt":null,"lockFai":null,"lockArc":null,"standardMaxUpgrade":25,"somberMaxUpgrade":10,"exactUpgrade":false,"twoHanding":true,"dlcScaling":false,"scadutreeLevel":0,"weaponName":"Uchigatana","affinity":"Keen","aowName":"Unsheathe","weaponTypeKey":null,"somberFilter":"all","filters":{"version":1,"entries":[{"dimension":"affinity","id":"affinity:keen","mode":"include"}]},"resultGrouping":"automatic","objective":"max_ar_plus_bleed","topK":40},"lockedStatMode":true}`,
  "tarnisheds-arsenal.savedBuild.v2.c34f333b-fc89-4d54-8d91-2bdfb18b1158": String.raw`{"version":2,"id":"c34f333b-fc89-4d54-8d91-2bdfb18b1158","name":"Fixture","profileId":"vanilla","request":{"profileId":"vanilla","className":"Samurai","characterLevel":9,"vig":12,"mnd":11,"end":13,"strStat":12,"dex":30,"intStat":9,"fai":8,"arc":8,"minStr":0,"minDex":20,"minInt":0,"minFai":0,"minArc":0,"lockStr":null,"lockDex":40,"lockInt":null,"lockFai":null,"lockArc":null,"standardMaxUpgrade":25,"somberMaxUpgrade":10,"exactUpgrade":false,"twoHanding":true,"dlcScaling":false,"scadutreeLevel":0,"weaponName":"Uchigatana","affinity":"Keen","aowName":"Unsheathe","weaponTypeKey":null,"somberFilter":"all","filters":{"version":1,"entries":[{"dimension":"affinity","id":"affinity:keen","mode":"include"}]},"resultGrouping":"automatic","objective":"max_ar_plus_bleed","topK":40},"selectedBuild":{"weaponId":1,"weaponName":"Uchigatana","affinity":"Keen","isSomber":false,"upgrade":25,"aowId":100,"aowName":"Unsheathe","bleedBuildup":45,"bleedBuildupAdd":0,"frostBuildup":0,"poisonBuildup":0,"scarletRotBuildup":0,"sleepBuildup":0,"madnessBuildup":0,"deathBuildup":0,"aowFirstHitDamage":300,"aowFullSequenceDamage":300,"score":500,"stats":{"strStat":18,"dex":40,"intStat":9,"fai":8,"arc":8},"ar":{"physical":500,"magic":0,"fire":0,"lightning":0,"holy":0,"total":500},"aowRoute":null},"compareTarget":{"weaponId":1,"weaponName":"Uchigatana","affinity":"Standard","isSomber":false,"upgrade":25,"aowId":100,"aowName":"Unsheathe","bleedBuildup":45,"bleedBuildupAdd":0,"frostBuildup":0,"poisonBuildup":0,"scarletRotBuildup":0,"sleepBuildup":0,"madnessBuildup":0,"deathBuildup":0,"aowFirstHitDamage":300,"aowFullSequenceDamage":300,"score":500,"stats":{"strStat":18,"dex":40,"intStat":9,"fai":8,"arc":8},"ar":{"physical":500,"magic":0,"fire":0,"lightning":0,"holy":0,"total":500},"aowRoute":null},"compareBench":[{"weaponId":1,"weaponName":"Uchigatana","affinity":"Keen","isSomber":false,"upgrade":25,"aowId":100,"aowName":"Unsheathe","bleedBuildup":45,"bleedBuildupAdd":0,"frostBuildup":0,"poisonBuildup":0,"scarletRotBuildup":0,"sleepBuildup":0,"madnessBuildup":0,"deathBuildup":0,"aowFirstHitDamage":300,"aowFullSequenceDamage":300,"score":500,"stats":{"strStat":18,"dex":40,"intStat":9,"fai":8,"arc":8},"ar":{"physical":500,"magic":0,"fire":0,"lightning":0,"holy":0,"total":500},"aowRoute":null}],"dataVersion":"vanilla:4:vanilla-test:test-model","createdAt":"2026-11-26T06:28:07.125Z","updatedAt":"2026-11-26T06:28:07.125Z"}`,
  "tarnisheds-arsenal.savedBuildIndex.v1": String.raw`{"version":1,"builds":[{"id":"c34f333b-fc89-4d54-8d91-2bdfb18b1158","name":"Fixture","profileId":"vanilla","dataVersion":"vanilla:4:vanilla-test:test-model","updatedAt":"2026-11-26T06:28:07.125Z"}]}`,
  "tarnisheds-arsenal.compareBench.v1.vanilla": String.raw`{"version":1,"datasetVersion":"vanilla-test","schemaVersion":4,"modelVersion":"test-model","rows":[{"weaponId":1,"weaponName":"Uchigatana","affinity":"Keen","isSomber":false,"upgrade":25,"stats":{"strStat":18,"dex":40,"intStat":9,"fai":8,"arc":8},"ar":{"physical":500,"magic":0,"fire":0,"lightning":0,"holy":0,"total":500},"aowId":100,"aowName":"Unsheathe","bleedBuildup":45,"bleedBuildupAdd":0,"frostBuildup":0,"poisonBuildup":0,"scarletRotBuildup":0,"sleepBuildup":0,"madnessBuildup":0,"deathBuildup":0,"aowFirstHitDamage":300,"aowFullSequenceDamage":300,"aowRoute":null,"score":500}]}`,
};

describe("local storage written by the released build", () => {
  const values = new Map<string, string>();
  const query = JSON.parse(stored["tarnisheds-arsenal.query.v1.vanilla"]).request as OptimizeRequestDto;

  function openStorage() {
    values.clear();
    for (const [key, value] of Object.entries(stored)) values.set(key, value);
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    });
    useDesktopStore.setState({ catalog: null, catalogStatus: "loading", profiles: [catalog().dataManifest], request: defaultRequest,
      profileQueries: {}, lockedStatMode: false, compareBench: [], rows: [], selected: null, compareTarget: null, notices: [] });
    useDesktopStore.getState().beginProfileSwitch("vanilla");
    useDesktopStore.getState().setCatalog(catalog());
  }

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("restores the session query and pinned builds unchanged and without notices", () => {
    openStorage();
    const state = useDesktopStore.getState();
    expect(state.request).toEqual(query);
    expect(state.lockedStatMode).toBe(true);
    expect(state.compareBench).toEqual([solvedBuild()]);
    expect(state.notices.map((notice) => notice.message)).toEqual(["Restored your vanilla query. Search to rank it again."]);
    expect(Object.fromEntries(values)).toEqual(stored);
  });

  it("loads the saved build with its inputs unchanged and its rows verified", async () => {
    openStorage();
    const target = solvedBuild({ affinity: "Standard" });
    vi.spyOn(api, "solveBuild").mockImplementation(async (_base, _weapon, affinity) => affinity === "Keen" ? solvedBuild() : target);
    const preset = readBuildPreset("c34f333b-fc89-4d54-8d91-2bdfb18b1158")!;
    // Read back byte for byte, so saving it again writes the same record.
    expect(JSON.stringify(preset)).toBe(stored["tarnisheds-arsenal.savedBuild.v2.c34f333b-fc89-4d54-8d91-2bdfb18b1158"]);

    await useDesktopStore.getState().loadBuildPreset(preset);

    const state = useDesktopStore.getState();
    expect(state.request).toEqual({ ...query, lockDex: 40 });
    expect(state.lockedStatMode).toBe(true);
    expect(state).toMatchObject({ selected: solvedBuild(), compareTarget: target, compareBench: [solvedBuild()] });
    expect(state.notices.map((notice) => notice.message)).toEqual([
      "Loaded Fixture; saved results verified on current data. Its pinned builds replaced the 1 you had pinned."]);
    expect(values.get("tarnisheds-arsenal.savedBuild.v2.c34f333b-fc89-4d54-8d91-2bdfb18b1158"))
      .toBe(stored["tarnisheds-arsenal.savedBuild.v2.c34f333b-fc89-4d54-8d91-2bdfb18b1158"]);
  });
});
