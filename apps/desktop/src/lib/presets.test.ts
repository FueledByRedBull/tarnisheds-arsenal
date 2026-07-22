import { beforeEach, describe, expect, it } from "vitest";

import { defaultRequest } from "./state";
import {
  MAX_PRESET_IMPORT_BYTES,
  importBuildPreset,
  loadBuildPreset,
  parsePresetText,
  previewPresetImport,
  saveBuildPreset,
  savedBuildIndex,
} from "./presets";
import type { BuildPresetV1 } from "./types";

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

  it("removes legacy packaged-smoke presets leaked by older release jobs", () => {
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

    expect(savedBuildIndex().builds).toEqual([]);
    expect(localStorage.getItem("tarnisheds-arsenal.savedBuild.v1.smoke-id")).toBeNull();
  });

  it("migrates legacy profile-less presets to Vanilla explicitly", () => {
    const legacy = preset();
    const raw = JSON.parse(JSON.stringify(legacy));
    delete raw.profileId;
    delete raw.request.profileId;

    const parsed = parsePresetText(JSON.stringify(raw));

    expect(parsed.profileId).toBe("vanilla");
    expect(parsed.request.profileId).toBe("vanilla");
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
    ];
    for (const candidate of malformed) {
      expect(() => parsePresetText(candidate)).toThrow();
    }
    expect(savedBuildIndex().builds).toEqual([]);
  });
});
