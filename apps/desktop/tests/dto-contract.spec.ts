import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import {
  SCADUTREE_ATTACK_MULTIPLIERS,
  SCADUTREE_MAX_LEVEL,
} from "../src/lib/scadutree";
import type {
  BuildPresetV1,
  CatalogDto,
  SearchJobStatusDto,
  SavedBuildIndexV1,
} from "../src/lib/types";

test("app DTO samples use the camelCase backend contract", () => {
  const catalog = {
    weaponCount: 1,
    aowCount: 1,
    weaponNames: ["Uchigatana"],
    weaponTypeKeys: ["katana"],
    classes: [{
      name: "Samurai",
      baseLevel: 9,
      baseTotal: 80,
      baseStats: { vig: 12, mnd: 11, end: 13, strStat: 12, dex: 15, intStat: 9, fai: 8, arc: 8 },
    }],
    weaponTypeOptions: [{ key: "katana", label: "Katana" }],
    aowNames: ["Seppuku"],
    objectiveIds: ["max_ar"],
    somberFilters: ["all"],
    dataManifest: {
      id: "phase1",
      label: "Phase 1",
      appVersion: "0.5.0",
      source: "test",
      generatedAt: "2026-06-10T00:00:00Z",
    },
  } satisfies CatalogDto;

  expect(catalog).toHaveProperty("weaponCount");
  expect(catalog).toHaveProperty("dataManifest.appVersion");
  expect(catalog.classes[0]).toHaveProperty("baseStats.strStat");
  expect(catalog).not.toHaveProperty("weapon_count");
  expect(catalog).not.toHaveProperty("data_manifest");
});

test("job and saved-build DTO samples preserve app-facing field names", () => {
  const status = {
    progress: {
      jobId: "search-1",
      checked: 10,
      total: 100,
      eligible: 3,
      bestScore: 42.5,
      elapsedMs: 250,
    },
    finished: {
      jobId: "search-1",
      cancelled: false,
      rows: [{
        weaponId: 100,
        weaponName: "Uchigatana",
        affinity: "Blood",
        isSomber: false,
        upgrade: 25,
        stats: { strStat: 12, dex: 60, intStat: 9, fai: 8, arc: 45 },
        ar: { physical: 500, magic: 0, fire: 0, lightning: 0, holy: 0, total: 500 },
        aowId: 1,
        aowName: "Seppuku",
        bleedBuildup: 84,
        bleedBuildupAdd: 30,
        frostBuildup: 0,
        poisonBuildup: 0,
        scarletRotBuildup: 0,
        aowFirstHitDamage: 100,
        aowFullSequenceDamage: 250,
        score: 500,
      }],
      error: null,
    },
  } satisfies SearchJobStatusDto;

  const preset = {
    version: 1,
    id: "preset-1",
    name: "Blood Uchi",
    request: {
      className: "Samurai",
      characterLevel: 150,
      vig: 50,
      mnd: 11,
      end: 30,
      strStat: 12,
      dex: 60,
      intStat: 9,
      fai: 8,
      arc: 45,
      minStr: 12,
      minDex: 15,
      minInt: 9,
      minFai: 8,
      minArc: 8,
      lockStr: null,
      lockDex: null,
      lockInt: null,
      lockFai: null,
      lockArc: null,
      standardMaxUpgrade: 25,
      somberMaxUpgrade: 10,
      exactUpgrade: true,
      twoHanding: false,
      dlcScaling: true,
      scadutreeLevel: 20,
      weaponName: "Uchigatana",
      affinity: "Blood",
      aowName: "Seppuku",
      weaponTypeKey: "katana",
      somberFilter: "all",
      objective: "max_ar",
      topK: 10,
    },
    selectedBuild: status.finished.rows[0],
    compareTarget: null,
    dataVersion: "phase1",
    createdAt: "2026-06-10T00:00:00Z",
    updatedAt: "2026-06-10T00:00:00Z",
  } satisfies BuildPresetV1;

  const index = {
    version: 1,
    builds: [{ id: preset.id, name: preset.name, dataVersion: preset.dataVersion, updatedAt: preset.updatedAt }],
  } satisfies SavedBuildIndexV1;

  expect(status).toHaveProperty("progress.bestScore");
  expect(status.finished?.rows[0]).toHaveProperty("weaponName");
  expect(status.finished?.rows[0]).toHaveProperty("aowFirstHitDamage");
  expect(preset).toHaveProperty("request.standardMaxUpgrade");
  expect(preset).toHaveProperty("request.somberMaxUpgrade");
  expect(preset).toHaveProperty("request.exactUpgrade");
  expect(preset).toHaveProperty("selectedBuild.weaponName");
  expect(index.builds[0]).toHaveProperty("dataVersion");
});

test("Scadutree UI constants match Rust core constants", () => {
  const rustMath = readFileSync(
    new URL("../../../core/er_optimizer_core/src/math.rs", import.meta.url),
    "utf8",
  );

  const maxLevel = rustMath.match(/SCADUTREE_MAX_LEVEL:\s*u8\s*=\s*(\d+)/)?.[1];
  const multiplierBlock = rustMath.match(/SCADUTREE_ATTACK_MULTIPLIERS:\s*\[f32;\s*21\]\s*=\s*\[([\s\S]*?)\];/)?.[1];

  expect(Number(maxLevel)).toBe(SCADUTREE_MAX_LEVEL);
  expect(parseRustNumberList(multiplierBlock ?? "")).toEqual(Array.from(SCADUTREE_ATTACK_MULTIPLIERS));
});

function parseRustNumberList(block: string): number[] {
  return block
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => Number(part.replace(/_f32$/, "")));
}
