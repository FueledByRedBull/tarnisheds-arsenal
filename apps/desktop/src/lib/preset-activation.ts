import { api } from "./api";
import { noStatMinimums, statLocksAt } from "./combat-stats";
import { buildOptimizeRequest, upgradeCapPatch } from "./session";
import type { BuildPreset, CatalogDto, OptimizeRequestDto, SolvedBuildDto } from "./types";

export function catalogVersion(catalog: CatalogDto): string {
  const manifest = catalog.dataManifest;
  return `${manifest.profile.id}:${manifest.schemaVersion}:${manifest.datasetVersion}:${manifest.modelVersion}`;
}

// Persisted numerical snapshots are archives, not evidence that this binary calculated them.
export async function verifyPresetResults(preset: BuildPreset, catalog: CatalogDto, signal: AbortSignal): Promise<BuildPreset> {
  // With one shared cap (Convergence) Somber has no effect, and a data revision can move a weapon
  // between the groups; the saved flag is then not part of the build's identity.
  const unifiedCaps = catalog.dataManifest.rules.separateUpgradeCaps === false;
  async function verify(row: SolvedBuildDto | null, label: string, ownBudget = false): Promise<SolvedBuildDto | null> {
    if (!row) return null;
    // Current results must fit the saved inputs' level. A pinned build may come from another
    // level, so its budget is derived from its own stats, as every locked search does.
    const base: OptimizeRequestDto = buildOptimizeRequest(catalog, { ...preset.request, ...(ownBudget ? row.stats : {}),
      weaponName: row.weaponName, affinity: row.affinity, aowName: row.aowName,
      weaponTypeKey: null, somberFilter: "all", filters: { version: 1, entries: [] },
      exactUpgrade: true,
      ...upgradeCapPatch(!unifiedCaps, row.isSomber, row.upgrade),
      ...noStatMinimums(),
      ...statLocksAt(row.stats),
    });
    const solved = await api.solveBuild(base, row.weaponName, row.affinity, row.aowName, signal);
    if (!solved) throw new Error(`${label} cannot be verified with its saved equipment, upgrade and stats.`);
    const identity = ["weaponId", "weaponName", "affinity", "upgrade", "aowId", "aowName"] as const;
    if (identity.some(key => solved[key] !== row[key]) || (!unifiedCaps && solved.isSomber !== row.isSomber)
      || (Object.keys(row.stats) as (keyof typeof row.stats)[]).some(key => solved.stats[key] !== row.stats[key])) {
      throw new Error(`${label} equipment identity or stats contradict current game data. The saved record is unchanged.`);
    }
    return solved;
  }
  const selectedBuild = await verify(preset.selectedBuild, "Selected build");
  const compareTarget = await verify(preset.compareTarget, "Comparison target");
  const compareBench: SolvedBuildDto[] = [];
  for (const [index, row] of preset.compareBench.entries()) {
    compareBench.push((await verify(row, `Pinned build ${index + 1}`, true))!);
  }
  return { ...preset, selectedBuild, compareTarget, compareBench };
}
