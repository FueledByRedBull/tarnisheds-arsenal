import { expect, it } from "vitest";
import rustMath from "../../../../core/er_optimizer_core/src/math.rs?raw";
import {
  SCADUTREE_ATTACK_MULTIPLIERS,
  SCADUTREE_MAX_LEVEL,
  scadutreeAttackMultiplier,
  scadutreeDamageNegation,
  scadutreeReceivedDamageMultiplier,
} from "./scadutree";

it("uses the regulation attack rates for every Scadutree level", () => {
  // Vanilla regulation SpEffectParam 20000100..20000120,
  // atkEnemyDmgCorrectRate_Physics (also used by the other attack elements).
  const sourceRates = [1, 1.1, 1.2, 1.25, 1.3, 1.35, 1.425, 1.5, 1.55, 1.6, 1.65,
    1.75, 1.85, 1.875, 1.9, 1.925, 1.95, 1.975, 2, 2.025, 2.05];
  for (const [level, rate] of sourceRates.entries()) {
    expect(Math.fround(scadutreeAttackMultiplier(true, level)), `SpEffect ${20000100 + level}`).toBe(Math.fround(rate));
    expect(scadutreeReceivedDamageMultiplier(true, level)).toBeCloseTo(1 / rate, 7);
    expect(scadutreeDamageNegation(true, level)).toBeCloseTo(1 - 1 / rate, 7);
  }
  expect(scadutreeAttackMultiplier(false, 20)).toBe(1);
});

it("matches the Rust core's Scadutree table", () => {
  const maxLevel = rustMath.match(/SCADUTREE_MAX_LEVEL:\s*u8\s*=\s*(\d+)/)?.[1];
  const table = rustMath.match(/SCADUTREE_ATTACK_MULTIPLIERS:\s*\[f32;\s*21\]\s*=\s*\[([\s\S]*?)\];/)?.[1] ?? "";
  expect(Number(maxLevel)).toBe(SCADUTREE_MAX_LEVEL);
  expect(table.split(",").map((part) => part.trim()).filter(Boolean).map((part) => Number(part.replace(/_f32$/, ""))))
    .toEqual([...SCADUTREE_ATTACK_MULTIPLIERS]);
});
