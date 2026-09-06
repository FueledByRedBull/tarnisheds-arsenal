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
} from "./presets";
import type { BuildPresetV1, SolvedBuildDto } from "./types";

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

function solvedBuild(): SolvedBuildDto {
  return {
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
}

describe("saved build persistence", () => {
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

  it("migrates saved builds created before all status fields were exposed", () => {
    const raw = JSON.parse(JSON.stringify({
      ...preset(),
      selectedBuild: solvedBuild(),
      compareTarget: solvedBuild(),
    }));
    for (const key of ["selectedBuild", "compareTarget"] as const) {
      delete raw[key].sleepBuildup;
      delete raw[key].madnessBuildup;
      delete raw[key].deathBuildup;
    }

    const parsed = parsePresetText(JSON.stringify(raw));

    expect(parsed.selectedBuild).toMatchObject({
      sleepBuildup: 0,
      madnessBuildup: 0,
      deathBuildup: 0,
    });
    expect(parsed.compareTarget).toMatchObject({
      sleepBuildup: 0,
      madnessBuildup: 0,
      deathBuildup: 0,
    });
  });

  it("previews conflicts and rejects oversized input", () => {
    importBuildPreset(preset());
    const preview = previewPresetImport(JSON.stringify(preset()));
    expect(preview).toMatchObject({ idConflict: true, nameConflict: true });

    expect(() => parsePresetText("x".repeat(MAX_PRESET_IMPORT_BYTES + 1)))
      .toThrow(/too large/i);
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
    expect(() => saveBuildPreset(legacy)).toThrow(/Storage quota exceeded/);
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
