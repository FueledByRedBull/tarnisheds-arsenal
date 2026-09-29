import type { SolvedBuildDto } from "./types";

export function validateStoredBuild(value: unknown, policy: { standardMaxUpgrade: number; somberMaxUpgrade: number }, label = "build"): { value: SolvedBuildDto } | { error: { path: string; message: string } } {
  try {
    if (value === null) throw invalidPreset(`${label} must be a build`);
    assertSolvedBuild(value, label);
    if (!value) throw invalidPreset(`${label} must be a build`);
    assertInteger(value.upgrade, `${label}.upgrade`, 0, value.isSomber ? policy.somberMaxUpgrade : policy.standardMaxUpgrade);
    return { value };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { error: { path: message.split(" ")[0], message } };
  }
}

function assertSolvedBuild(value: unknown, label: string): asserts value is SolvedBuildDto | null {
  if (value === null) return;
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object or null`);
  assertInteger(value.weaponId, `${label}.weaponId`, 0, 0xffff_ffff);
  assertText(value.weaponName, `${label}.weaponName`, 200);
  if (value.weaponTypeName !== undefined) assertText(value.weaponTypeName, `${label}.weaponTypeName`, 200, true);
  if (value.requirements !== undefined) assertCombatStats(value.requirements, `${label}.requirements`, 0xff);
  if (value.effectiveScaling !== undefined) {
    if (!isRecord(value.effectiveScaling)) throw invalidPreset(`${label}.effectiveScaling must be an object`);
    for (const key of ["str", "dex", "int", "fai", "arc"] as const) {
      assertFinite(value.effectiveScaling[key], `${label}.effectiveScaling.${key}`);
    }
  }
  assertText(value.affinity, `${label}.affinity`, 80);
  assertBoolean(value.isSomber, `${label}.isSomber`);
  assertInteger(value.upgrade, `${label}.upgrade`, 0, 25);
  assertCombatStats(value.stats, `${label}.stats`);
  assertDamage(value.ar, `${label}.ar`);
  assertNullableInteger(value.aowId, `${label}.aowId`, 0, 0xffff);
  assertNullableText(value.aowName, `${label}.aowName`, 200);
  for (const key of [
    "bleedBuildup", "bleedBuildupAdd", "frostBuildup", "poisonBuildup",
    "scarletRotBuildup", "sleepBuildup", "madnessBuildup", "deathBuildup",
    "aowFirstHitDamage", "aowFullSequenceDamage", "score",
  ] as const) assertFinite(value[key], `${label}.${key}`);
  assertAowRoute(value.aowRoute, `${label}.aowRoute`);
}

function assertAowRoute(value: unknown, label: string) {
  if (value === null) return;
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object or null`);
  assertText(value.routeId, `${label}.routeId`, 200);
  assertText(value.routeLabel, `${label}.routeLabel`, 300);
  assertInteger(value.routePriority, `${label}.routePriority`, 0, 0xffff);
  assertNullableText(value.buffActivationActionId, `${label}.buffActivationActionId`, 200);
  assertFinite(value.firstHitDamage, `${label}.firstHitDamage`);
  assertDamage(value.totalDamage, `${label}.totalDamage`);
  assertFinite(value.totalPoiseDamage, `${label}.totalPoiseDamage`);
  assertStatus(value.totalStatusBuildup, `${label}.totalStatusBuildup`);
  assertFinite(value.totalStaminaCost, `${label}.totalStaminaCost`);
  assertArray(value.actions, `${label}.actions`, 512);
  value.actions.forEach((action, actionIndex) => {
    const actionLabel = `${label}.actions[${actionIndex}]`;
    if (!isRecord(action)) throw invalidPreset(`${actionLabel} must be an object`);
    assertText(action.actionId, `${actionLabel}.actionId`, 200);
    assertInteger(action.actionOrder, `${actionLabel}.actionOrder`, 0, 0xffff);
    assertFinite(action.staminaCost, `${actionLabel}.staminaCost`);
    assertArray(action.hits, `${actionLabel}.hits`, 4096);
    action.hits.forEach((hit, hitIndex) => assertAowHit(hit, `${actionLabel}.hits[${hitIndex}]`));
  });
}

function assertAowHit(value: unknown, label: string) {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  assertInteger(value.sheetRow, `${label}.sheetRow`, 0, 0xffff);
  assertInteger(value.hitOrder, `${label}.hitOrder`, 0, 0xffff);
  assertText(value.rawName, `${label}.rawName`, 500, true);
  assertDamage(value.damage, `${label}.damage`);
  assertFinite(value.poiseDamage, `${label}.poiseDamage`);
  assertStatus(value.statusBuildup, `${label}.statusBuildup`);
  assertText(value.physicalAttackAttribute, `${label}.physicalAttackAttribute`, 80, true);
  assertBoolean(value.buffActive, `${label}.buffActive`);
  assertArray(value.warnings, `${label}.warnings`, 256);
  value.warnings.forEach((warning, index) => assertText(warning, `${label}.warnings[${index}]`, 1000, true));
  assertArray(value.effects, `${label}.effects`, 256);
  value.effects.forEach((effect, index) => {
    const effectLabel = `${label}.effects[${index}]`;
    if (!isRecord(effect)) throw invalidPreset(`${effectLabel} must be an object`);
    assertInteger(effect.effectId, `${effectLabel}.effectId`, 0, 0xffff_ffff);
    assertText(effect.effectName, `${effectLabel}.effectName`, 500, true);
    assertText(effect.role, `${effectLabel}.role`, 100);
    assertText(effect.activationTiming, `${effectLabel}.activationTiming`, 100);
    assertBoolean(effect.isSupported, `${effectLabel}.isSupported`);
    assertText(effect.reason, `${effectLabel}.reason`, 1000, true);
    assertDamage(effect.attackPower, `${effectLabel}.attackPower`);
    assertStatus(effect.statusBuildup, `${effectLabel}.statusBuildup`);
  });
}

function assertCombatStats(value: unknown, label: string, max = 99) {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  for (const key of ["strStat", "dex", "intStat", "fai", "arc"] as const) {
    assertInteger(value[key], `${label}.${key}`, 0, max);
  }
}

function assertDamage(value: unknown, label: string) {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  for (const key of ["physical", "magic", "fire", "lightning", "holy", "total"] as const) {
    assertFinite(value[key], `${label}.${key}`);
  }
}

function assertStatus(value: unknown, label: string) {
  if (!isRecord(value)) throw invalidPreset(`${label} must be an object`);
  for (const key of ["bleed", "frost", "poison", "scarletRot", "sleep", "madness", "death"] as const) {
    assertFinite(value[key], `${label}.${key}`);
  }
}

function assertText(value: unknown, label: string, maxLength: number, allowEmpty = false): asserts value is string {
  if (typeof value !== "string" || value.length > maxLength || (!allowEmpty && !value.trim())) {
    throw invalidPreset(`${label} must be ${allowEmpty ? "a" : "a non-empty"} string no longer than ${maxLength} characters`);
  }
}

function assertNullableText(value: unknown, label: string, maxLength: number): asserts value is string | null {
  if (value !== null) assertText(value, label, maxLength);
}

function assertFinite(value: unknown, label: string): asserts value is number {
  if (typeof value !== "number" || !Number.isFinite(value) || !Number.isFinite(Math.fround(value))) {
    throw invalidPreset(`${label} must be a finite native float`);
  }
}

function assertInteger(value: unknown, label: string, min: number, max: number): asserts value is number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < min || value > max) {
    throw invalidPreset(`${label} must be an integer from ${min} through ${max}`);
  }
}

function assertNullableInteger(value: unknown, label: string, min: number, max: number): asserts value is number | null {
  if (value !== null) assertInteger(value, label, min, max);
}

function assertBoolean(value: unknown, label: string): asserts value is boolean {
  if (typeof value !== "boolean") throw invalidPreset(`${label} must be true or false`);
}

function assertArray(value: unknown, label: string, maxLength: number): asserts value is unknown[] {
  if (!Array.isArray(value) || value.length > maxLength) throw invalidPreset(`${label} must be an array with at most ${maxLength} entries`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function invalidPreset(detail: string): Error {
  return new Error(detail);
}
