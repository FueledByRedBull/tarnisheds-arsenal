import {
  CatalogDto,
  ClassMetadataDto,
  EightStatsDto,
  OptimizeRequestDto,
  ProfileRulesDto,
  SolvedBuildDto,
} from "./types";

export const STAT_KEYS = ["strStat", "dex", "intStat", "fai", "arc"] as const;
export const EIGHT_STAT_KEYS = ["vig", "mnd", "end", ...STAT_KEYS] as const;
export type CombatStatKey = (typeof STAT_KEYS)[number];
export type EightStatKey = (typeof EIGHT_STAT_KEYS)[number];

type LegacyUpgradeRequest = Partial<OptimizeRequestDto> & {
  fixedUpgrade?: number | null;
  maxUpgrade?: number | null;
};

export const STARTING_CLASS_METADATA: ClassMetadataDto[] = [
  {
    name: "Vagabond",
    baseLevel: 9,
    baseTotal: 88,
    baseStats: { vig: 15, mnd: 10, end: 11, strStat: 14, dex: 13, intStat: 9, fai: 9, arc: 7 },
  },
  {
    name: "Warrior",
    baseLevel: 8,
    baseTotal: 87,
    baseStats: { vig: 11, mnd: 12, end: 11, strStat: 10, dex: 16, intStat: 10, fai: 8, arc: 9 },
  },
  {
    name: "Hero",
    baseLevel: 7,
    baseTotal: 86,
    baseStats: { vig: 14, mnd: 9, end: 12, strStat: 16, dex: 9, intStat: 7, fai: 8, arc: 11 },
  },
  {
    name: "Bandit",
    baseLevel: 5,
    baseTotal: 84,
    baseStats: { vig: 10, mnd: 11, end: 10, strStat: 9, dex: 13, intStat: 9, fai: 8, arc: 14 },
  },
  {
    name: "Astrologer",
    baseLevel: 6,
    baseTotal: 85,
    baseStats: { vig: 9, mnd: 15, end: 9, strStat: 8, dex: 12, intStat: 16, fai: 7, arc: 9 },
  },
  {
    name: "Prophet",
    baseLevel: 7,
    baseTotal: 86,
    baseStats: { vig: 10, mnd: 14, end: 8, strStat: 11, dex: 10, intStat: 7, fai: 16, arc: 10 },
  },
  {
    name: "Samurai",
    baseLevel: 9,
    baseTotal: 88,
    baseStats: { vig: 12, mnd: 11, end: 13, strStat: 12, dex: 15, intStat: 9, fai: 8, arc: 8 },
  },
  {
    name: "Prisoner",
    baseLevel: 9,
    baseTotal: 88,
    baseStats: { vig: 11, mnd: 12, end: 11, strStat: 11, dex: 14, intStat: 14, fai: 6, arc: 9 },
  },
  {
    name: "Confessor",
    baseLevel: 10,
    baseTotal: 89,
    baseStats: { vig: 10, mnd: 13, end: 10, strStat: 12, dex: 12, intStat: 9, fai: 14, arc: 9 },
  },
  {
    name: "Wretch",
    baseLevel: 1,
    baseTotal: 80,
    baseStats: { vig: 10, mnd: 10, end: 10, strStat: 10, dex: 10, intStat: 10, fai: 10, arc: 10 },
  },
  {
    name: "Idus Knight",
    baseLevel: 7,
    baseTotal: 86,
    baseStats: { vig: 10, mnd: 12, end: 11, strStat: 13, dex: 15, intStat: 8, fai: 11, arc: 6 },
  },
  {
    name: "Heavy Knight",
    baseLevel: 10,
    baseTotal: 89,
    baseStats: { vig: 14, mnd: 8, end: 17, strStat: 15, dex: 11, intStat: 7, fai: 8, arc: 9 },
  },
];

export function classOptions(catalog: CatalogDto | null): ClassMetadataDto[] {
  return catalog?.classes.length ? catalog.classes : STARTING_CLASS_METADATA;
}

export function classMeta(catalog: CatalogDto | null, className: string): ClassMetadataDto {
  const found = classOptions(catalog).find((entry) => entry.name === className);
  if (found) {
    return found;
  }
  throw new Error(`Unknown starting class: ${className}`);
}

export function currentStatTotal(request: OptimizeRequestDto): number {
  return EIGHT_STAT_KEYS.reduce((total, key) => total + Number(request[key]), 0);
}

export function derivedLevel(catalog: CatalogDto | null, request: OptimizeRequestDto): number {
  const meta = classMeta(catalog, request.className);
  return meta.baseLevel + (currentStatTotal(request) - meta.baseTotal);
}

export function startingClassLevel(meta: ClassMetadataDto, targets: EightStatsDto): number {
  return meta.baseLevel + EIGHT_STAT_KEYS.reduce(
    (levels, key) => levels + Math.max(targets[key] - meta.baseStats[key], 0),
    0,
  );
}

export function optimalStartingClass(
  catalog: CatalogDto | null,
  targets: EightStatsDto,
  currentClassName: string,
): ClassMetadataDto {
  return classOptions(catalog).reduce((best, candidate) => {
    const candidateLevel = startingClassLevel(candidate, targets);
    const bestLevel = startingClassLevel(best, targets);
    if (candidateLevel < bestLevel) return candidate;
    if (candidateLevel === bestLevel && candidate.name === currentClassName) return candidate;
    return best;
  });
}

export function budgetSnapshot(catalog: CatalogDto | null, request: OptimizeRequestDto) {
  const meta = classMeta(catalog, request.className);
  const level = derivedLevel(catalog, request);
  const total = meta.baseTotal + (level - meta.baseLevel);
  const floorSum =
    request.vig +
    request.mnd +
    request.end +
    Math.max(meta.baseStats.strStat, request.minStr) +
    Math.max(meta.baseStats.dex, request.minDex) +
    Math.max(meta.baseStats.intStat, request.minInt) +
    Math.max(meta.baseStats.fai, request.minFai) +
    Math.max(meta.baseStats.arc, request.minArc);
  return {
    level,
    baseLevel: meta.baseLevel,
    total,
    levelUps: Math.max(0, level - meta.baseLevel),
    redistributable: Math.max(0, total - floorSum),
    freePoints: total - currentStatTotal(request),
  };
}

export function buildOptimizeRequest(
  catalog: CatalogDto | null,
  uiRequest: OptimizeRequestDto,
  useLockedStats = true,
): OptimizeRequestDto {
  const meta = classMeta(catalog, uiRequest.className);
  return {
    ...uiRequest,
    characterLevel: derivedLevel(catalog, uiRequest),
    strStat: meta.baseStats.strStat,
    dex: meta.baseStats.dex,
    intStat: meta.baseStats.intStat,
    fai: meta.baseStats.fai,
    arc: meta.baseStats.arc,
    lockStr: useLockedStats ? uiRequest.lockStr : null,
    lockDex: useLockedStats ? uiRequest.lockDex : null,
    lockInt: useLockedStats ? uiRequest.lockInt : null,
    lockFai: useLockedStats ? uiRequest.lockFai : null,
    lockArc: useLockedStats ? uiRequest.lockArc : null,
  };
}

export function normalizeOptimizeRequest(
  raw: LegacyUpgradeRequest,
  fallback: OptimizeRequestDto,
): OptimizeRequestDto {
  const legacyMaxUpgrade = numberOrNull(raw.maxUpgrade);
  const {
    fixedUpgrade: _fixedUpgrade,
    maxUpgrade: _maxUpgrade,
    ...next
  } = raw;

  return {
    ...fallback,
    ...next,
    standardMaxUpgrade: clampUpgrade(
      numberOrNull(raw.standardMaxUpgrade) ?? legacyMaxUpgrade ?? fallback.standardMaxUpgrade,
      25,
    ),
    somberMaxUpgrade: clampUpgrade(
      numberOrNull(raw.somberMaxUpgrade) ?? (
        legacyMaxUpgrade === null ? fallback.somberMaxUpgrade : Math.min(legacyMaxUpgrade, 10)
      ),
      10,
    ),
    exactUpgrade: Boolean(raw.exactUpgrade ?? (raw.fixedUpgrade !== undefined && raw.fixedUpgrade !== null)),
  };
}

export function applyProfileRules(
  request: OptimizeRequestDto,
  rules: ProfileRulesDto | null | undefined,
  resetUpgradeCaps = false,
): OptimizeRequestDto {
  if (!rules) return request;
  return {
    ...request,
    standardMaxUpgrade: resetUpgradeCaps
      ? rules.standardMaxUpgrade
      : clampUpgrade(request.standardMaxUpgrade, rules.standardMaxUpgrade),
    somberMaxUpgrade: resetUpgradeCaps
      ? rules.somberMaxUpgrade
      : clampUpgrade(request.somberMaxUpgrade, rules.somberMaxUpgrade),
    somberFilter: rules.separateUpgradeCaps ? request.somberFilter : "all",
    dlcScaling: rules.scadutreeScaling ? request.dlcScaling : false,
    scadutreeLevel: rules.scadutreeScaling ? request.scadutreeLevel : 0,
  };
}

export function upgradeCapForRow(row: Pick<SolvedBuildDto, "isSomber">, request: OptimizeRequestDto): number {
  return row.isSomber ? request.somberMaxUpgrade : request.standardMaxUpgrade;
}

export function compareUpgradeHorizon(request: OptimizeRequestDto): number {
  return Math.max(request.standardMaxUpgrade, request.somberMaxUpgrade);
}

function numberOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function clampUpgrade(value: number, max: number): number {
  return Math.min(Math.max(Math.trunc(value), 0), max);
}

export function rowFingerprint(row: SolvedBuildDto | null): string | null {
  if (!row) {
    return null;
  }
  return [
    row.weaponId,
    row.weaponName.toLocaleLowerCase(),
    row.affinity.toLocaleLowerCase(),
    (row.aowName ?? "").toLocaleLowerCase(),
    row.upgrade,
    row.stats.strStat,
    row.stats.dex,
    row.stats.intStat,
    row.stats.fai,
    row.stats.arc,
  ].join("|");
}

export function remainingCombatLevels(request: OptimizeRequestDto): number {
  return STAT_KEYS.reduce((total, key) => total + Math.max(0, 99 - Number(request[key])), 0);
}

export function clampHorizon(request: OptimizeRequestDto, horizon: number): number {
  return Math.max(0, Math.min(200, horizon, remainingCombatLevels(request)));
}

export function stableSignature(value: unknown): string {
  return JSON.stringify(value, (_key, raw) => {
    if (raw && typeof raw === "object" && !Array.isArray(raw)) {
      return Object.fromEntries(Object.entries(raw).sort(([left], [right]) => left.localeCompare(right)));
    }
    return raw;
  });
}

export function scalingLetter(value: number, extended = false): string {
  if (value <= 0) return "-";
  if (extended && value >= 2.25) return "S++";
  if (extended && value >= 2.0) return "S+";
  if (value >= 1.75) return "S";
  if (value >= 1.4) return "A";
  if (value >= 0.9) return "B";
  if (value >= 0.6) return "C";
  if (value >= 0.25) return "D";
  return "E";
}
