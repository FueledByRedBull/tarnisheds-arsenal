import type { CatalogDto, Notice, SolvedBuildDto } from "./types";

function compareBenchKey(profileId: string): string {
  return `tarnisheds-arsenal.compareBench.v1.${profileId}`;
}

export function readCompareBench(catalog: CatalogDto): { rows: SolvedBuildDto[]; notices: Notice[] } {
  const empty = { rows: [], notices: [] };
  if (typeof localStorage === "undefined") return empty;
  try {
    const raw = localStorage.getItem(compareBenchKey(catalog.dataManifest.profile.id));
    if (!raw) return empty;
    const value: unknown = JSON.parse(raw);
    if (!isRecord(value) || value.version !== 1 || value.datasetVersion !== catalog.dataManifest.datasetVersion
      || value.schemaVersion !== catalog.dataManifest.schemaVersion
      || value.modelVersion !== catalog.dataManifest.modelVersion || !Array.isArray(value.rows)) {
      return empty;
    }
    const rows = value.rows.filter((row): row is SolvedBuildDto => isStoredBuild(row, catalog));
    return {
      rows: rows.slice(0, 8),
      notices: rows.length === value.rows.length ? [] : [{
        scope: "global", tone: "warning", message: "Some saved comparison builds were discarded because their data is invalid.",
      }],
    };
  } catch {
    return empty;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isStoredInteger(value: unknown, max: number): boolean {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= max;
}

function isStoredFloat(value: unknown): boolean {
  // DTO floats are f32; allow the shortest JSON spelling of their rounded values.
  return typeof value === "number" && Number.isFinite(value) && Number.isFinite(Math.fround(value));
}

function isStoredStats(value: unknown, max: number): boolean {
  return isRecord(value) && ["strStat", "dex", "intStat", "fai", "arc"]
    .every((key) => isStoredInteger(value[key], max));
}

function isStoredDamage(value: unknown): boolean {
  return isRecord(value) && ["physical", "magic", "fire", "lightning", "holy", "total"]
    .every((key) => isStoredFloat(value[key]));
}

function isStoredStatus(value: unknown): boolean {
  return isRecord(value) && ["bleed", "frost", "poison", "scarletRot", "sleep", "madness", "death"]
    .every((key) => isStoredFloat(value[key]));
}

function isStoredEffect(value: unknown): boolean {
  return isRecord(value) && isStoredInteger(value.effectId, 0xffff_ffff)
    && ["effectName", "role", "activationTiming", "reason"].every((key) => typeof value[key] === "string")
    && typeof value.isSupported === "boolean"
    && isStoredDamage(value.attackPower) && isStoredStatus(value.statusBuildup);
}

function isStoredHit(value: unknown): boolean {
  return isRecord(value) && isStoredInteger(value.sheetRow, 0xffff) && isStoredInteger(value.hitOrder, 0xffff)
    && typeof value.rawName === "string" && typeof value.physicalAttackAttribute === "string"
    && isStoredDamage(value.damage) && isStoredFloat(value.poiseDamage) && isStoredStatus(value.statusBuildup)
    && typeof value.buffActive === "boolean"
    && Array.isArray(value.effects) && value.effects.every(isStoredEffect)
    && Array.isArray(value.warnings) && value.warnings.every((warning) => typeof warning === "string");
}

function isStoredAction(value: unknown): boolean {
  return isRecord(value) && typeof value.actionId === "string" && isStoredInteger(value.actionOrder, 0xffff)
    && isStoredFloat(value.staminaCost) && Array.isArray(value.hits) && value.hits.every(isStoredHit);
}

function isStoredRoute(value: unknown): boolean {
  return isRecord(value) && typeof value.routeId === "string" && typeof value.routeLabel === "string"
    && isStoredInteger(value.routePriority, 0xffff)
    && (value.buffActivationActionId === null || typeof value.buffActivationActionId === "string")
    && Array.isArray(value.actions) && value.actions.every(isStoredAction)
    && isStoredFloat(value.firstHitDamage) && isStoredDamage(value.totalDamage)
    && isStoredFloat(value.totalPoiseDamage) && isStoredStatus(value.totalStatusBuildup)
    && isStoredFloat(value.totalStaminaCost);
}

function isStoredBuild(build: unknown, catalog: CatalogDto): build is SolvedBuildDto {
  if (!isRecord(build)) return false;
  const scaling = build.effectiveScaling;
  return isStoredInteger(build.weaponId, 0xffff_ffff)
    && typeof build.weaponName === "string" && typeof build.affinity === "string"
    && typeof build.isSomber === "boolean"
    && isStoredInteger(build.upgrade, build.isSomber
      ? catalog.dataManifest.rules.somberMaxUpgrade : catalog.dataManifest.rules.standardMaxUpgrade)
    && isStoredStats(build.stats, 99)
    && (build.weaponTypeName === undefined || typeof build.weaponTypeName === "string")
    && (build.requirements === undefined || isStoredStats(build.requirements, 0xff))
    && (scaling === undefined || (isRecord(scaling)
      && ["str", "dex", "int", "fai", "arc"].every((key) => isStoredFloat(scaling[key]))))
    && isStoredDamage(build.ar)
    && (build.aowId === null || isStoredInteger(build.aowId, 0xffff))
    && (build.aowName === null || typeof build.aowName === "string")
    && ["bleedBuildup", "bleedBuildupAdd", "frostBuildup", "poisonBuildup", "scarletRotBuildup",
      "sleepBuildup", "madnessBuildup", "deathBuildup", "aowFirstHitDamage", "aowFullSequenceDamage", "score"]
      .every((key) => isStoredFloat(build[key]))
    && (build.aowRoute === null || isStoredRoute(build.aowRoute));
}

export function writeCompareBench(catalog: CatalogDto | null, rows: SolvedBuildDto[]): Notice[] {
  if (!catalog) return [];
  try {
    if (typeof localStorage === "undefined") return [];
    localStorage.setItem(compareBenchKey(catalog.dataManifest.profile.id), JSON.stringify({
      version: 1,
      datasetVersion: catalog.dataManifest.datasetVersion,
      schemaVersion: catalog.dataManifest.schemaVersion,
      modelVersion: catalog.dataManifest.modelVersion,
      rows,
    }));
    return [];
  } catch {
    return [{ scope: "global", tone: "warning", message: "Comparison changes could not be saved to device storage and may be lost after restarting. Your saved builds are unchanged." }];
  }
}
