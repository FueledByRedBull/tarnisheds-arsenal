import { api } from "./api";
import { buildOptimizeRequest } from "./session";
import type { BuildPreset, CatalogDto, OptimizeRequestDto, SolvedBuildDto } from "./types";

// Convergence class names now come from the game's menu text; earlier saves hold the data row names.
const RENAMED_CLASSES: Record<string, Record<string, string>> = {
  convergence: {
    "storm caller": "Stormcaller",
    "retainer alt (tarnished edition)": "Retainer (Alt)",
    "dreadnought alt (tarnished edition)": "Dreadnought (Alt)",
  },
};

export function currentClassName(profileId: string, className: string): string {
  return RENAMED_CLASSES[profileId]?.[className.toLowerCase()] ?? className;
}

export function activationRequest(request: OptimizeRequestDto, catalog: CatalogDto): OptimizeRequestDto {
  if (request.profileId !== catalog.dataManifest.profile.id) {
    throw new Error(`This build belongs to ${request.profileId}. Switch game profiles before loading it.`);
  }
  const className = currentClassName(request.profileId, request.className).toLowerCase();
  const matches = catalog.classes.filter(entry => entry.name.toLowerCase() === className);
  if (matches.length !== 1) {
    throw new Error(`Starting class '${request.className}' is unavailable in this profile. Correct the saved inputs before loading; the original record is unchanged.`);
  }
  return { ...request, className: matches[0].name };
}

/** Clears saved names the current data no longer has, before Migrate data verifies the build. */
export function migratePresetRequest(request: OptimizeRequestDto, catalog: CatalogDto): { request: OptimizeRequestDto; issues: string[] } {
  const migrated = activationRequest(request, catalog);
  return { request: migrated, issues: clearMissingNames(migrated, catalog) };
}

/** Clears the names and filters of a restored query that the current catalog no longer has; returns what it cleared. */
export function reconcileRestoredQuery(request: OptimizeRequestDto, catalog: CatalogDto): { request: OptimizeRequestDto; issues: string[] } {
  const reconciled = { ...request };
  const issues = clearMissingNames(reconciled, catalog);
  // Coverage filters are not catalog dimensions; native validation checks them.
  const entries = request.filters.entries.filter((entry) => entry.dimension === "coverage" || catalog.filterDimensions
    .some((dimension) => dimension.id === entry.dimension && dimension.options.some((option) => option.id === entry.id)));
  const dropped = request.filters.entries.length - entries.length;
  if (dropped) {
    issues.push(dropped === 1 ? "1 filter no longer exists" : `${dropped} filters no longer exist`);
    reconciled.filters = { ...request.filters, entries };
  }
  return { request: reconciled, issues };
}

// Edits the request it is given, which must be a copy.
function clearMissingNames(migrated: OptimizeRequestDto, catalog: CatalogDto): string[] {
  const issues: string[] = [];
  // Older builds stored the type's label; newer ones its key. Either still filters correctly.
  if (migrated.weaponTypeKey && !catalog.weaponTypeOptions.some((option) => option.key === migrated.weaponTypeKey || option.label === migrated.weaponTypeKey)) {
    issues.push(`weapon type '${migrated.weaponTypeKey}' no longer exists`);
    migrated.weaponTypeKey = null;
  }
  if (migrated.weaponName && !catalog.weaponNames.includes(migrated.weaponName)) {
    issues.push(`weapon '${migrated.weaponName}' no longer exists`);
    migrated.weaponName = null;
    migrated.affinity = null;
    migrated.aowName = null;
  }
  if (migrated.affinity && !catalog.affinityNames.includes(migrated.affinity)) {
    issues.push(`affinity '${migrated.affinity}' no longer exists`);
    migrated.affinity = null;
    migrated.aowName = null;
  }
  if (migrated.aowName && !catalog.aowNames.includes(migrated.aowName)) {
    issues.push(`skill '${migrated.aowName}' no longer exists`);
    migrated.aowName = null;
  }
  return issues;
}

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
      standardMaxUpgrade: unifiedCaps || !row.isSomber ? row.upgrade : preset.request.standardMaxUpgrade,
      somberMaxUpgrade: unifiedCaps || row.isSomber ? row.upgrade : preset.request.somberMaxUpgrade,
      minStr: 0, minDex: 0, minInt: 0, minFai: 0, minArc: 0,
      lockStr: row.stats.strStat, lockDex: row.stats.dex, lockInt: row.stats.intStat,
      lockFai: row.stats.fai, lockArc: row.stats.arc,
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
