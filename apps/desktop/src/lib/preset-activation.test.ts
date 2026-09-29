import { beforeEach, expect, it, vi } from "vitest";

const { solveBuild } = vi.hoisted(() => ({ solveBuild: vi.fn() }));
vi.mock("./api", () => ({ api: { solveBuild } }));

import { verifyPresetResults } from "./preset-activation";
import { defaultRequest } from "./state";
import type { BuildPreset, CatalogDto, SolvedBuildDto } from "./types";

const samurai = { vig: 12, mnd: 11, end: 13, strStat: 12, dex: 15, intStat: 9, fai: 8, arc: 8 };
const catalog = {
  classes: [{ name: "Samurai", baseLevel: 9, baseTotal: 88, baseStats: samurai }],
  dataManifest: { capabilities: { classBudget: true } },
} as unknown as CatalogDto;

function row(weaponName: string, stats: SolvedBuildDto["stats"]): SolvedBuildDto {
  return {
    weaponId: 1, weaponName, affinity: "Standard", isSomber: false, upgrade: 25, aowId: null, aowName: null, stats,
  } as unknown as SolvedBuildDto;
}

beforeEach(() => solveBuild.mockReset());

it("verifies each saved row at the level its own stats require", async () => {
  const selected = row("Uchigatana", { strStat: 12, dex: 15, intStat: 9, fai: 8, arc: 8 });
  // Pinned from a higher-level search: 136 combat points, more than a level-9 Samurai owns.
  const pinned = row("Fire Knight's Greatsword", { strStat: 22, dex: 18, intStat: 9, fai: 79, arc: 8 });
  const preset = {
    request: { ...defaultRequest, className: "Samurai", characterLevel: 9, ...samurai },
    selectedBuild: selected, compareTarget: null, compareBench: [pinned],
  } as unknown as BuildPreset;
  solveBuild.mockImplementation(async (_base, weaponName) => weaponName === pinned.weaponName ? pinned : selected);

  await verifyPresetResults(preset, catalog, new AbortController().signal);

  const [selectedRequest] = solveBuild.mock.calls[0];
  const [pinnedRequest] = solveBuild.mock.calls[1];
  expect(selectedRequest).toMatchObject({ characterLevel: 9, lockStr: 12, lockFai: 8 });
  // 84 combat points above the class base -> level 93, with the row's stats as exact locks.
  expect(pinnedRequest).toMatchObject({ characterLevel: 93, strStat: 12, fai: 8, lockStr: 22, lockFai: 79 });
});

it("verifies the selected build and comparison target against the saved inputs' level", async () => {
  // Rows needing level 93 cannot be current results of a level-9 build.
  const oversized = row("Fire Knight's Greatsword", { strStat: 22, dex: 18, intStat: 9, fai: 79, arc: 8 });
  const preset = {
    request: { ...defaultRequest, className: "Samurai", characterLevel: 9, ...samurai },
    selectedBuild: oversized, compareTarget: oversized, compareBench: [],
  } as unknown as BuildPreset;
  solveBuild.mockResolvedValue(oversized);

  await verifyPresetResults(preset, catalog, new AbortController().signal);

  for (const [request] of solveBuild.mock.calls) {
    expect(request).toMatchObject({ characterLevel: 9, lockStr: 22, lockFai: 79 });
  }
});
