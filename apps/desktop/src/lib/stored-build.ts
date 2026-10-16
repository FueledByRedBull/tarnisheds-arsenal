import { errorMessage } from "./format";
import type { AowHitDto, AowRouteDto, CombatStateDto, DamageBreakdownDto, ScalingDto, SolvedBuildDto, StatusBuildupDto } from "./types";

// Validators return copies built from known keys only, so extra keys in stored or imported
// builds never reach state, undo history, native requests or later saves.
export function validateStoredBuild(value: unknown, policy: { standardMaxUpgrade: number; somberMaxUpgrade: number }, label = "build"): { value: SolvedBuildDto } | { error: string } {
  try {
    if (value === null) throw invalidPreset(`${label} must be a build`);
    const build = readSolvedBuild(value, label);
    assertInteger(build.upgrade, `${label}.upgrade`, 0, build.isSomber ? policy.somberMaxUpgrade : policy.standardMaxUpgrade);
    return { value: build };
  } catch (error) {
    return { error: errorMessage(error) };
  }
}

const BUILD_NUMBER_KEYS = [
  "bleedBuildup", "bleedBuildupAdd", "frostBuildup", "poisonBuildup",
  "scarletRotBuildup", "sleepBuildup", "madnessBuildup", "deathBuildup",
  "aowFirstHitDamage", "aowFullSequenceDamage", "score",
] as const;

function readSolvedBuild(value: unknown, label: string): SolvedBuildDto {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object or null`);
  assertInteger(value.weaponId, `${label}.weaponId`, 0, 0xffff_ffff);
  assertText(value.weaponName, `${label}.weaponName`, 200);
  if (value.weaponTypeName !== undefined) assertText(value.weaponTypeName, `${label}.weaponTypeName`, 200, true);
  const requirements = value.requirements === undefined ? undefined : readCombatStats(value.requirements, `${label}.requirements`, 0xff);
  let effectiveScaling: ScalingDto | undefined;
  if (value.effectiveScaling !== undefined) {
    if (!isRecord(value.effectiveScaling)) throw invalidPreset(`${label}.effectiveScaling must be an object`);
    const keys = ["str", "dex", "int", "fai", "arc"] as const;
    for (const key of keys) assertFinite(value.effectiveScaling[key], `${label}.effectiveScaling.${key}`);
    effectiveScaling = pick(value.effectiveScaling, keys) as ScalingDto;
  }
  assertText(value.affinity, `${label}.affinity`, 80);
  assertBoolean(value.isSomber, `${label}.isSomber`);
  assertInteger(value.upgrade, `${label}.upgrade`, 0, 25);
  const stats = readCombatStats(value.stats, `${label}.stats`);
  const ar = readDamage(value.ar, `${label}.ar`);
  assertNullableInteger(value.aowId, `${label}.aowId`, 0, 0xffff);
  assertNullableText(value.aowName, `${label}.aowName`, 200);
  for (const key of BUILD_NUMBER_KEYS) assertFinite(value[key], `${label}.${key}`);
  const aowRoute = readAowRoute(value.aowRoute, `${label}.aowRoute`);
  return {
    ...pick(value, ["weaponId", "weaponName", "weaponTypeName", "affinity", "isSomber", "upgrade", "aowId", "aowName", ...BUILD_NUMBER_KEYS]),
    ...(requirements && { requirements }),
    ...(effectiveScaling && { effectiveScaling }),
    stats, ar, aowRoute,
  } as SolvedBuildDto;
}

function readAowRoute(value: unknown, label: string): AowRouteDto | null {
  if (value === null) return null;
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object or null`);
  assertText(value.routeId, `${label}.routeId`, 200);
  assertText(value.routeLabel, `${label}.routeLabel`, 300);
  assertInteger(value.routePriority, `${label}.routePriority`, 0, 0xffff);
  assertNullableText(value.buffActivationActionId, `${label}.buffActivationActionId`, 200);
  assertFinite(value.firstHitDamage, `${label}.firstHitDamage`);
  const totalDamage = readDamage(value.totalDamage, `${label}.totalDamage`);
  assertFinite(value.totalPoiseDamage, `${label}.totalPoiseDamage`);
  const totalStatusBuildup = readStatus(value.totalStatusBuildup, `${label}.totalStatusBuildup`);
  assertFinite(value.totalStaminaCost, `${label}.totalStaminaCost`);
  assertArray(value.actions, `${label}.actions`, 512);
  const actions = value.actions.map((action, actionIndex) => {
    const actionLabel = `${label}.actions[${actionIndex}]`;
    if (!isRecord(action)) throw invalidPreset(`${actionLabel} must be an object`);
    assertText(action.actionId, `${actionLabel}.actionId`, 200);
    assertInteger(action.actionOrder, `${actionLabel}.actionOrder`, 0, 0xffff);
    assertFinite(action.staminaCost, `${actionLabel}.staminaCost`);
    assertArray(action.hits, `${actionLabel}.hits`, 4096);
    const hits = action.hits.map((hit, hitIndex) => readAowHit(hit, `${actionLabel}.hits[${hitIndex}]`));
    return { ...pick(action, ["actionId", "actionOrder", "staminaCost"]), hits };
  });
  return {
    ...pick(value, ["routeId", "routeLabel", "routePriority", "buffActivationActionId", "firstHitDamage", "totalPoiseDamage", "totalStaminaCost"]),
    totalDamage, totalStatusBuildup, actions,
  } as AowRouteDto;
}

function readAowHit(value: unknown, label: string): AowHitDto {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  assertInteger(value.sheetRow, `${label}.sheetRow`, 0, 0xffff);
  assertInteger(value.hitOrder, `${label}.hitOrder`, 0, 0xffff);
  assertText(value.rawName, `${label}.rawName`, 500, true);
  const damage = readDamage(value.damage, `${label}.damage`);
  assertFinite(value.poiseDamage, `${label}.poiseDamage`);
  const statusBuildup = readStatus(value.statusBuildup, `${label}.statusBuildup`);
  assertText(value.physicalAttackAttribute, `${label}.physicalAttackAttribute`, 80, true);
  assertBoolean(value.buffActive, `${label}.buffActive`);
  assertArray(value.warnings, `${label}.warnings`, 256);
  value.warnings.forEach((warning, index) => assertText(warning, `${label}.warnings[${index}]`, 1000, true));
  assertArray(value.effects, `${label}.effects`, 256);
  const effects = value.effects.map((effect, index) => {
    const effectLabel = `${label}.effects[${index}]`;
    if (!isRecord(effect)) throw invalidPreset(`${effectLabel} must be an object`);
    assertInteger(effect.effectId, `${effectLabel}.effectId`, 0, 0xffff_ffff);
    assertText(effect.effectName, `${effectLabel}.effectName`, 500, true);
    assertText(effect.role, `${effectLabel}.role`, 100);
    assertText(effect.activationTiming, `${effectLabel}.activationTiming`, 100);
    assertBoolean(effect.isSupported, `${effectLabel}.isSupported`);
    assertText(effect.reason, `${effectLabel}.reason`, 1000, true);
    const attackPower = readDamage(effect.attackPower, `${effectLabel}.attackPower`);
    const effectStatus = readStatus(effect.statusBuildup, `${effectLabel}.statusBuildup`);
    return {
      ...pick(effect, ["effectId", "effectName", "role", "activationTiming", "isSupported", "reason"]),
      attackPower, statusBuildup: effectStatus,
    };
  });
  return {
    ...pick(value, ["sheetRow", "hitOrder", "rawName", "poiseDamage", "physicalAttackAttribute", "buffActive"]),
    damage, statusBuildup, warnings: [...value.warnings], effects,
  } as AowHitDto;
}

function readCombatStats(value: unknown, label: string, max = 99): CombatStateDto {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  const keys = ["strStat", "dex", "intStat", "fai", "arc"] as const;
  for (const key of keys) assertInteger(value[key], `${label}.${key}`, 0, max);
  return pick(value, keys) as CombatStateDto;
}

function readDamage(value: unknown, label: string): DamageBreakdownDto {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  const keys = ["physical", "magic", "fire", "lightning", "holy", "total"] as const;
  for (const key of keys) assertFinite(value[key], `${label}.${key}`);
  return pick(value, keys) as DamageBreakdownDto;
}

function readStatus(value: unknown, label: string): StatusBuildupDto {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  const keys = ["bleed", "frost", "poison", "scarletRot", "sleep", "madness", "death"] as const;
  for (const key of keys) assertFinite(value[key], `${label}.${key}`);
  return pick(value, keys) as StatusBuildupDto;
}

// Copies only the named keys that are present; callers validate them first.
function pick<K extends string>(value: Record<string, unknown>, keys: readonly K[]): Record<K, unknown> {
  return Object.fromEntries(keys.filter((key) => value[key] !== undefined).map((key) => [key, value[key]])) as Record<K, unknown>;
}

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
