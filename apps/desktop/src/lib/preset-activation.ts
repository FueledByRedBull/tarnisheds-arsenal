import { api } from "./api";
import { buildOptimizeRequest } from "./session";
import type { BuildPreset, CatalogDto, OptimizeRequestDto, SolvedBuildDto } from "./types";

export function activationRequest(request: OptimizeRequestDto, catalog: CatalogDto): OptimizeRequestDto {
  if (request.profileId !== catalog.dataManifest.profile.id) {
    throw new Error(`This build belongs to ${request.profileId}. Switch game profiles before loading it.`);
  }
  const matches = catalog.classes.filter(entry => entry.name.toLowerCase() === request.className.toLowerCase());
  if (matches.length !== 1) {
    throw new Error(`Starting class '${request.className}' is unavailable in this profile. Correct the saved inputs before loading; the original record is unchanged.`);
  }
  return { ...request, className: matches[0].name };
}

export function catalogVersion(catalog: CatalogDto): string {
  const manifest = catalog.dataManifest;
  return `${manifest.profile.id}:${manifest.schemaVersion}:${manifest.datasetVersion}:${manifest.modelVersion}`;
}

// Persisted numerical snapshots are archives, not evidence that this binary calculated them.
export async function verifyPresetResults(preset: BuildPreset, catalog: CatalogDto, signal: AbortSignal): Promise<BuildPreset> {
  async function verify(row: SolvedBuildDto | null, label: string, ownBudget = false): Promise<SolvedBuildDto | null> {
    if (!row) return null;
    // Current results must fit the saved inputs' level. A pinned build may come from another
    // level, so its budget is derived from its own stats, as every locked search does.
    const base: OptimizeRequestDto = buildOptimizeRequest(catalog, { ...preset.request, ...(ownBudget ? row.stats : {}),
      weaponName: row.weaponName, affinity: row.affinity, aowName: row.aowName,
      weaponTypeKey: null, somberFilter: "all", filters: { version: 1, entries: [] },
      exactUpgrade: true,
      standardMaxUpgrade: row.isSomber ? preset.request.standardMaxUpgrade : row.upgrade,
      somberMaxUpgrade: row.isSomber ? row.upgrade : preset.request.somberMaxUpgrade,
      minStr: 0, minDex: 0, minInt: 0, minFai: 0, minArc: 0,
      lockStr: row.stats.strStat, lockDex: row.stats.dex, lockInt: row.stats.intStat,
      lockFai: row.stats.fai, lockArc: row.stats.arc,
    });
    const solved = await api.solveBuild(base, row.weaponName, row.affinity, row.aowName, signal);
    if (!solved) throw new Error(`${label} cannot be verified with its saved equipment, upgrade and stats.`);
    const identity = ["weaponId", "weaponName", "affinity", "isSomber", "upgrade", "aowId", "aowName"] as const;
    if (identity.some(key => solved[key] !== row[key])
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
