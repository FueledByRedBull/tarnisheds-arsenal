import { OBJECTIVE_IDS, type DataManifestDto, type ObjectiveId, type ProfileCapabilitiesDto, type SolvedBuildDto } from "./types";

// What each ranking objective is called, what it measures, and which profile capabilities it
// needs. The core checks the same support rule before it searches (optimizer.rs, validate_request).

interface ObjectiveInfo {
  label: string;
  /** What the score counts, as "120 AR" or "120 damage". */
  unit: string;
  /** One line on what the objective ranks by. */
  hint: string;
  /** Extra words the command palette matches. */
  searchWords: string;
  /** The data the objective needs that these capabilities lack, worded as the core reports it; null when supported. */
  missingData: (capabilities: ProfileCapabilitiesDto) => string | null;
}

const STATUS_DATA = "status-buildup data";
const AOW_DAMAGE_DATA = "verified Ash of War damage data";
// The core names route data even when damage data is missing too.
const AOW_ROUTE_DATA = "verified Ash of War route data";

const OBJECTIVES = {
  max_ar: {
    label: "Max AR",
    unit: "AR",
    hint: "Highest total attack rating",
    searchWords: "max ar attack rating damage",
    missingData: () => null,
  },
  max_physical_ar: {
    label: "Max Physical AR",
    unit: "AR",
    hint: "Highest physical attack rating",
    searchWords: "max physical ar attack rating",
    missingData: () => null,
  },
  max_ar_plus_bleed: {
    label: "Bleed, then AR",
    unit: "bleed build-up",
    hint: "Most bleed buildup; AR breaks ties",
    searchWords: "bleed blood loss hemorrhage then ar",
    missingData: (capabilities) => (capabilities.statusBuildup ? null : STATUS_DATA),
  },
  aow_first_hit: {
    label: "AoW First Hit",
    unit: "damage",
    hint: "Hardest first damaging hit of the skill",
    searchWords: "aow ash of war skill first hit",
    missingData: (capabilities) => (capabilities.aowDamage ? null : AOW_DAMAGE_DATA),
  },
  aow_full_sequence: {
    label: "AoW Sequence",
    unit: "damage",
    hint: "Most damage over one full skill route",
    searchWords: "aow ash of war skill full sequence combo",
    missingData: (capabilities) => (capabilities.aowDamage && capabilities.aowRoutes ? null : AOW_ROUTE_DATA),
  },
} as const satisfies Record<ObjectiveId, ObjectiveInfo>;

export function objectiveLabel(objective: ObjectiveId): string {
  return OBJECTIVES[objective].label;
}

export function objectiveUnit(objective: ObjectiveId): string {
  return OBJECTIVES[objective].unit;
}

export function objectiveHint(objective: ObjectiveId): string {
  return OBJECTIVES[objective].hint;
}

export function objectiveSearchWords(objective: ObjectiveId): string {
  return OBJECTIVES[objective].searchWords;
}

/** The objectives a profile with these capabilities can rank by, in catalog order. */
export function supportedObjectives(capabilities: ProfileCapabilitiesDto): ObjectiveId[] {
  return OBJECTIVE_IDS.filter((objective) => objectiveSupported(objective, capabilities));
}

export function objectiveSupported(objective: ObjectiveId, capabilities: ProfileCapabilitiesDto): boolean {
  return objectiveMissingData(objective, capabilities) === null;
}

/** What a profile lacks to rank by this objective, as "verified Ash of War route data"; null when it has it. */
export function objectiveMissingData(objective: ObjectiveId, capabilities: ProfileCapabilitiesDto): string | null {
  return OBJECTIVES[objective].missingData(capabilities);
}

/** AoW damage is shown only when the profile models both skill damage and its routes. */
export function aowModelSupported(manifest: Pick<DataManifestDto, "capabilities"> | null | undefined): boolean {
  return Boolean(manifest && objectiveSupported("aow_full_sequence", manifest.capabilities));
}

export function hasAowDamage(row: Pick<SolvedBuildDto, "aowRoute">, modelSupported: boolean): boolean {
  return modelSupported && row.aowRoute !== null;
}

/** The build field an objective ranks by, read as it is. */
export function objectiveValue(row: SolvedBuildDto, objective: ObjectiveId): number {
  switch (objective) {
    case "max_ar":
      return row.ar.total;
    case "max_physical_ar":
      return row.ar.physical;
    case "max_ar_plus_bleed":
      return row.bleedBuildup;
    case "aow_first_hit":
      return row.aowFirstHitDamage;
    case "aow_full_sequence":
      return row.aowFullSequenceDamage;
    default:
      return unknownObjective(objective);
  }
}

/** The objective's value for display: null for skill damage the profile or the row does not model. */
export function metricForObjective(row: SolvedBuildDto, objective: ObjectiveId, aowModelSupported: boolean): number | null {
  switch (objective) {
    case "max_ar":
    case "max_physical_ar":
    case "max_ar_plus_bleed":
      return objectiveValue(row, objective);
    case "aow_first_hit":
    case "aow_full_sequence":
      return hasAowDamage(row, aowModelSupported) ? objectiveValue(row, objective) : null;
    default:
      return unknownObjective(objective);
  }
}

function unknownObjective(objective: never): never {
  throw new Error(`Unknown objective ${String(objective)}`);
}
