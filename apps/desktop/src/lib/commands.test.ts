import { describe, expect, it } from "vitest";
import { CommandContext, findCommands, highlightRanges, matchScore, recallable } from "./commands";
import { STARTING_CLASS_METADATA } from "./session";
import { defaultRequest } from "./state";
import type { CatalogDto } from "./types";

function catalog(profileId: "vanilla" | "convergence" = "vanilla"): CatalogDto {
  const vanilla = profileId === "vanilla";
  return {
    weaponCount: 3,
    aowCount: 2,
    weaponNames: ["Uchigatana", "Nagakiba", "Zweihander"],
    weaponTypeKeys: ["katana", "colossal_sword"],
    classes: vanilla ? Object.values(STARTING_CLASS_METADATA) : [{ name: "Custom stats", baseLevel: 0, baseTotal: 0,
      baseStats: { vig: 0, mnd: 0, end: 0, strStat: 0, dex: 0, intStat: 0, fai: 0, arc: 0 } }],
    weaponTypeOptions: [{ key: "katana", label: "Katana" }],
    aowNames: ["Unsheathe", "Seppuku"],
    affinityNames: ["Standard", "Keen"],
    objectiveIds: ["max_ar", "max_ar_plus_bleed"],
    somberFilters: ["all"],
    filterDimensions: [
      { id: "weapon_type", label: "Weapon type", options: [{ id: "type:katana", label: "Katana", count: 2 }] },
      { id: "affinity", label: "Affinity", options: [{ id: "affinity:keen", label: "Keen", count: 3 }] },
    ],
    dataManifest: {
      schemaVersion: 4, datasetVersion: `${profileId}-test`, modelVersion: "test", id: profileId, label: profileId,
      appVersion: "1", source: "test", generatedAt: "2026-10-01", extractorVersion: "test", provenance: "test",
      profile: { id: profileId, displayName: vanilla ? "Vanilla" : "Convergence", gameVersion: "1.17", modVersion: vanilla ? null : "3.0.0.1" },
      capabilities: { classBudget: vanilla, weaponArForAmmunition: true, weaponAr: true, statusBuildup: true,
        weaponPassives: true, aowCompatibility: true, aowDamage: vanilla, aowRoutes: vanilla },
      rules: { standardMaxUpgrade: vanilla ? 25 : 15, somberMaxUpgrade: vanilla ? 10 : 15, separateUpgradeCaps: vanilla,
        scadutreeScaling: vanilla, zeroAttackElementUsesWeaponScaling: !vanilla, extendedScalingGrades: !vanilla,
        statusBuildupScales: vanilla },
    },
  } as CatalogDto;
}

function context(overrides: Partial<CommandContext> = {}): CommandContext {
  return {
    catalog: catalog(), request: defaultRequest, lockedStatMode: false, fixedStats: false, isSearching: false,
    resultsStale: false, hasRows: false, analysesAvailable: true,
    profiles: [{ id: "vanilla", label: "Vanilla 1.17" }, { id: "convergence", label: "Convergence 3.0.0.1" }],
    ...overrides,
  };
}

const ids = (query: string, ctx = context()) => findCommands(query, ctx).map((command) => command.id);

describe("palette level planning", () => {
  it("turns a target level into the Paths horizon from the derived level", () => {
    // Samurai's base stats are level 9.
    const [plan] = findCommands("level 50", context());
    expect(plan).toMatchObject({ id: "plan-levels", label: "Plan levels 9 to 50", action: { kind: "planLevels", horizon: 41 } });
    expect(recallable(plan)).toBe(false);
    expect(findCommands("level 5", context()).some((entry) => entry.id === "plan-levels")).toBe(false);
  });

  it("asks for a search first when no current selection can be traced", () => {
    expect(findCommands("lvl 60", context({ analysesAvailable: false }))[0]).toMatchObject({ id: "search", label: "Search first to plan levels to 60" });
  });
});

describe("palette parsing", () => {
  it("offers set, lock and minimum for a combat stat, led by the typed verb", () => {
    expect(ids("str 40").slice(0, 3)).toEqual(["set-strStat", "lock-strStat", "min-strStat"]);
    expect(ids("lock dex 30").slice(0, 3)).toEqual(["lock-dex", "set-dex", "min-dex"]);
    expect(ids("minimum arcane = 25").slice(0, 3)).toEqual(["min-arc", "set-arc", "lock-arc"]);
    const [set, lock, min] = findCommands("strength to 40", context());
    expect(set.action).toEqual({ kind: "patch", patch: { strStat: 40 } });
    expect(lock.action).toEqual({ kind: "lock", patch: { lockStr: 40 } });
    expect(min.action).toEqual({ kind: "patch", patch: { minStr: 40 } });
  });

  it("raises values below the class base to the base, like the ribbon inputs", () => {
    const [set, lock] = findCommands("str 3", context());
    expect(set.label).toBe("Set STR to 12");
    expect(set.detail).toBe("Samurai starts at 12");
    expect(lock.action).toEqual({ kind: "lock", patch: { lockStr: 12 } });
  });

  it("only sets non-combat stats and ignores impossible values", () => {
    expect(ids("vig 60")[0]).toBe("set-vig");
    expect(ids("vig 60")).not.toContain("lock-vig");
    expect(ids("str 100").filter((id) => id.endsWith("strStat"))).toEqual([]);
    expect(ids("str 0").filter((id) => id.endsWith("strStat"))).toEqual([]);
  });

  it("has no locks or minimums when stats are evaluated exactly", () => {
    const fixed = context({ catalog: catalog("convergence"), fixedStats: true,
      request: { ...defaultRequest, profileId: "convergence", className: "Custom stats" } });
    const commands = findCommands("str 40", fixed);
    expect(commands[0].action).toEqual({ kind: "patch", patch: { strStat: 40 } });
    expect(commands.map((command) => command.id)).not.toContain("lock-strStat");
    expect(findCommands("str 0", fixed).map((command) => command.id)).not.toContain("set-strStat");
    expect(ids("", fixed)).not.toContain("optimize-class");
  });

  it("parses separate and shared upgrade caps within each profile's limits", () => {
    expect(findCommands("+10", context()).slice(0, 2).map((command) => command.action)).toEqual([
      { kind: "patch", patch: { standardMaxUpgrade: 10 } },
      { kind: "patch", patch: { somberMaxUpgrade: 10 } },
    ]);
    expect(ids("+20")[0]).toBe("upgrade-standard");
    expect(ids("+20")).not.toContain("upgrade-somber");
    expect(ids("somber 5")[0]).toBe("upgrade-somber");
    const convergence = context({ catalog: catalog("convergence"), fixedStats: true,
      request: { ...defaultRequest, profileId: "convergence", className: "Custom stats" } });
    expect(findCommands("+15", convergence)[0].action).toEqual({ kind: "patch", patch: { standardMaxUpgrade: 15, somberMaxUpgrade: 15 } });
    expect(ids("+16", convergence)).not.toContain("upgrade-both");
  });

  it("parses result counts and Scadutree blessings only where they apply", () => {
    expect(findCommands("top 10", context())[0].action).toEqual({ kind: "patch", patch: { topK: 10 } });
    expect(ids("top 0")).not.toContain("top");
    expect(findCommands("blessing 20", context())[0].action).toEqual({ kind: "patch", patch: { dlcScaling: true, scadutreeLevel: 20 } });
    expect(ids("blessing 21")).not.toContain("blessing");
    const convergence = context({ catalog: catalog("convergence"), fixedStats: true,
      request: { ...defaultRequest, profileId: "convergence", className: "Custom stats" } });
    expect(ids("blessing 5", convergence)).not.toContain("blessing");
    expect(ids("dlc", convergence)).not.toContain("dlc-scaling");
  });
});

describe("palette search", () => {
  it("ranks prefix and word-prefix matches above loose matches", () => {
    expect(matchScore("uchi", "Weapon: Uchigatana Weapon")).toBe(80);
    expect(matchScore("weapon: u", "Weapon: Uchigatana")).toBe(100);
    expect(matchScore("gata", "Weapon: Uchigatana")).toBe(50);
    expect(matchScore("ugt", "Weapon: Uchigatana")).toBe(10);
    expect(matchScore("xyz", "Weapon: Uchigatana")).toBe(0);
    expect(ids("uchi")[0]).toBe("weapon-Uchigatana");
    expect(ids("naga")[0]).toBe("weapon-Nagakiba");
  });

  it("finds classes, skills, objectives, filters, workspaces and profiles", () => {
    expect(ids("vagabond")[0]).toBe("class-Vagabond");
    expect(ids("seppuku")[0]).toBe("aow-Seppuku");
    expect(ids("bleed")).toContain("objective-max_ar_plus_bleed");
    expect(ids("katana")).toContain("type-type:katana");
    expect(ids("keen")).toContain("affinity-affinity:keen");
    expect(ids("compare")).toContain("workspace-compare");
    expect(ids("convergence")).toContain("profile-convergence");
    expect(ids("vanilla")).not.toContain("profile-vanilla");
  });

  it("keeps the request's other filters when adding a type filter", () => {
    const request = { ...defaultRequest, filters: { version: 1 as const, entries: [{ dimension: "affinity" as const, id: "affinity:keen", mode: "include" as const }] } };
    const type = findCommands("katana", context({ request })).find((command) => command.id === "type-type:katana")!;
    expect(type.action).toEqual({ kind: "patch", patch: { weaponTypeKey: null, weaponName: null, aowName: null, filters: { version: 1, entries: [
      { dimension: "affinity", id: "affinity:keen", mode: "include" },
      { dimension: "weapon_type", id: "type:katana", mode: "include" },
    ] } } });
  });

  it("reflects current state in toggles and actions", () => {
    expect(ids("")[0]).toBe("search");
    expect(ids("", context({ isSearching: true }))[0]).toBe("cancel-search");
    expect(findCommands("", context({ resultsStale: true }))[0].label).toBe("Update results");
    expect(findCommands("two", context())[0].label).toBe("Two-hand weapons");
    expect(findCommands("one", context({ request: { ...defaultRequest, twoHanding: true } }))[0].label).toBe("One-hand weapons");
    expect(ids("clear locks")).not.toContain("clear-locks");
    expect(ids("clear locks", context({ request: { ...defaultRequest, lockStr: 40 } }))).toContain("clear-locks");
    expect(ids("go to", context({ analysesAvailable: false })).filter((id) => id.startsWith("workspace-"))).toEqual(["workspace-rankings"]);
  });

  it("limits results and rebuilds filter commands for each request", () => {
    expect(findCommands("a", context())).toHaveLength(8);
    expect(findCommands("a", context(), 3)).toHaveLength(3);
    const first = findCommands("katana", context()).find((command) => command.id === "type-type:katana")!;
    const changed = context({ request: { ...defaultRequest, filters: { version: 1, entries: [{ dimension: "affinity", id: "affinity:keen", mode: "exclude" }] } } });
    const second = findCommands("katana", changed).find((command) => command.id === "type-type:katana")!;
    expect(first.action).not.toEqual(second.action);
  });
});

describe("palette history, help and recall", () => {
  it("offers undo and redo with their descriptions only when they exist", () => {
    expect(ids("undo")).not.toContain("undo");
    const withHistory = context({ undoLabel: "STR 12 → 40", redoLabel: "DEX 15 → 20" });
    const undo = findCommands("undo", withHistory)[0];
    expect(undo).toMatchObject({ id: "undo", detail: "STR 12 → 40", action: { kind: "undo" } });
    expect(ids("redo", withHistory)[0]).toBe("redo");
    expect(ids("", withHistory)).toContain("undo");
    expect(ids("shortcuts")[0]).toBe("shortcuts");
  });

  it("leads the empty palette with recent commands that still apply", () => {
    const recent = context({ recent: ["weapon-Nagakiba", "weapon-Missing", "objective-max_ar_plus_bleed"] });
    const commands = findCommands("", recent);
    expect(commands.slice(0, 2).map((command) => [command.id, command.group])).toEqual([
      ["weapon-Nagakiba", "Recent"],
      ["objective-max_ar_plus_bleed", "Recent"],
    ]);
    expect(commands.map((command) => command.id)).toContain("search");
  });

  it("recalls named commands but not typed values or history steps", () => {
    const [set] = findCommands("str 40", context());
    expect(recallable(set)).toBe(false);
    expect(recallable(findCommands("uchi", context())[0])).toBe(true);
    expect(recallable(findCommands("stat locks", context())[0])).toBe(true);
    expect(recallable(findCommands("undo", context({ undoLabel: "x" }))[0])).toBe(false);
  });

  it("highlights each typed word once", () => {
    expect(highlightRanges("Set STR to 40", "str 40")).toEqual([[4, 7], [11, 13]]);
    expect(highlightRanges("Weapon: Uchigatana", "uchi")).toEqual([[8, 12]]);
    expect(highlightRanges("Weapon: Uchigatana", "a a")).toEqual([[2, 3], [13, 14]]);
    expect(highlightRanges("Search", "")).toEqual([]);
  });
});
