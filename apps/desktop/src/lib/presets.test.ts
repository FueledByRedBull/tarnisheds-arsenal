import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { defaultRequest } from "./state";
import {
  MAX_PRESET_IMPORT_BYTES,
  deleteBuildPreset,
  importBuildPreset,
  loadBuildPreset,
  parsePresetText,
  previewPresetImport,
  saveBuildPreset,
  savedBuildIndex,
  renameBuildPreset,
  replaceImportedBuildPreset,
  inspectSavedBuilds,
  recoverSavedBuilds,
  savedBuildBackupText,
  previewBuildBackup,
  restoreBuildBackup,
  shareTextForPreset,
  MAX_BUILD_BACKUP_BYTES,
  MAX_BUILD_BACKUP_COUNT,
} from "./presets";
import type { BuildPresetV1, SolvedBuildDto } from "./types";
import { validateStoredBuild } from "./stored-build";
import { solvedBuild } from "./test-fixtures";

class MemoryStorage implements Storage {
  private values = new Map<string, string>();
  get length() { return this.values.size; }
  clear() { this.values.clear(); }
  getItem(key: string) { return this.values.get(key) ?? null; }
  key(index: number) { return Array.from(this.values.keys())[index] ?? null; }
  removeItem(key: string) { this.values.delete(key); }
  setItem(key: string, value: string) { this.values.set(key, value); }
}

function preset(id = "preset-one", name = "Dexterity route"): BuildPresetV1 {
  return {
    version: 1,
    id,
    name,
    profileId: "vanilla",
    request: { ...defaultRequest },
    selectedBuild: null,
    compareTarget: null,
    dataVersion: "1:dataset:model",
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z",
  };
}

function routedBuild(): SolvedBuildDto {
  const build = solvedBuild();
  const status = { bleed: 45, frost: 0, poison: 0, scarletRot: 0, sleep: 0, madness: 0, death: 0 };
  return { ...build, weaponTypeName: "Katana", requirements: build.stats,
    effectiveScaling: { str: 0.2, dex: 1.4, int: 0, fai: 0, arc: 0 },
    aowRoute: { routeId: "light", routeLabel: "Light", routePriority: 0, buffActivationActionId: null,
      firstHitDamage: 300, totalDamage: build.ar, totalPoiseDamage: 10, totalStatusBuildup: status, totalStaminaCost: 10,
      actions: [{ actionId: "attack", actionOrder: 0, staminaCost: 10, hits: [{
        sheetRow: 1, hitOrder: 0, rawName: "Unsheathe", damage: build.ar, poiseDamage: 10,
        statusBuildup: status, physicalAttackAttribute: "Slash", buffActive: false, warnings: [], effects: [],
      }] }],
    },
  };
}

describe("saved build persistence", () => {
  it.each([
    ["missing route", (build: any) => { delete build.aowRoute; }, "build.aowRoute"],
    ["non-native float", (build: any) => { build.aowRoute.actions[0].hits[0].damage = { ...build.ar, total: 1e100 }; }, "build.aowRoute.actions[0].hits[0].damage.total"],
    ["oversized warning", (build: any) => { build.aowRoute.actions[0].hits[0].warnings = ["x".repeat(1001)]; }, "build.aowRoute.actions[0].hits[0].warnings[0]"],
    ["invalid stats", (build: any) => { build.stats.dex = 100; }, "build.stats.dex"],
    ["invalid hit order", (build: any) => { build.aowRoute.actions[0].hits[0].hitOrder = -1; }, "build.aowRoute.actions[0].hits[0].hitOrder"],
  ] as const)("uses shared structural validation for %s", (_name, mutate, path) => {
    const build = structuredClone(routedBuild());
    mutate(build);
    expect(validateStoredBuild(build, { standardMaxUpgrade: 25, somberMaxUpgrade: 25 })).toMatchObject({ error: expect.stringContaining(`${path} `) });
    // Schema 2: only schema-1 records get later fields backfilled.
    expect(() => parsePresetText(JSON.stringify({ ...preset(), version: 2, compareBench: [], selectedBuild: build }))).toThrow(path.replace(/^build/, "selectedBuild"));
  });

  it("keeps archive shape limits distinct from profile-specific activation limits", () => {
    const build = { ...routedBuild(), isSomber: true, upgrade: 15 };
    expect(validateStoredBuild(build, { standardMaxUpgrade: 25, somberMaxUpgrade: 25 })).toHaveProperty("value");
    expect(validateStoredBuild(build, { standardMaxUpgrade: 25, somberMaxUpgrade: 10 })).toMatchObject({ error: expect.stringMatching(/^build\.upgrade /) });
    expect(validateStoredBuild(build, { standardMaxUpgrade: 15, somberMaxUpgrade: 15 })).toHaveProperty("value");
    expect(parsePresetText(JSON.stringify({ ...preset(), version: 2, compareBench: [], selectedBuild: build })).selectedBuild).toEqual(build);
  });

  beforeEach(() => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: new MemoryStorage(),
    });
  });

  it("updates a selected preset without creating a duplicate", () => {
    const first = saveBuildPreset({ ...preset(), dataVersion: "1:dataset:model" });
    saveBuildPreset({ ...preset(first.id, "Updated route"), dataVersion: "1:dataset:model" });

    expect(savedBuildIndex().builds).toHaveLength(1);
    expect(loadBuildPreset(first.id)?.name).toBe("Updated route");
  });

  it("keeps conflicting imports as an explicit copy", () => {
    const existing = importBuildPreset(preset());
    const imported = importBuildPreset(preset(existing.id));

    expect(imported.id).not.toBe(existing.id);
    expect(imported.name).toBe("Dexterity route (imported)");
    expect(savedBuildIndex().builds).toHaveLength(2);
  });

  it("stores profile identity in both the preset and index", () => {
    const saved = saveBuildPreset({
      ...preset(),
      request: { ...defaultRequest, profileId: "convergence" },
      dataVersion: "convergence:3:convergence-3.0.0.1:model",
    });

    expect(saved.profileId).toBe("convergence");
    expect(saved.request.profileId).toBe("convergence");
    expect(savedBuildIndex().builds[0].profileId).toBe("convergence");
  });

  it("round-trips a Convergence preset with the profile's +15 caps", () => {
    const convergence = preset();
    convergence.profileId = "convergence";
    convergence.request = {
      ...defaultRequest,
      profileId: "convergence",
      className: "Custom stats",
      characterLevel: 792,
      vig: 99,
      mnd: 99,
      end: 99,
      strStat: 99,
      dex: 99,
      intStat: 99,
      fai: 99,
      arc: 99,
      lockStr: 99,
      lockDex: 99,
      lockInt: 99,
      lockFai: 99,
      lockArc: 99,
      standardMaxUpgrade: 15,
      somberMaxUpgrade: 15,
    };
    convergence.dataVersion = "convergence:4:convergence-3.0.0.1:model";

    const parsed = parsePresetText(JSON.stringify(convergence));

    expect(parsed.profileId).toBe("convergence");
    expect(parsed.request).toMatchObject({
      profileId: "convergence",
      characterLevel: 792,
      standardMaxUpgrade: 15,
      somberMaxUpgrade: 15,
    });

    const saved = saveBuildPreset({
      name: parsed.name,
      request: parsed.request,
      selectedBuild: parsed.selectedBuild,
      compareTarget: parsed.compareTarget,
      compareBench: parsed.compareBench,
      dataVersion: parsed.dataVersion,
    });
    expect(loadBuildPreset(saved.id)?.request).toMatchObject({
      characterLevel: 792,
      standardMaxUpgrade: 15,
      somberMaxUpgrade: 15,
    });
  });

  it("never deletes a build based on its name", () => {
    const smoke = preset("smoke-id", "Packaged smoke 1784149134195");
    localStorage.setItem("tarnisheds-arsenal.savedBuild.v1.smoke-id", JSON.stringify(smoke));
    localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.v1", JSON.stringify({
      version: 1,
      builds: [{
        id: smoke.id,
        name: smoke.name,
        profileId: smoke.profileId,
        dataVersion: smoke.dataVersion,
        updatedAt: smoke.updatedAt,
      }],
    }));

    expect(savedBuildIndex().builds).toHaveLength(1);
    expect(localStorage.getItem("tarnisheds-arsenal.savedBuild.v1.smoke-id")).not.toBeNull();
    saveBuildPreset({ ...preset("new-smoke", smoke.name) });
    expect(savedBuildIndex().builds).toHaveLength(2);
    expect(loadBuildPreset("new-smoke")).not.toBeNull();
  });

  it("migrates legacy profile-less presets to Vanilla explicitly", () => {
    const legacy = preset();
    const raw = JSON.parse(JSON.stringify(legacy));
    delete raw.profileId;
    delete raw.request.profileId;
    raw.request.budgetMode = "target_level";
    raw.request.offensivePointBudget = 40;

    const parsed = parsePresetText(JSON.stringify(raw));

    expect(parsed.profileId).toBe("vanilla");
    expect(parsed.request.profileId).toBe("vanilla");
    expect(parsed).toMatchObject({ version: 2, compareBench: [] });
    expect(parsed.request).toMatchObject({
      filters: { version: 1, entries: [] },
      resultGrouping: "automatic",
    });
    expect(parsed.request).not.toHaveProperty("budgetMode");
    expect(parsed.request).not.toHaveProperty("offensivePointBudget");
  });

  it("migrates schema-1 builds created before later result fields existed", () => {
    const raw = JSON.parse(JSON.stringify({
      ...preset(),
      selectedBuild: solvedBuild(),
      compareTarget: solvedBuild(),
      compareBench: [solvedBuild()],
    }));
    for (const build of [raw.selectedBuild, raw.compareTarget, raw.compareBench[0]]) {
      for (const key of ["sleepBuildup", "madnessBuildup", "deathBuildup", "frostBuildup", "bleedBuildupAdd", "aowId", "aowRoute"]) {
        delete build[key];
      }
    }

    const parsed = parsePresetText(JSON.stringify(raw));

    for (const build of [parsed.selectedBuild, parsed.compareTarget, parsed.compareBench[0]]) {
      expect(build).toMatchObject({
        sleepBuildup: 0,
        madnessBuildup: 0,
        deathBuildup: 0,
        frostBuildup: 0,
        bleedBuildupAdd: 0,
        aowId: null,
        aowRoute: null,
      });
    }
  });

  // Schema-1 records (v0.7 through v0.10) stored skill routes without poise damage.
  function schemaOneRecord() {
    const record = JSON.parse(JSON.stringify({ ...preset("old-route"), selectedBuild: routedBuild(), compareTarget: routedBuild() }));
    for (const build of [record.selectedBuild, record.compareTarget]) {
      delete build.aowRoute.totalPoiseDamage;
      for (const hit of build.aowRoute.actions[0].hits) delete hit.poiseDamage;
    }
    localStorage.setItem(`tarnisheds-arsenal.savedBuild.v1.${record.id}`, JSON.stringify(record));
    localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.v1", JSON.stringify({ version: 1, builds: [
      { id: record.id, name: record.name, profileId: record.profileId, dataVersion: record.dataVersion, updatedAt: record.updatedAt },
    ] }));
    return record;
  }

  it("opens and backs up schema-1 builds whose skill routes predate poise damage", () => {
    const record = schemaOneRecord();
    // The route is re-solved on load, so the migrated archive keeps none of its unverifiable numbers.
    expect(loadBuildPreset(record.id)).toMatchObject({ version: 2, name: record.name, selectedBuild: { aowName: "Unsheathe", aowRoute: null } });
    const inspection = inspectSavedBuilds();
    expect(inspection.issues).toEqual([]);
    expect(JSON.parse(savedBuildBackupText(inspection)).builds.map((build: { id: string }) => build.id)).toEqual([record.id]);
  });

  it("re-imports a schema-1 build as a copy or over its stored record", () => {
    const record = schemaOneRecord();
    const text = JSON.stringify(record);
    expect(previewPresetImport(text)).toMatchObject({ idConflict: true, nameConflict: true });
    const copy = importBuildPreset(parsePresetText(text));
    expect(copy.id).not.toBe(record.id);
    const replaced = replaceImportedBuildPreset(parsePresetText(text));
    expect(replaced.id).toBe(record.id);
    expect(loadBuildPreset(record.id)?.version).toBe(2);
    expect(inspectSavedBuilds()).toMatchObject({ issues: [], needsRecovery: false });
    expect(savedBuildIndex().builds).toHaveLength(2);
  });

  it("previews conflicts and rejects oversized input", () => {
    importBuildPreset(preset());
    const preview = previewPresetImport(JSON.stringify(preset()));
    expect(preview).toMatchObject({ idConflict: true, nameConflict: true });

    expect(() => parsePresetText("x".repeat(3 * MAX_PRESET_IMPORT_BYTES + 1)))
      .toThrow(/too large/i);
  });

  function presetWithWarnings(count: number) {
    const build = routedBuild();
    build.aowRoute!.actions[0].hits[0].warnings = Array.from({ length: count }, () => "{".repeat(1000));
    return { ...preset(), selectedBuild: build };
  }

  it("round-trips share text, including text longer than the limit once encoded", () => {
    const saved = saveBuildPreset({ ...presetWithWarnings(240), dataVersion: "1:dataset:model" });
    const share = shareTextForPreset(saved);
    expect(share.length).toBeGreaterThan(MAX_PRESET_IMPORT_BYTES);
    expect(parsePresetText(share)).toEqual(saved);
  });

  it("accepts legacy share text and explains damaged share text", () => {
    expect(parsePresetText(`ta-v1:${encodeURIComponent(JSON.stringify(preset()))}`)).toMatchObject({ version: 2, name: "Dexterity route" });
    expect(() => parsePresetText("ta-v2:%E0%A4%A")).toThrow(/damaged or incomplete/);
  });

  it("refuses to save or import a build over the limit, leaving storage unchanged", () => {
    const large = { ...presetWithWarnings(200), compareTarget: presetWithWarnings(200).selectedBuild };
    expect(() => saveBuildPreset({ ...large, dataVersion: "1:dataset:model" })).toThrow(/too large/);
    expect(localStorage.length).toBe(0);
    expect(() => parsePresetText(JSON.stringify({ ...large, version: 2, compareBench: [] }))).toThrow(/too large/);
  });

  it("keeps a __proto__ key as plain data", () => {
    const text = JSON.stringify(preset()).replace("{", "{\"__proto__\":{\"polluted\":true},");
    parsePresetText(text);
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
  });

  it("rebuilds presets, builds, backups and the index from known keys only", () => {
    // Adds an extra key and an own __proto__ key to every object in the JSON text.
    const withExtras = (value: unknown) => JSON.stringify(value).replaceAll("{\"", "{\"__proto__\":{\"polluted\":true},\"extra\":\"x\",\"");
    const unknownKeys = (value: unknown): boolean => typeof value === "object" && value !== null
      && (Object.hasOwn(value, "extra") || Object.hasOwn(value, "__proto__") || Object.values(value).some(unknownKeys));
    const clean = saveBuildPreset({ ...preset(), request: { ...defaultRequest, filters: { version: 1, entries: [{ dimension: "weapon_type", id: "katana", mode: "include" }] } },
      selectedBuild: routedBuild(), compareTarget: null, compareBench: [routedBuild()] });
    const text = withExtras(clean);
    expect(text).toContain("\"extra\"");

    const shared = parsePresetText(`ta-v2:${encodeURIComponent(text)}`);
    const restoredBackup = previewBuildBackup(withExtras({ format: "tarnisheds-arsenal.build-backup", version: 1, builds: [clean] })).presets[0];
    localStorage.setItem(`tarnisheds-arsenal.savedBuild.v2.${clean.id}`, text);
    localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.v1", withExtras(savedBuildIndex()));
    const stored = loadBuildPreset(clean.id);
    for (const result of [parsePresetText(text), shared, restoredBackup, stored, inspectSavedBuilds().presets[0]]) {
      expect(result).toEqual(clean);
      expect(unknownKeys(result)).toBe(false);
    }
    expect(unknownKeys(savedBuildIndex())).toBe(false);
    const build = validateStoredBuild(JSON.parse(withExtras(routedBuild())), { standardMaxUpgrade: 25, somberMaxUpgrade: 25 });
    expect(build).toEqual({ value: routedBuild() });
    expect(unknownKeys(build)).toBe(false);
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
  });

  it("rejects malformed import corpus without persisting anything", () => {
    const malformed = [
      "",
      "null",
      "[]",
      "{}",
      '{"version":2}',
      JSON.stringify({ ...preset(), id: "" }),
      JSON.stringify({ ...preset(), name: "" }),
      JSON.stringify({ ...preset(), dataVersion: 4 }),
      JSON.stringify({ ...preset(), dataVersion: "missing-parts" }),
      JSON.stringify({ ...preset(), profileId: "convergence" }),
      JSON.stringify({ ...preset(), createdAt: "not-a-date" }),
      JSON.stringify({ ...preset(), request: { ...defaultRequest, objective: "damage_everything" } }),
      JSON.stringify({ ...preset(), request: { ...defaultRequest, topK: 0 } }),
      JSON.stringify({ ...preset(), request: { ...defaultRequest, scadutreeLevel: 21 } }),
      JSON.stringify({ ...preset(), request: { ...defaultRequest, characterLevel: Number.NaN } }),
      JSON.stringify({ ...preset(), selectedBuild: { weaponId: 1 } }),
      JSON.stringify({ ...preset(), selectedBuild: { ...solvedBuild(), sleepBuildup: "unknown" } }),
    ];
    for (const [index, candidate] of malformed.entries()) {
      expect(() => parsePresetText(candidate), `malformed corpus entry ${index}`).toThrow();
    }
    expect(savedBuildIndex().builds).toEqual([]);
  });
  afterEach(() => vi.restoreAllMocks());

  it("rejects null comparison pins before import, restore, or recovery can expose them to the UI", () => {
    const malformed = { ...preset(), version: 2, compareBench: [null] };
    expect(() => parsePresetText(JSON.stringify(malformed))).toThrow(/compareBench/);
    expect(() => restoreBuildBackup(JSON.stringify({
      format: "tarnisheds-arsenal.build-backup", version: 1, builds: [malformed],
    }))).toThrow(/compareBench/);
    expect(savedBuildIndex().builds).toEqual([]);
    localStorage.setItem(`tarnisheds-arsenal.savedBuild.v2.${malformed.id}`, JSON.stringify(malformed));
    expect(loadBuildPreset(malformed.id)).toBeNull();
    expect(inspectSavedBuilds().presets).toEqual([]);
  });

  it("rejects array-valued request enums instead of accepting their string conversion", () => {
    const mutations = [
      ["somberFilter", ["all"]], ["objective", ["max_ar"]],
      ["resultGrouping", ["automatic"]],
      ["filters", { version: 1, entries: [{ dimension: ["affinity"], id: "Keen", mode: "include" }] }],
      ["filters", { version: 1, entries: [{ dimension: "affinity", id: "Keen", mode: ["include"] }] }],
    ] as const;
    for (const [key, value] of mutations) {
      const malformed = { ...preset(), request: { ...defaultRequest, [key]: value } };
      expect(() => parsePresetText(JSON.stringify(malformed)), key).toThrow(/valid BuildPreset/);
      expect(() => previewBuildBackup(JSON.stringify({
        format: "tarnisheds-arsenal.build-backup", version: 1, builds: [malformed],
      })), key).toThrow(/valid BuildPreset/);
    }
  });

  it("blocks writes to a damaged index and recovers valid orphan records without erasing evidence", () => {
    const saved = saveBuildPreset(preset());
    const indexKey = "tarnisheds-arsenal.savedBuildIndex.v1";
    localStorage.setItem(indexKey, "{broken");
    localStorage.setItem("tarnisheds-arsenal.savedBuild.v2.bad", "{also broken");
    expect(() => savedBuildIndex()).toThrow(/recovery/i);
    expect(() => saveBuildPreset(preset("new"))).toThrow(/recovery/i);
    const scan = inspectSavedBuilds();
    expect(scan.presets.map((entry) => entry.id)).toEqual([saved.id]);
    expect(scan.issues).toHaveLength(2);
    const backupKey = recoverSavedBuilds(scan);
    expect(localStorage.getItem(backupKey!)).toBe("{broken");
    expect(localStorage.getItem("tarnisheds-arsenal.savedBuild.v2.bad")).toBe("{also broken");
    expect(savedBuildIndex().builds[0].id).toBe(saved.id);
  });

  it("rejects malformed route, hit and display fields at both preset and backup boundaries", () => {
    const valid = { ...preset(), version: 2, compareBench: [], selectedBuild: routedBuild() };
    expect(parsePresetText(JSON.stringify(valid)).selectedBuild).toEqual(valid.selectedBuild);
    const mutations = [
      ["aowRoute.totalPoiseDamage", "10"], ["aowRoute.totalPoiseDamage", undefined],
      ["aowRoute.actions.0.hits.0.poiseDamage", "10"], ["aowRoute.actions.0.hits.0.poiseDamage", null],
      ["aowRoute.totalDamage.total", 1e39], ["aowRoute.actions.0.hits.0.poiseDamage", 1e39],
      ["aowRoute.routePriority", -1], ["aowRoute.routePriority", 65536],
      ["aowRoute.actions.0.actionOrder", 65536], ["aowRoute.actions.0.hits.0.hitOrder", 65536],
      ["aowRoute.actions.0.hits.0.sheetRow", 65536],
      ["effectiveScaling.str", "1.4"], ["effectiveScaling.arc", 1e39],
      ["requirements.dex", "15"], ["requirements.dex", 256], ["weaponTypeName", {}],
    ] as const;
    for (const [path, value] of mutations) {
      const malformed = JSON.parse(JSON.stringify(valid));
      const keys = path.split(".");
      let target = malformed.selectedBuild;
      for (const key of keys.slice(0, -1)) target = target[key];
      target[keys.at(-1)!] = value;
      expect(() => parsePresetText(JSON.stringify(malformed)), path).toThrow(/valid BuildPreset/);
      expect(() => previewBuildBackup(JSON.stringify({ format: "tarnisheds-arsenal.build-backup", version: 1, builds: [malformed] })), path).toThrow(/valid BuildPreset/);
    }
  });

  it("rejects stale recovery previews and leaves all stored bytes untouched on backup quota failure", () => {
    saveBuildPreset(preset());
    const scan = inspectSavedBuilds();
    saveBuildPreset(preset("second"));
    expect(() => recoverSavedBuilds(scan)).toThrow(/changed/i);
    localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.v1", "broken");
    const broken = inspectSavedBuilds();
    vi.spyOn(localStorage, "setItem").mockImplementation(() => { throw new Error("quota"); });
    expect(() => recoverSavedBuilds(broken)).toThrow(/quota/);
    expect(localStorage.getItem("tarnisheds-arsenal.savedBuildIndex.v1")).toBe("broken");
  });

  it("validates an entire backup before restoring and keeps every existing build on conflict", () => {
    const saved = saveBuildPreset(preset());
    const backup = savedBuildBackupText(inspectSavedBuilds());
    const parsed = previewBuildBackup(backup);
    expect(parsed.presets).toHaveLength(1);
    const restored = restoreBuildBackup(backup);
    expect(restored[0].id).not.toBe(saved.id);
    expect(restored[0].name).toBe("Dexterity route (imported)");
    const malformed = JSON.parse(backup);
    malformed.builds.push({ version: 2 });
    expect(() => restoreBuildBackup(JSON.stringify(malformed))).toThrow();
    expect(savedBuildIndex().builds).toHaveLength(2);
    expect(loadBuildPreset(saved.id)).toEqual(saved);
  });

  it("rolls back a partially written backup restore without modifying existing builds", () => {
    const saved = saveBuildPreset(preset());
    const backup = savedBuildBackupText(inspectSavedBuilds());
    const write = localStorage.setItem.bind(localStorage);
    vi.spyOn(localStorage, "setItem").mockImplementation((key, value) => {
      if (key === "tarnisheds-arsenal.savedBuildIndex.v1") throw new Error("index quota");
      write(key, value);
    });
    expect(() => restoreBuildBackup(backup)).toThrow(/index quota/);
    expect(inspectSavedBuilds().presets).toEqual([saved]);
    expect(savedBuildIndex().builds).toHaveLength(1);
  });

  it("retains recoverable orphan records when interruption prevents cleanup", () => {
    const saved = saveBuildPreset(preset());
    const backup = savedBuildBackupText(inspectSavedBuilds());
    const write = localStorage.setItem.bind(localStorage);
    vi.spyOn(localStorage, "setItem").mockImplementation((key, value) => {
      if (key === "tarnisheds-arsenal.savedBuildIndex.v1") throw new Error("index unavailable");
      write(key, value);
    });
    vi.spyOn(localStorage, "removeItem").mockImplementation(() => { throw new Error("cleanup unavailable"); });
    expect(() => restoreBuildBackup(backup)).toThrow(/index unavailable/);
    expect(savedBuildIndex().builds).toHaveLength(1);
    expect(loadBuildPreset(saved.id)).toEqual(saved);
    const interrupted = inspectSavedBuilds();
    expect(interrupted).toMatchObject({ orphanCount: 1, needsRecovery: true });
    expect(interrupted.presets).toHaveLength(2);
    vi.restoreAllMocks();
    recoverSavedBuilds(interrupted);
    expect(savedBuildIndex().builds).toHaveLength(2);
  });

  it("rejects oversized, duplicate, and malformed backup entries before any write", () => {
    saveBuildPreset(preset());
    const raw = savedBuildBackupText(inspectSavedBuilds());
    const envelope = JSON.parse(raw);
    const write = vi.spyOn(localStorage, "setItem");
    expect(() => restoreBuildBackup(" ".repeat(MAX_BUILD_BACKUP_BYTES) + raw)).toThrow(/too large/);
    expect(() => restoreBuildBackup(JSON.stringify({ ...envelope, builds: [envelope.builds[0], envelope.builds[0]] }))).toThrow(/duplicate/);
    expect(() => restoreBuildBackup(JSON.stringify({ ...envelope, builds: Array(501).fill(envelope.builds[0]) }))).toThrow(/500/);
    // Deterministic field mutations cover the shared single/bulk import boundary.
    const badValues = [null, [], {}, false, 0, "", "x".repeat(301)];
    for (const field of ["id", "name", "request", "dataVersion", "createdAt", "selectedBuild"]) {
      for (const value of badValues) {
        if (field === "selectedBuild" && value === null) continue;
        const invalid = { ...envelope.builds[0], [field]: value };
        expect(() => restoreBuildBackup(JSON.stringify({ ...envelope, builds: [envelope.builds[0], invalid] }))).toThrow();
      }
    }
    expect(write).not.toHaveBeenCalled();
  });

  it("detects partial and duplicate indexes instead of discarding their bad entries", () => {
    saveBuildPreset(preset());
    const entry = savedBuildIndex().builds[0];
    for (const builds of [[entry, null], [entry, entry], [{ ...entry, id: "" }]]) {
      const raw = JSON.stringify({ version: 1, builds });
      localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.v1", raw);
      expect(() => savedBuildIndex()).toThrow(/recovery/);
      expect(() => deleteBuildPreset(entry.id)).toThrow(/recovery/);
      expect(localStorage.getItem("tarnisheds-arsenal.savedBuildIndex.v1")).toBe(raw);
    }
  });

  it("preserves a damaged current record while recovering its readable legacy version", () => {
    const legacy = preset();
    localStorage.setItem(`tarnisheds-arsenal.savedBuild.v1.${legacy.id}`, JSON.stringify(legacy));
    const currentKey = `tarnisheds-arsenal.savedBuild.v2.${legacy.id}`;
    localStorage.setItem(currentKey, "{broken");
    const scan = inspectSavedBuilds();
    expect(scan.presets).toHaveLength(1);
    recoverSavedBuilds(scan);
    expect(loadBuildPreset(legacy.id)?.name).toBe(legacy.name);
    expect(() => renameBuildPreset(legacy.id, "Changed")).toThrow(/unreadable/);
    expect(localStorage.getItem(currentKey)).toBe("{broken");
  });


  it("keeps only the three newest recovery copies of the index", () => {
    localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.recovery.legacy-random-id", "{oldest");
    vi.useFakeTimers();
    try {
      for (let copy = 0; copy < 4; copy += 1) {
        vi.setSystemTime(new Date(Date.UTC(2026, 0, 1, 0, 0, copy)));
        localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.v1", `{damaged-${copy}`);
        recoverSavedBuilds(inspectSavedBuilds());
      }
    } finally {
      vi.useRealTimers();
    }
    const copies = Array.from({ length: localStorage.length }, (_, index) => localStorage.key(index)!)
      .filter((key) => key.startsWith("tarnisheds-arsenal.savedBuildIndex.recovery."))
      .map((key) => localStorage.getItem(key));
    expect(copies.sort()).toEqual(["{damaged-1", "{damaged-2", "{damaged-3"]);
  });

  it("exports a recovered library larger than the restore limit", () => {
    const saved = saveBuildPreset(preset());
    const record = localStorage.getItem(`tarnisheds-arsenal.savedBuild.v2.${saved.id}`)!;
    for (let index = 0; index < MAX_BUILD_BACKUP_COUNT; index += 1) {
      localStorage.setItem(`tarnisheds-arsenal.savedBuild.v2.extra-${index}`, record.replace(saved.id, `extra-${index}`));
    }
    const backup = JSON.parse(savedBuildBackupText(inspectSavedBuilds()));
    expect(backup.builds).toHaveLength(MAX_BUILD_BACKUP_COUNT + 1);
  });

  it("backs up existing valid records beyond the single-share import limit", () => {
    const saved = saveBuildPreset(preset());
    // Long route warnings are valid; the total backup still has a 10 MiB limit.
    const build = routedBuild();
    const route = build.aowRoute!;
    const hit = { ...route.actions[0].hits[0], warnings: Array<string>(256).fill("x".repeat(1000)) };
    const stored = { ...saved, selectedBuild: { ...build, aowRoute: { ...route, actions: [{ ...route.actions[0], hits: [hit, { ...hit, hitOrder: 1 }] }] } } };
    expect(JSON.stringify(stored).length).toBeGreaterThan(MAX_PRESET_IMPORT_BYTES);
    localStorage.setItem(`tarnisheds-arsenal.savedBuild.v2.${saved.id}`, JSON.stringify(stored));
    expect(loadBuildPreset(saved.id)).toEqual(stored);
    const backup = savedBuildBackupText(inspectSavedBuilds());
    expect(previewBuildBackup(backup).presets).toEqual([stored]);
  });

  it("rejects new saves and imports at capacity before changing storage", () => {
    for (let index = 0; index < 500; index += 1) saveBuildPreset(preset(`build-${index}`));
    const before = localStorage.getItem("tarnisheds-arsenal.savedBuildIndex.v1");
    expect(() => saveBuildPreset(preset("overflow"))).toThrow(/limit reached/);
    expect(() => importBuildPreset(preset("overflow"))).toThrow(/limit reached/);
    expect(() => replaceImportedBuildPreset(preset("overflow"))).toThrow(/limit reached/);
    expect(localStorage.getItem("tarnisheds-arsenal.savedBuild.v2.overflow")).toBeNull();
    expect(localStorage.getItem("tarnisheds-arsenal.savedBuildIndex.v1")).toBe(before);
    saveBuildPreset(preset("build-0", "Updated"));
    expect(savedBuildIndex().builds).toHaveLength(500);
    expect(loadBuildPreset("build-0")?.name).toBe("Updated");
  });

  it("retains an oversized existing library and permits updates", () => {
    const saved = saveBuildPreset(preset("build-0"));
    const entry = savedBuildIndex().builds[0];
    const builds = Array.from({ length: 501 }, (_, index) => ({ ...entry, id: `build-${index}` }));
    localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.v1", JSON.stringify({ version: 1, builds }));
    expect(savedBuildIndex().builds).toHaveLength(501);
    renameBuildPreset(saved.id, "Renamed");
    expect(savedBuildIndex().builds).toHaveLength(501);
    expect(() => saveBuildPreset(preset("overflow"))).toThrow(/limit reached/);
  });

  it("rejects overlong rename without damaging the existing build", () => {
    const saved = saveBuildPreset(preset());
    expect(() => renameBuildPreset(saved.id, "x".repeat(201))).toThrow(/200 characters/);
    expect(loadBuildPreset(saved.id)?.name).toBe(saved.name);
    expect(savedBuildIndex().builds).toHaveLength(1);
  });

  it("bounds imported names including successive conflict suffixes", () => {
    const saved = saveBuildPreset(preset("long-name", "x".repeat(200)));
    const first = importBuildPreset(saved);
    const second = importBuildPreset(saved);
    expect(first.name).toHaveLength(200);
    expect(second.name).toHaveLength(200);
    expect(second.name).not.toBe(first.name);
    expect(loadBuildPreset(first.id)?.name).toBe(first.name);
    expect(loadBuildPreset(second.id)?.name).toBe(second.name);
    expect(savedBuildIndex().builds).toHaveLength(3);
  });

  it("loads and migrates legacy builds without requiring writable storage", () => {
    const legacy = preset();
    localStorage.setItem(`tarnisheds-arsenal.savedBuild.v1.${legacy.id}`, JSON.stringify(legacy));
    const write = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
      throw new DOMException("Storage quota exceeded", "QuotaExceededError");
    });
    expect(loadBuildPreset(legacy.id)).toMatchObject({ version: 2, name: legacy.name });
    expect(write).not.toHaveBeenCalled();
    expect(() => saveBuildPreset(legacy)).toThrow(/index is missing.*recovery/);
  });

  it("rolls back a preset write when its index cannot be saved", () => {
    const saved = saveBuildPreset(preset());
    const write = localStorage.setItem.bind(localStorage);
    vi.spyOn(localStorage, "setItem").mockImplementation((key, value) => {
      if (key === "tarnisheds-arsenal.savedBuildIndex.v1") throw new Error("index quota exceeded");
      write(key, value);
    });
    expect(() => renameBuildPreset(saved.id, "Changed")).toThrow(/index quota/);
    expect(loadBuildPreset(saved.id)?.name).toBe(saved.name);
    expect(() => saveBuildPreset(preset("new-id"))).toThrow(/index quota/);
    expect(loadBuildPreset("new-id")).toBeNull();
  });

  it("preserves the record and index if deletion cannot save the index", () => {
    const saved = saveBuildPreset(preset());
    const write = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
      throw new DOMException("index quota exceeded", "QuotaExceededError");
    });
    expect(() => deleteBuildPreset(saved.id)).toThrow(/index quota/);
    expect(loadBuildPreset(saved.id)).toEqual(saved);
    expect(savedBuildIndex().builds.map((entry) => entry.id)).toEqual([saved.id]);
    write.mockRestore();
    deleteBuildPreset(saved.id);
    expect(loadBuildPreset(saved.id)).toBeNull();
    expect(savedBuildIndex().builds).toEqual([]);
  });

});
