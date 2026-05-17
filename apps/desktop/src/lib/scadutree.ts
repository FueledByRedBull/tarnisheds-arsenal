export const SCADUTREE_MAX_LEVEL = 20;

export const SCADUTREE_ATTACK_MULTIPLIERS = [
  1.0, 1.1, 1.2, 1.25, 1.3, 1.35, 1.42, 1.5, 1.55, 1.6, 1.65, 1.75, 1.85, 1.87, 1.9,
  1.92, 1.95, 1.97, 2.0, 2.02, 2.05,
] as const;

export function scadutreeAttackMultiplier(dlcScaling: boolean, scadutreeLevel: number): number {
  if (!dlcScaling) return 1;
  const level = Math.max(0, Math.min(SCADUTREE_MAX_LEVEL, Math.trunc(scadutreeLevel)));
  return SCADUTREE_ATTACK_MULTIPLIERS[level];
}

export function scadutreeReceivedDamageMultiplier(dlcScaling: boolean, scadutreeLevel: number): number {
  return 1 / scadutreeAttackMultiplier(dlcScaling, scadutreeLevel);
}

export function scadutreeDamageNegation(dlcScaling: boolean, scadutreeLevel: number): number {
  return 1 - scadutreeReceivedDamageMultiplier(dlcScaling, scadutreeLevel);
}
