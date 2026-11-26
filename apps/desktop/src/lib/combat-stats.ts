import type { CombatStateDto, EightStatsDto, OptimizeRequestDto, ScalingDto } from "./types";

// The character stats and how each one appears in a query: its label, its minimum and lock
// fields, and its scaling key. Callers iterate these tables instead of naming the fields.

export type CombatStatKey = keyof CombatStateDto;
export type EightStatKey = keyof EightStatsDto;

interface StatInfo {
  /** Short label, as the ribbon and stat lines show it. */
  label: string;
  name: string;
}

interface CombatStatInfo extends StatInfo {
  minimum: keyof OptimizeRequestDto;
  lock: keyof OptimizeRequestDto;
  scaling: keyof ScalingDto;
}

const COMBAT_STAT_INFO = {
  strStat: { label: "STR", name: "Strength", minimum: "minStr", lock: "lockStr", scaling: "str" },
  dex: { label: "DEX", name: "Dexterity", minimum: "minDex", lock: "lockDex", scaling: "dex" },
  intStat: { label: "INT", name: "Intelligence", minimum: "minInt", lock: "lockInt", scaling: "int" },
  fai: { label: "FAI", name: "Faith", minimum: "minFai", lock: "lockFai", scaling: "fai" },
  arc: { label: "ARC", name: "Arcane", minimum: "minArc", lock: "lockArc", scaling: "arc" },
} as const satisfies Record<CombatStatKey, CombatStatInfo>;

const FIXED_STAT_INFO = {
  vig: { label: "VIG", name: "Vigor" },
  mnd: { label: "MND", name: "Mind" },
  end: { label: "END", name: "Endurance" },
} as const satisfies Record<Exclude<EightStatKey, CombatStatKey>, StatInfo>;

export type MinimumKey = (typeof COMBAT_STAT_INFO)[CombatStatKey]["minimum"];
export type LockKey = (typeof COMBAT_STAT_INFO)[CombatStatKey]["lock"];

/** The five stats a search may redistribute, in display order. */
export const COMBAT_STAT_KEYS = ["strStat", "dex", "intStat", "fai", "arc"] as const satisfies readonly CombatStatKey[];
/** All eight stats in display order: the three fixed ones, then the combat stats. */
export const EIGHT_STAT_KEYS = ["vig", "mnd", "end", ...COMBAT_STAT_KEYS] as const satisfies readonly EightStatKey[];

export const COMBAT_STATS = COMBAT_STAT_KEYS.map((key) => ({ key, ...COMBAT_STAT_INFO[key] }));
export const EIGHT_STATS: ReadonlyArray<{ key: EightStatKey } & StatInfo> = EIGHT_STAT_KEYS.map((key) =>
  ({ key, ...(key in FIXED_STAT_INFO ? FIXED_STAT_INFO[key as keyof typeof FIXED_STAT_INFO] : COMBAT_STAT_INFO[key as CombatStatKey]) }));
export const MINIMUM_KEYS = COMBAT_STATS.map((stat) => stat.minimum);
export const LOCK_KEYS = COMBAT_STATS.map((stat) => stat.lock);

export function isCombatStat(key: EightStatKey): key is CombatStatKey {
  return key in COMBAT_STAT_INFO;
}

export function statLabel(key: EightStatKey): string {
  return isCombatStat(key) ? COMBAT_STAT_INFO[key].label : FIXED_STAT_INFO[key].label;
}

export function minimumKey(key: CombatStatKey): MinimumKey {
  return COMBAT_STAT_INFO[key].minimum;
}

export function lockKey(key: CombatStatKey): LockKey {
  return COMBAT_STAT_INFO[key].lock;
}

export function hasCombatStatLocks(request: Pick<OptimizeRequestDto, LockKey>): boolean {
  return LOCK_KEYS.some((key) => request[key] !== null);
}

export function allCombatStatsLocked(request: Pick<OptimizeRequestDto, LockKey>): boolean {
  return LOCK_KEYS.every((key) => request[key] !== null);
}

export function activeMinimumCount(request: Pick<OptimizeRequestDto, MinimumKey>): number {
  return MINIMUM_KEYS.filter((key) => request[key] > 0).length;
}

/** Every lock open, as a request patch. */
export function noStatLocks(): Pick<OptimizeRequestDto, LockKey> {
  return statLocksAt({ strStat: null, dex: null, intStat: null, fai: null, arc: null });
}

/** Every minimum at zero, as a request patch. */
export function noStatMinimums(): Pick<OptimizeRequestDto, MinimumKey> {
  return Object.fromEntries(MINIMUM_KEYS.map((key) => [key, 0])) as Pick<OptimizeRequestDto, MinimumKey>;
}

/** Locks each combat stat at the given value, as a request patch. */
export function statLocksAt(stats: Record<CombatStatKey, number | null>): Pick<OptimizeRequestDto, LockKey> {
  return Object.fromEntries(COMBAT_STATS.map((stat) => [stat.lock, stats[stat.key]])) as Pick<OptimizeRequestDto, LockKey>;
}

export function combatStatsOf(source: CombatStateDto): CombatStateDto {
  return Object.fromEntries(COMBAT_STAT_KEYS.map((key) => [key, source[key]])) as unknown as CombatStateDto;
}

export function sameCombatStats(left: CombatStateDto, right: CombatStateDto): boolean {
  return COMBAT_STAT_KEYS.every((key) => left[key] === right[key]);
}

/** "STR 12 / DEX 40 / INT 9 / FAI 8 / ARC 8". */
export function statLine(stats: CombatStateDto, separator = " / "): string {
  return COMBAT_STATS.map((stat) => `${stat.label} ${stats[stat.key]}`).join(separator);
}

/** The locked stats only, as "STR 12 / ARC 40". */
export function statLockLine(request: Pick<OptimizeRequestDto, LockKey>): string {
  return COMBAT_STATS.filter((stat) => request[stat.lock] !== null)
    .map((stat) => `${stat.label} ${request[stat.lock]}`)
    .join(" / ");
}
