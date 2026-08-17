import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import {
  SCADUTREE_ATTACK_MULTIPLIERS,
  SCADUTREE_MAX_LEVEL,
} from "../src/lib/scadutree";

test("Scadutree UI constants match Rust core constants", () => {
  const rustMath = readFileSync(
    new URL("../../../core/er_optimizer_core/src/math.rs", import.meta.url),
    "utf8",
  );

  const maxLevel = rustMath.match(/SCADUTREE_MAX_LEVEL:\s*u8\s*=\s*(\d+)/)?.[1];
  const multiplierBlock = rustMath.match(/SCADUTREE_ATTACK_MULTIPLIERS:\s*\[f32;\s*21\]\s*=\s*\[([\s\S]*?)\];/)?.[1];

  expect(Number(maxLevel)).toBe(SCADUTREE_MAX_LEVEL);
  expect(parseRustNumberList(multiplierBlock ?? "")).toEqual(Array.from(SCADUTREE_ATTACK_MULTIPLIERS));
});

function parseRustNumberList(block: string): number[] {
  return block
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => Number(part.replace(/_f32$/, "")));
}
