import { EIGHT_STAT_KEYS } from "./combat-stats";
import { errorMessage } from "./format";
import { objectiveLabel } from "./objectives";
import { assertRequest, knownRequestFields } from "./presets";
import { applyProfileRules, classMeta, DEFAULT_CLASS_NAME } from "./session";
import type { CatalogDto, ClassMetadataDto, OptimizeRequestDto } from "./types";

// One pipeline for every stored request: the session query saved per profile, and the inputs
// of a saved build when it is loaded or migrated. Each step either keeps a value, repairs it
// and says how, or (for saved builds) rejects the whole request.

/** Why a stored request was not applied at all. */
export type RestoreRejection = "invalid" | "other-profile" | "unknown-class";

/** Which part of a stored request restore changed to fit the loaded profile. */
export type RestoreRepairKind =
  | "class" | "stats" | "weapon-type" | "weapon" | "affinity" | "skill" | "filters" | "objective" | "profile-rules";

export interface RestoreRepair {
  kind: RestoreRepairKind;
  /** What changed, worded to follow "adjusted to the current game data:". */
  detail: string;
}

export type RestoreOutcome =
  | { ok: true; request: OptimizeRequestDto; repairs: RestoreRepair[] }
  | { ok: false; reason: RestoreRejection; message: string };

/** Checks a stored request's shape and profile, and keeps only known request fields. */
export function parseStoredRequest(value: unknown, profileId: string):
  { ok: true; request: OptimizeRequestDto } | { ok: false; reason: "invalid" | "other-profile"; message: string } {
  try {
    assertRequest(value);
  } catch (error) {
    return { ok: false, reason: "invalid", message: errorMessage(error) };
  }
  if (value.profileId !== profileId) {
    return { ok: false, reason: "other-profile", message: `This build belongs to ${value.profileId}. Switch game profiles before loading it.` };
  }
  return { ok: true, request: knownRequestFields(value) };
}

/**
 * A saved build's inputs, for the loaded profile. A class the profile does not have rejects
 * the build: a saved build is never silently turned into another class's build.
 */
export function restoreSavedRequest(value: unknown, catalog: CatalogDto): RestoreOutcome {
  const parsed = parseStoredRequest(value, catalog.dataManifest.profile.id);
  if (parsed.ok === false) return parsed;
  const classInfo = findClass(catalog, parsed.request.className);
  if (!classInfo) {
    return { ok: false, reason: "unknown-class",
      message: `Starting class '${parsed.request.className}' is unavailable in this profile. Correct the saved inputs before loading; the original record is unchanged.` };
  }
  return { ok: true, ...fitToCatalog(parsed.request, catalog, classInfo, []) };
}

/**
 * The session query, for the loaded profile. It is always applied: a class the profile does
 * not have is replaced by the profile's default class at its starting stats, and says so.
 */
export function restoreSessionQuery(request: OptimizeRequestDto, catalog: CatalogDto): { request: OptimizeRequestDto; repairs: RestoreRepair[] } {
  const classInfo = findClass(catalog, request.className);
  if (classInfo) return fitToCatalog(request, catalog, classInfo, []);
  const replacement = defaultClass(catalog);
  return fitToCatalog(withClass(request, replacement, catalog), catalog, replacement, [
    { kind: "class", detail: `starting class '${request.className}' is unavailable, so the query starts from ${replacement.name}` },
  ]);
}

/** A profile's first query: its default class at that class's starting stats, under the profile's rules. */
export function startingRequest(request: OptimizeRequestDto, catalog: CatalogDto): OptimizeRequestDto {
  const classInfo = findClass(catalog, request.className) ?? defaultClass(catalog);
  return applyProfileRules(withClass(request, classInfo, catalog), catalog.dataManifest.rules);
}

/**
 * The settings a first visit to another profile keeps: how weapons are held and upgraded,
 * the blessing, and how results are listed. Everything about the build itself starts fresh.
 */
export function carriedHandling(request: OptimizeRequestDto):
  Pick<OptimizeRequestDto, "twoHanding" | "exactUpgrade" | "dlcScaling" | "scadutreeLevel" | "resultGrouping" | "topK"> {
  const { twoHanding, exactUpgrade, dlcScaling, scadutreeLevel, resultGrouping, topK } = request;
  return { twoHanding, exactUpgrade, dlcScaling, scadutreeLevel, resultGrouping, topK };
}

/** The repairs as one clause list, for a notice. */
export function repairSummary(repairs: RestoreRepair[]): string {
  return repairs.map((repair) => repair.detail).join("; ");
}

// Convergence class names now come from the game's menu text; earlier saves hold the data row names.
const RENAMED_CLASSES: Record<string, Record<string, string>> = {
  convergence: {
    "storm caller": "Stormcaller",
    "retainer alt (tarnished edition)": "Retainer (Alt)",
    "dreadnought alt (tarnished edition)": "Dreadnought (Alt)",
  },
};

// An exact name wins; otherwise one unambiguous case-insensitive match is canonicalized.
function findClass(catalog: CatalogDto, className: string): ClassMetadataDto | null {
  const current = RENAMED_CLASSES[catalog.dataManifest.profile.id]?.[className.toLowerCase()] ?? className;
  const exact = catalog.classes.find((entry) => entry.name === current);
  if (exact) return exact;
  const matches = catalog.classes.filter((entry) => entry.name.toLowerCase() === current.toLowerCase());
  return matches.length === 1 ? matches[0] : null;
}

function defaultClass(catalog: CatalogDto): ClassMetadataDto {
  return catalog.classes.find((entry) => entry.name === DEFAULT_CLASS_NAME) ?? catalog.classes[0] ?? classMeta(null, DEFAULT_CLASS_NAME);
}

// Without a class budget the level comes from the entered stats, so they are kept.
function withClass(request: OptimizeRequestDto, classInfo: ClassMetadataDto, catalog: CatalogDto): OptimizeRequestDto {
  return {
    ...request,
    className: classInfo.name,
    ...(catalog.dataManifest.capabilities.classBudget ? { characterLevel: classInfo.baseLevel, ...classInfo.baseStats } : {}),
  };
}

function fitToCatalog(
  request: OptimizeRequestDto,
  catalog: CatalogDto,
  classInfo: ClassMetadataDto,
  repairs: RestoreRepair[],
): { request: OptimizeRequestDto; repairs: RestoreRepair[] } {
  let fitted: OptimizeRequestDto = { ...request, className: classInfo.name };
  // Profiles can share a class name with different starting stats (Prisoner), and a data
  // revision can raise them; stats below the class's own restart from it.
  if (catalog.dataManifest.capabilities.classBudget && EIGHT_STAT_KEYS.some((key) => fitted[key] < classInfo.baseStats[key])) {
    fitted = withClass(fitted, classInfo, catalog);
    repairs.push({ kind: "stats", detail: `stats below ${classInfo.name}'s starting stats were reset to them` });
  }
  repairs.push(...clearMissingNames(fitted, catalog));
  // Coverage filters are not catalog dimensions; native validation checks them.
  const entries = fitted.filters.entries.filter((entry) => entry.dimension === "coverage" || catalog.filterDimensions
    .some((dimension) => dimension.id === entry.dimension && dimension.options.some((option) => option.id === entry.id)));
  const dropped = fitted.filters.entries.length - entries.length;
  if (dropped) {
    repairs.push({ kind: "filters", detail: dropped === 1 ? "1 filter no longer exists" : `${dropped} filters no longer exist` });
    fitted.filters = { ...fitted.filters, entries };
  }
  if (!catalog.objectiveIds.includes(fitted.objective)) {
    const objective = catalog.objectiveIds[0] ?? "max_ar";
    repairs.push({ kind: "objective",
      detail: `${objectiveLabel(fitted.objective)} is not available in this profile, so results rank by ${objectiveLabel(objective)}` });
    fitted.objective = objective;
  }
  const ruled = applyProfileRules(fitted, catalog.dataManifest.rules);
  repairs.push(...ruleRepairs(fitted, ruled, catalog.dataManifest.rules.separateUpgradeCaps));
  return { request: ruled, repairs };
}

// Edits the request it is given, which must be a copy.
function clearMissingNames(request: OptimizeRequestDto, catalog: CatalogDto): RestoreRepair[] {
  const repairs: RestoreRepair[] = [];
  // Older builds stored the type's label; newer ones its key. Either still filters correctly.
  if (request.weaponTypeKey && !catalog.weaponTypeOptions.some((option) => option.key === request.weaponTypeKey || option.label === request.weaponTypeKey)) {
    repairs.push({ kind: "weapon-type", detail: `weapon type '${request.weaponTypeKey}' no longer exists` });
    request.weaponTypeKey = null;
  }
  if (request.weaponName && !catalog.weaponNames.includes(request.weaponName)) {
    repairs.push({ kind: "weapon", detail: `weapon '${request.weaponName}' no longer exists` });
    request.weaponName = null;
    request.affinity = null;
    request.aowName = null;
  }
  if (request.affinity && !catalog.affinityNames.includes(request.affinity)) {
    repairs.push({ kind: "affinity", detail: `affinity '${request.affinity}' no longer exists` });
    request.affinity = null;
    request.aowName = null;
  }
  if (request.aowName && !catalog.aowNames.includes(request.aowName)) {
    repairs.push({ kind: "skill", detail: `skill '${request.aowName}' no longer exists` });
    request.aowName = null;
  }
  return repairs;
}

function ruleRepairs(before: OptimizeRequestDto, after: OptimizeRequestDto, separateUpgradeCaps: boolean): RestoreRepair[] {
  const details = [
    after.standardMaxUpgrade !== before.standardMaxUpgrade
      ? `${separateUpgradeCaps ? "Standard upgrade" : "upgrade"} cap lowered to +${after.standardMaxUpgrade}` : null,
    // A single-path profile caps every weapon with the standard field, so its Somber field is not shown.
    separateUpgradeCaps && after.somberMaxUpgrade !== before.somberMaxUpgrade ? `Somber upgrade cap lowered to +${after.somberMaxUpgrade}` : null,
    after.somberFilter !== before.somberFilter ? "the Somber filter was cleared, since this profile has one upgrade path" : null,
    after.dlcScaling !== before.dlcScaling || after.scadutreeLevel !== before.scadutreeLevel
      ? "Scadutree blessing was turned off, since this profile has none" : null,
  ];
  return details.filter((detail) => detail !== null).map((detail) => ({ kind: "profile-rules" as const, detail }));
}
