import { errorMessage } from "./format";
import type {
  AowActionDto, AowEffectDto, AowHitDto, AowRouteDto, CombatStateDto, DamageBreakdownDto, ScalingDto, SolvedBuildDto, StatusBuildupDto,
} from "./types";

// The one validator for every stored or imported build: saved builds, shared text, backups
// and Compare pins. It returns copies built from known keys only, so extra keys never reach
// state, undo history, native requests or later saves.
export function validateStoredBuild(value: unknown, policy: { standardMaxUpgrade: number; somberMaxUpgrade: number }, label = "build"): { value: SolvedBuildDto } | { error: string } {
  try {
    if (value === null) throw invalidPreset(`${label} must be a build`);
    const build = record(SOLVED_BUILD, "an object or null")(value, label);
    assertInteger(build.upgrade, `${label}.upgrade`, 0, build.isSomber ? policy.somberMaxUpgrade : policy.standardMaxUpgrade);
    return { value: build };
  } catch (error) {
    return { error: errorMessage(error) };
  }
}

type Reader<T> = (value: unknown, label: string) => T;
// One reader per DTO field: a field without one does not compile, and
// scripts/check-dto-contract.mjs compares each table with the native DTO.
type Fields<T> = { readonly [K in keyof T]-?: Reader<T[K]> };

const integer = (min: number, max: number): Reader<number> => (value, label) => {
  assertInteger(value, label, min, max);
  return value;
};
const nullableInteger = (min: number, max: number): Reader<number | null> => (value, label) => {
  assertNullableInteger(value, label, min, max);
  return value;
};
const text = (maxLength: number, allowEmpty = false): Reader<string> => (value, label) => {
  assertText(value, label, maxLength, allowEmpty);
  return value;
};
const nullableText = (maxLength: number): Reader<string | null> => (value, label) => {
  assertNullableText(value, label, maxLength);
  return value;
};
const boolean: Reader<boolean> = (value, label) => {
  assertBoolean(value, label);
  return value;
};
const finite: Reader<number> = (value, label) => {
  assertFinite(value, label);
  return value;
};
const optional = <T>(read: Reader<T>): Reader<T | undefined> => (value, label) => value === undefined ? undefined : read(value, label);
const arrayOf = <T>(read: Reader<T>, maxLength: number): Reader<T[]> => (value, label) => {
  assertArray(value, label, maxLength);
  return value.map((entry, index) => read(entry, `${label}[${index}]`));
};
// Copies the table's keys in table order, leaving out optional fields that are absent.
function record<T>(fields: Fields<T>, shape = "an object"): Reader<T> {
  return (value, label) => {
    if (!isRecord(value)) throw invalidPreset(`${label} must be ${shape}`);
    const copy: Record<string, unknown> = {};
    for (const [key, read] of Object.entries<Reader<unknown>>(fields)) {
      const field = read(value[key], `${label}.${key}`);
      if (field !== undefined) copy[key] = field;
    }
    return copy as T;
  };
}

const combatStats = (max: number) => record({
  strStat: integer(0, max), dex: integer(0, max), intStat: integer(0, max), fai: integer(0, max), arc: integer(0, max),
} satisfies Fields<CombatStateDto>);

const SCALING = {
  str: finite, dex: finite, int: finite, fai: finite, arc: finite,
} satisfies Fields<ScalingDto>;

const DAMAGE = record({
  physical: finite, magic: finite, fire: finite, lightning: finite, holy: finite, total: finite,
} satisfies Fields<DamageBreakdownDto>);

const STATUS = record({
  bleed: finite, frost: finite, poison: finite, scarletRot: finite, sleep: finite, madness: finite, death: finite,
} satisfies Fields<StatusBuildupDto>);

const AOW_EFFECT = {
  effectId: integer(0, 0xffff_ffff), effectName: text(500, true), role: text(100), activationTiming: text(100),
  isSupported: boolean, reason: text(1000, true), attackPower: DAMAGE, statusBuildup: STATUS,
} satisfies Fields<AowEffectDto>;

const AOW_HIT = {
  sheetRow: integer(0, 0xffff), hitOrder: integer(0, 0xffff), rawName: text(500, true), poiseDamage: finite,
  physicalAttackAttribute: text(80, true), buffActive: boolean, damage: DAMAGE, statusBuildup: STATUS,
  warnings: arrayOf(text(1000, true), 256), effects: arrayOf(record(AOW_EFFECT), 256),
} satisfies Fields<AowHitDto>;

const AOW_ACTION = {
  actionId: text(200), actionOrder: integer(0, 0xffff), staminaCost: finite, hits: arrayOf(record(AOW_HIT), 4096),
} satisfies Fields<AowActionDto>;

const AOW_ROUTE = {
  routeId: text(200), routeLabel: text(300), routePriority: integer(0, 0xffff), buffActivationActionId: nullableText(200),
  firstHitDamage: finite, totalPoiseDamage: finite, totalStaminaCost: finite,
  totalDamage: DAMAGE, totalStatusBuildup: STATUS, actions: arrayOf(record(AOW_ACTION), 512),
} satisfies Fields<AowRouteDto>;
const readAowRoute = record(AOW_ROUTE, "an object or null");

// Table order is the key order saved builds have always been written in.
const SOLVED_BUILD = {
  weaponId: integer(0, 0xffff_ffff), weaponName: text(200), weaponTypeName: optional(text(200, true)),
  affinity: text(80), isSomber: boolean, upgrade: integer(0, 25),
  aowId: nullableInteger(0, 0xffff), aowName: nullableText(200),
  bleedBuildup: finite, bleedBuildupAdd: finite, frostBuildup: finite, poisonBuildup: finite,
  scarletRotBuildup: finite, sleepBuildup: finite, madnessBuildup: finite, deathBuildup: finite,
  aowFirstHitDamage: finite, aowFullSequenceDamage: finite, score: finite,
  requirements: optional(combatStats(0xff)), effectiveScaling: optional(record(SCALING)),
  stats: combatStats(99), ar: DAMAGE,
  aowRoute: (value, label) => value === null ? null : readAowRoute(value, label),
} satisfies Fields<SolvedBuildDto>;

export function assertText(value: unknown, label: string, maxLength: number, allowEmpty = false): asserts value is string {
  if (typeof value !== "string" || value.length > maxLength || (!allowEmpty && !value.trim())) {
    throw invalidPreset(`${label} must be ${allowEmpty ? "a" : "a non-empty"} string no longer than ${maxLength} characters`);
  }
}

export function assertNullableText(value: unknown, label: string, maxLength: number): asserts value is string | null {
  if (value !== null) assertText(value, label, maxLength);
}

function assertFinite(value: unknown, label: string): asserts value is number {
  if (typeof value !== "number" || !Number.isFinite(value) || !Number.isFinite(Math.fround(value))) {
    throw invalidPreset(`${label} must be a finite native float`);
  }
}

export function assertInteger(value: unknown, label: string, min: number, max: number): asserts value is number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < min || value > max) {
    throw invalidPreset(`${label} must be an integer from ${min} through ${max}`);
  }
}

export function assertNullableInteger(value: unknown, label: string, min: number, max: number): asserts value is number | null {
  if (value !== null) assertInteger(value, label, min, max);
}

export function assertBoolean(value: unknown, label: string): asserts value is boolean {
  if (typeof value !== "boolean") throw invalidPreset(`${label} must be true or false`);
}

export function assertArray(value: unknown, label: string, maxLength: number): asserts value is unknown[] {
  if (!Array.isArray(value) || value.length > maxLength) throw invalidPreset(`${label} must be an array with at most ${maxLength} entries`);
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function invalidPreset(detail: string): Error {
  return new Error(detail);
}
