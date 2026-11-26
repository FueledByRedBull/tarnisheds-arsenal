import { describe, expect, it } from "vitest";

import {
  activeMinimumCount,
  allCombatStatsLocked,
  COMBAT_STAT_KEYS,
  EIGHT_STATS,
  hasCombatStatLocks,
  lockKey,
  minimumKey,
  noStatLocks,
  noStatMinimums,
  sameCombatStats,
  statLabel,
  statLine,
  statLockLine,
  statLocksAt,
} from "./combat-stats";
import { defaultRequest } from "./state";

const stats = { strStat: 12, dex: 40, intStat: 9, fai: 8, arc: 7 };

describe("combat stat vocabulary", () => {
  it("lists all eight stats in display order with their labels", () => {
    expect(EIGHT_STATS.map((stat) => stat.label)).toEqual(["VIG", "MND", "END", "STR", "DEX", "INT", "FAI", "ARC"]);
    expect(statLabel("mnd")).toBe("MND");
    expect(statLabel("intStat")).toBe("INT");
  });

  it("names every combat stat's minimum and lock field of the request", () => {
    for (const key of COMBAT_STAT_KEYS) {
      expect(defaultRequest).toHaveProperty(minimumKey(key));
      expect(defaultRequest).toHaveProperty(lockKey(key));
    }
    expect(minimumKey("arc")).toBe("minArc");
    expect(lockKey("strStat")).toBe("lockStr");
  });

  it("writes stat lines in display order", () => {
    expect(statLine(stats)).toBe("STR 12 / DEX 40 / INT 9 / FAI 8 / ARC 7");
    expect(statLine(stats, " ")).toBe("STR 12 DEX 40 INT 9 FAI 8 ARC 7");
  });

  it("builds lock and minimum patches and reads them back", () => {
    expect(noStatLocks()).toEqual({ lockStr: null, lockDex: null, lockInt: null, lockFai: null, lockArc: null });
    expect(noStatMinimums()).toEqual({ minStr: 0, minDex: 0, minInt: 0, minFai: 0, minArc: 0 });
    const locked = { ...defaultRequest, ...statLocksAt(stats) };
    expect(locked).toMatchObject({ lockStr: 12, lockDex: 40, lockInt: 9, lockFai: 8, lockArc: 7 });
    expect(allCombatStatsLocked(locked)).toBe(true);

    const partial = { ...defaultRequest, ...noStatLocks(), lockArc: 40 };
    expect(hasCombatStatLocks(partial)).toBe(true);
    expect(allCombatStatsLocked(partial)).toBe(false);
    expect(statLockLine(partial)).toBe("ARC 40");
    expect(hasCombatStatLocks({ ...defaultRequest, ...noStatLocks() })).toBe(false);

    expect(activeMinimumCount({ ...defaultRequest, ...noStatMinimums(), minDex: 20, minFai: 1 })).toBe(2);
  });

  it("compares the five combat stats only", () => {
    expect(sameCombatStats(stats, { ...stats })).toBe(true);
    expect(sameCombatStats(stats, { ...stats, arc: 8 })).toBe(false);
  });
});
