import { describe, expect, it } from "vitest";

import { buildOptimizeRequest, classMeta, derivedLevel, EIGHT_STAT_KEYS, optimalStartingClass, replaceFilterEntries, respecChanges, rowFingerprint, scalingLetter, stableSignature, STARTING_CLASS_METADATA, startingClassLevel } from "./session";
import { solvedBuild } from "./test-fixtures";
import { defaultRequest } from "./state";
import type { CatalogDto } from "./types";

describe("request normalization properties", () => {
  it("rejects unknown starting classes instead of using Vagabond", () => {
    expect(() => classMeta(null, "Unknown Class")).toThrow("Unknown starting class");
  });

  it("derives level from all eight entered stats", () => {
    expect(derivedLevel(null, defaultRequest)).toBe(9);
    expect(derivedLevel(null, { ...defaultRequest, vig: 60 })).toBe(57);
    expect(derivedLevel(null, { ...defaultRequest, dex: 99 })).toBe(93);
  });

  it("finds the lowest-level starting class for the requested stat floors", () => {
    const targets = { vig: 40, mnd: 15, end: 20, strStat: 16, dex: 12, intStat: 70, fai: 0, arc: 0 };
    const optimal = optimalStartingClass(null, targets, "Samurai");
    expect(optimal.name).toBe("Astrologer");
    expect(startingClassLevel(optimal, targets)).toBe(110);
  });

  it("exposes the two 1.17 Tarnished Pack class stat lines", () => {
    expect(classMeta(null, "Idus Knight")).toMatchObject({ baseLevel: 7, baseStats: { dex: 15, arc: 6 } });
    expect(classMeta(null, "Heavy Knight")).toMatchObject({ baseLevel: 10, baseStats: { vig: 14, end: 17 } });
  });

  it("checks every class against the independent total-stats minus 79 level rule", () => {
    for (const targets of [defaultRequest, { ...defaultRequest, intStat: 70, mnd: 38 },
      { ...defaultRequest, strStat: 1, dex: 99, arc: 45 }]) {
      const levels = STARTING_CLASS_METADATA.map((entry) => {
        const level = EIGHT_STAT_KEYS.reduce((sum, key) => sum + Math.max(entry.baseStats[key], targets[key]), 0) - 79;
        expect(startingClassLevel(entry, targets)).toBe(level);
        return level;
      });
      expect(startingClassLevel(optimalStartingClass(null, targets, "Samurai"), targets)).toBe(Math.min(...levels));
    }
  });

  it("builds a full-range Convergence custom-stat request", () => {
    const catalog = {
      classes: [{
        name: "Custom stats",
        baseLevel: 0,
        baseTotal: 0,
        baseStats: { vig: 0, mnd: 0, end: 0, strStat: 0, dex: 0, intStat: 0, fai: 0, arc: 0 },
      }],
      dataManifest: { capabilities: { classBudget: false } },
    } as unknown as CatalogDto;
    const request = {
      ...defaultRequest,
      profileId: "convergence",
      className: "Custom stats",
      vig: 99,
      mnd: 99,
      end: 99,
      strStat: 99,
      dex: 99,
      intStat: 99,
      fai: 99,
      arc: 99,
    };

    expect(buildOptimizeRequest(catalog, request)).toMatchObject({
      characterLevel: 792,
      lockStr: 99,
      lockDex: 99,
      lockInt: 99,
      lockFai: 99,
      lockArc: 99,
    });
  });

  it("uses Convergence S+ and S++ grades without changing Vanilla grades", () => {
    expect(scalingLetter(1.99, true)).toBe("S");
    expect(scalingLetter(2.0, true)).toBe("S+");
    expect(scalingLetter(2.249, true)).toBe("S+");
    expect(scalingLetter(2.25, true)).toBe("S++");
    expect(scalingLetter(2.277, true)).toBe("S++");
    expect(scalingLetter(2.277)).toBe("S");
  });

  it("replaces one filter dimension while preserving the others", () => {
    const entries = [
      { dimension: "coverage" as const, id: "coverage:weapon-ar", mode: "include" as const },
      { dimension: "weapon_type" as const, id: "weapon-type:old", mode: "include" as const },
      { dimension: "affinity" as const, id: "affinity:old", mode: "exclude" as const },
    ];

    expect(replaceFilterEntries(entries, "weapon_type", ["weapon-type:new"], ["weapon-type:blocked"])).toEqual([
      entries[0],
      { dimension: "affinity", id: "affinity:old", mode: "exclude" },
      { dimension: "weapon_type", id: "weapon-type:new", mode: "include" },
      { dimension: "weapon_type", id: "weapon-type:blocked", mode: "exclude" },
    ]);
  });
});

describe("result identity", () => {
  it("fingerprints every identity field and ignores letter case", () => {
    const base = solvedBuild();
    const changed = [
      { weaponId: 2 }, { weaponName: "Nagakiba" }, { affinity: "Blood" }, { aowName: "Seppuku" }, { upgrade: 24 },
      ...(["strStat", "dex", "intStat", "fai", "arc"] as const).map((key) => ({ stats: { ...base.stats, [key]: base.stats[key] + 1 } })),
    ];
    for (const change of changed) expect(rowFingerprint(solvedBuild(change))).not.toBe(rowFingerprint(base));
    expect(rowFingerprint(solvedBuild({ weaponName: "UCHIGATANA", affinity: "keen", aowName: "UNSHEATHE" }))).toBe(rowFingerprint(base));
    expect(rowFingerprint(solvedBuild({ score: 1, ar: { ...base.ar, total: 1 } }))).toBe(rowFingerprint(base));
  });

  it("signs equal requests equally whatever their key order", () => {
    const reordered = Object.fromEntries(Object.entries(defaultRequest).reverse());
    expect(stableSignature(reordered)).toBe(stableSignature(defaultRequest));
    expect(stableSignature({ ...defaultRequest, dex: defaultRequest.dex + 1 })).not.toBe(stableSignature(defaultRequest));
    expect(stableSignature({ ...defaultRequest, profileId: "convergence" })).not.toBe(stableSignature(defaultRequest));
  });
});

describe("stat changes", () => {
  const entered = { ...defaultRequest, strStat: 12, dex: 15, intStat: 9, fai: 8, arc: 8 };

  it("lists each combat stat the build moves, in level-up order", () => {
    expect(respecChanges(solvedBuild({ stats: { strStat: 12, dex: 22, intStat: 9, fai: 8, arc: 60 } }), entered))
      .toEqual(["DEX 15 → 22", "ARC 8 → 60"]);
  });

  it("is empty when the build uses the stats as entered", () => {
    expect(respecChanges(solvedBuild({ stats: { strStat: 12, dex: 15, intStat: 9, fai: 8, arc: 8 } }), entered)).toEqual([]);
  });
});
