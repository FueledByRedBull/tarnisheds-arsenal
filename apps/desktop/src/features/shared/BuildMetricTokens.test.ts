import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { SolvedBuildDto } from "../../lib/types";
import { DamageTokens, ScalingTokens, StatusTokens } from "./BuildMetricTokens";

const row: SolvedBuildDto = {
  weaponId: 1,
  weaponName: "Status Test",
  affinity: "Standard",
  isSomber: false,
  upgrade: 25,
  stats: { strStat: 10, dex: 10, intStat: 10, fai: 10, arc: 10 },
  effectiveScaling: { str: 0.4, dex: 0.8, int: 1.2, fai: 1.6, arc: 2.0 },
  ar: { physical: 100, magic: 20, fire: 30, lightning: 40, holy: 50, total: 240 },
  aowId: null,
  aowName: null,
  bleedBuildup: 11,
  bleedBuildupAdd: 0,
  frostBuildup: 22,
  poisonBuildup: 33,
  scarletRotBuildup: 44,
  sleepBuildup: 55,
  madnessBuildup: 66,
  deathBuildup: 77,
  aowFirstHitDamage: 0,
  aowFullSequenceDamage: 0,
  aowRoute: null,
  score: 100,
};

describe("build metric tokens", () => {
  it("renders every scaling attribute with an accessible full name", () => {
    const markup = renderToStaticMarkup(
      createElement(ScalingTokens, { scaling: row.effectiveScaling, extended: true }),
    );

    for (const name of ["Strength", "Dexterity", "Intelligence", "Faith", "Arcane"]) {
      expect(markup).toContain(`${name} scaling:`);
    }
    for (const short of ["STR", "DEX", "INT", "FAI", "ARC"]) {
      expect(markup).toContain(`>${short}<`);
    }
  });

  it("renders all seven status families, including explicit zero-capable fields", () => {
    const markup = renderToStaticMarkup(createElement(StatusTokens, { row }));

    for (const label of [
      "Bleed buildup: 11",
      "Frost buildup: 22",
      "Poison buildup: 33",
      "Scarlet Rot buildup: 44",
      "Sleep buildup: 55",
      "Madness buildup: 66",
      "Death Blight buildup: 77",
    ]) {
      expect(markup).toContain(label);
    }
  });

  it("renders every attack-rating damage component", () => {
    const markup = renderToStaticMarkup(createElement(DamageTokens, { ar: row.ar }));

    for (const label of [
      "Physical attack rating: 100",
      "Magic attack rating: 20",
      "Fire attack rating: 30",
      "Lightning attack rating: 40",
      "Holy attack rating: 50",
    ]) {
      expect(markup).toContain(label);
    }
  });
});
