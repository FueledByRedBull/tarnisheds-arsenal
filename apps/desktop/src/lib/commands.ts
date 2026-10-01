import { objectiveLabel } from "./format";
import { SCADUTREE_MAX_LEVEL } from "./scadutree";
import { classMeta, replaceFilterEntries } from "./session";
import { CatalogDto, ObjectiveId, OptimizeRequestDto, WorkspaceTab } from "./types";

// The command palette's vocabulary. Parsing is pure: it returns data describing what to do,
// and the palette applies it through the same store actions the visible controls use.

export type CommandAction =
  | { kind: "patch"; patch: Partial<OptimizeRequestDto> }
  | { kind: "lock"; patch: Partial<OptimizeRequestDto> }
  | { kind: "class"; className: string }
  | { kind: "weapon"; weaponName: string }
  | { kind: "lockedMode"; value: boolean }
  | { kind: "clearLocks" }
  | { kind: "workspace"; workspace: WorkspaceTab }
  | { kind: "profile"; profileId: string }
  | { kind: "search" }
  | { kind: "cancelSearch" }
  | { kind: "optimizeClass" }
  | { kind: "resetStats" }
  | { kind: "resetFilters" };

export interface Command {
  id: string;
  group: string;
  label: string;
  detail?: string;
  action: CommandAction;
}

export interface CommandContext {
  catalog: CatalogDto;
  request: OptimizeRequestDto;
  lockedStatMode: boolean;
  fixedStats: boolean;
  isSearching: boolean;
  resultsStale: boolean;
  hasRows: boolean;
  analysesAvailable: boolean;
  profiles: Array<{ id: string; label: string }>;
}

type StatKey = "vig" | "mnd" | "end" | "strStat" | "dex" | "intStat" | "fai" | "arc";
type CombatKey = "strStat" | "dex" | "intStat" | "fai" | "arc";

const STATS: Array<{ key: StatKey; short: string; names: string[] }> = [
  { key: "vig", short: "VIG", names: ["vig", "vigor", "vigour"] },
  { key: "mnd", short: "MND", names: ["mnd", "mind"] },
  { key: "end", short: "END", names: ["end", "endurance"] },
  { key: "strStat", short: "STR", names: ["str", "strength"] },
  { key: "dex", short: "DEX", names: ["dex", "dexterity"] },
  { key: "intStat", short: "INT", names: ["int", "intelligence"] },
  { key: "fai", short: "FAI", names: ["fai", "faith"] },
  { key: "arc", short: "ARC", names: ["arc", "arcane"] },
];
const LOCK_KEYS: Record<CombatKey, keyof OptimizeRequestDto> = {
  strStat: "lockStr", dex: "lockDex", intStat: "lockInt", fai: "lockFai", arc: "lockArc",
};
const MIN_KEYS: Record<CombatKey, keyof OptimizeRequestDto> = {
  strStat: "minStr", dex: "minDex", intStat: "minInt", fai: "minFai", arc: "minArc",
};
const OBJECTIVE_WORDS: Record<ObjectiveId, string> = {
  max_ar: "max ar attack rating damage",
  max_physical_ar: "max physical ar attack rating",
  max_ar_plus_bleed: "bleed blood loss hemorrhage then ar",
  aow_first_hit: "aow ash of war skill first hit",
  aow_full_sequence: "aow ash of war skill full sequence combo",
};
const WORKSPACES: Array<{ id: WorkspaceTab; label: string }> = [
  { id: "rankings", label: "Rankings" },
  { id: "compare", label: "Compare" },
  { id: "paths", label: "Paths" },
  { id: "affinity_watch", label: "Affinity Watch" },
];

const STAT_PATTERN = /^(lock|min|minimum|set)?\s*([a-z]+)\s*(?:to|at|=)?\s*(\d{1,3})$/;
const UPGRADE_PATTERN = /^(upgrade|standard|somber|cap|weapon)?\s*\+\s*(\d{1,2})$|^(upgrade|standard|somber|cap)\s*(\d{1,2})$/;
const TOP_PATTERN = /^top\s*(\d{1,3})$/;
const BLESSING_PATTERN = /^(?:blessing|scadutree|sb)\s*(\d{1,2})$/;

export function normalizeQuery(query: string): string {
  return query.toLowerCase().replace(/\s+/g, " ").trim();
}

export function findCommands(rawQuery: string, context: CommandContext, limit = 8): Command[] {
  const query = normalizeQuery(rawQuery);
  const parsed = parsedCommands(query, context);
  if (!query) return [...parsed, ...suggestions(context)].slice(0, limit);
  const ranked = staticCommands(context)
    .map((command) => ({ command, score: matchScore(query, `${command.label} ${command.group}`) }))
    .filter((entry) => entry.score > 0)
    .sort((a, b) => b.score - a.score || a.command.label.length - b.command.label.length)
    .map((entry) => entry.command);
  const seen = new Set(parsed.map((command) => command.id));
  return [...parsed, ...ranked.filter((command) => !seen.has(command.id))].slice(0, limit);
}

// Higher is better; zero means no match. Word-prefix matches beat loose substring matches,
// so "uchi" finds Uchigatana before anything that merely contains those letters.
export function matchScore(query: string, text: string): number {
  const haystack = text.toLowerCase();
  if (!query) return 1;
  if (haystack.startsWith(query)) return 100;
  const words = haystack.split(/[^a-z0-9+']+/).filter(Boolean);
  if (words.some((word) => word.startsWith(query))) return 80;
  const tokens = query.split(" ");
  if (tokens.every((token) => words.some((word) => word.startsWith(token)))) return 70;
  if (haystack.includes(query)) return 50;
  return isSubsequence(query.replaceAll(" ", ""), haystack) ? 10 : 0;
}

function isSubsequence(needle: string, haystack: string): boolean {
  let index = 0;
  for (const char of haystack) {
    if (char === needle[index]) index += 1;
    if (index === needle.length) return true;
  }
  return needle.length === 0;
}

function parsedCommands(query: string, context: CommandContext): Command[] {
  const { request, catalog } = context;
  const statMatch = STAT_PATTERN.exec(query);
  if (statMatch) {
    const [, verb, name, digits] = statMatch;
    const stat = STATS.find((entry) => entry.names.includes(name));
    if (stat) return statCommands(stat, verb ?? null, Number(digits), context);
  }
  const rules = catalog.dataManifest.rules;
  const upgradeMatch = UPGRADE_PATTERN.exec(query);
  if (upgradeMatch) {
    const kind = upgradeMatch[1] ?? upgradeMatch[3] ?? null;
    const value = Number(upgradeMatch[2] ?? upgradeMatch[4]);
    const commands: Command[] = [];
    if (!rules.separateUpgradeCaps) {
      if (value <= rules.standardMaxUpgrade) {
        commands.push(command("upgrade-both", "Upgrades", `Weapon upgrade +${value}`, current(`+${request.standardMaxUpgrade}`),
          { kind: "patch", patch: { standardMaxUpgrade: value, somberMaxUpgrade: value } }));
      }
      return commands;
    }
    if (kind !== "somber" && value <= rules.standardMaxUpgrade) {
      commands.push(command("upgrade-standard", "Upgrades", `Standard upgrade cap +${value}`, current(`+${request.standardMaxUpgrade}`),
        { kind: "patch", patch: { standardMaxUpgrade: value } }));
    }
    if (kind !== "standard" && value <= rules.somberMaxUpgrade) {
      commands.push(command("upgrade-somber", "Upgrades", `Somber upgrade cap +${value}`, current(`+${request.somberMaxUpgrade}`),
        { kind: "patch", patch: { somberMaxUpgrade: value } }));
    }
    return commands;
  }
  const topMatch = TOP_PATTERN.exec(query);
  if (topMatch) {
    const value = Number(topMatch[1]);
    return value >= 1 && value <= 50
      ? [command("top", "Results", `Show the top ${value} results`, current(String(request.topK)), { kind: "patch", patch: { topK: value } })]
      : [];
  }
  const blessingMatch = BLESSING_PATTERN.exec(query);
  if (blessingMatch && rules.scadutreeScaling) {
    const value = Number(blessingMatch[1]);
    return value <= SCADUTREE_MAX_LEVEL
      ? [command("blessing", "Scaling", `Scadutree Blessing ${value}`, request.dlcScaling ? current(String(request.scadutreeLevel)) : "Turns DLC scaling on",
        { kind: "patch", patch: { dlcScaling: true, scadutreeLevel: value } })]
      : [];
  }
  return [];
}

function statCommands(
  stat: { key: StatKey; short: string },
  verb: string | null,
  value: number,
  { request, catalog, fixedStats }: CommandContext,
): Command[] {
  if (value < 1 || value > 99) return [];
  // Same floor as the ribbon inputs: the class base, and never below 1.
  const base = Math.max(1, classMeta(catalog, request.className).baseStats[stat.key]);
  const entered = Math.max(base, value);
  const set = command(`set-${stat.key}`, "Stats", `Set ${stat.short} to ${entered}`,
    entered !== value ? `${request.className} starts at ${base}` : current(String(request[stat.key])),
    { kind: "patch", patch: { [stat.key]: entered } });
  const combat = stat.key === "strStat" || stat.key === "dex" || stat.key === "intStat" || stat.key === "fai" || stat.key === "arc";
  if (!combat || fixedStats) return [set];
  const lockKey = LOCK_KEYS[stat.key as CombatKey];
  const minKey = MIN_KEYS[stat.key as CombatKey];
  const lock = command(`lock-${stat.key}`, "Stats", `Lock ${stat.short} at ${entered}`, "Exact result lock",
    { kind: "lock", patch: { [lockKey]: entered } });
  const min = command(`min-${stat.key}`, "Stats", `Require at least ${value} ${stat.short}`, current(String(request[minKey] || "none")),
    { kind: "patch", patch: { [minKey]: value } });
  if (verb === "lock") return [lock, set, min];
  if (verb === "min" || verb === "minimum") return [min, set, lock];
  return [set, lock, min];
}

// Entity and toggle commands depend only on the catalog and a few flags, so they are rebuilt
// cheaply per keystroke; the catalog lists themselves are cached per catalog object.
const entityCache = new WeakMap<CatalogDto, Command[]>();

function staticCommands(context: CommandContext): Command[] {
  return [...stateCommands(context), ...entityCommands(context)];
}

function entityCommands({ catalog, request }: CommandContext): Command[] {
  let named = entityCache.get(catalog);
  if (!named) {
    named = [
      ...catalog.classes.map((entry) => command(`class-${entry.name}`, "Class", `Class: ${entry.name}`, `Starts at level ${entry.baseLevel}`,
        { kind: "class", className: entry.name })),
      ...catalog.weaponNames.map((name) => command(`weapon-${name}`, "Weapon", `Weapon: ${name}`, undefined, { kind: "weapon", weaponName: name })),
      ...catalog.aowNames.map((name) => command(`aow-${name}`, "Skill", `Skill: ${name}`, undefined, { kind: "patch", patch: { aowName: name } })),
    ];
    entityCache.set(catalog, named);
  }
  // Filter commands carry the request's other filters, so they are never cached.
  const filterCommand = (dimensionId: "weapon_type" | "affinity") =>
    (catalog.filterDimensions.find((dimension) => dimension.id === dimensionId)?.options ?? []).map((option) => {
      const entries = replaceFilterEntries(request.filters.entries, dimensionId, [option.id], []);
      return dimensionId === "weapon_type"
        ? command(`type-${option.id}`, "Weapon type", `Only ${option.label} weapons`, `${option.count} weapons`,
          { kind: "patch", patch: { weaponTypeKey: null, weaponName: null, aowName: null, filters: { version: 1, entries } } })
        : command(`affinity-${option.id}`, "Affinity", `Affinity: ${option.label}`, undefined,
          { kind: "patch", patch: { affinity: null, aowName: null, filters: { version: 1, entries } } });
    });
  return [...named, ...filterCommand("weapon_type"), ...filterCommand("affinity")];
}

function stateCommands(context: CommandContext): Command[] {
  const { catalog, request, lockedStatMode, fixedStats, isSearching, resultsStale, analysesAvailable, profiles } = context;
  const rules = catalog.dataManifest.rules;
  const commands: Command[] = [];
  commands.push(isSearching
    ? command("cancel-search", "Actions", "Cancel search", undefined, { kind: "cancelSearch" })
    : command("search", "Actions", resultsStale ? "Update results" : "Search", "Rank every legal setup", { kind: "search" }));
  for (const objective of catalog.objectiveIds) {
    commands.push(command(`objective-${objective}`, "Objective", `Objective: ${objectiveLabel(objective)}`,
      request.objective === objective ? "Current" : OBJECTIVE_WORDS[objective], { kind: "patch", patch: { objective } }));
  }
  for (const workspace of WORKSPACES) {
    if (workspace.id !== "rankings" && !analysesAvailable) continue;
    commands.push(command(`workspace-${workspace.id}`, "Go to", `Go to ${workspace.label}`, undefined, { kind: "workspace", workspace: workspace.id }));
  }
  for (const profile of profiles) {
    if (profile.id === request.profileId) continue;
    commands.push(command(`profile-${profile.id}`, "Profile", `Switch to ${profile.label}`, undefined, { kind: "profile", profileId: profile.id }));
  }
  commands.push(command("two-handing", "Handling", request.twoHanding ? "One-hand weapons" : "Two-hand weapons",
    request.twoHanding ? "Two-handing is on" : "Applies the 1.5x Strength rule", { kind: "patch", patch: { twoHanding: !request.twoHanding } }));
  commands.push(request.exactUpgrade
    ? command("explore-upgrades", "Upgrades", "Explore up to upgrade caps", "Every level up to each cap", { kind: "patch", patch: { exactUpgrade: false } })
    : command("exact-upgrades", "Upgrades", "Use exact upgrade levels", "Only the entered levels", { kind: "patch", patch: { exactUpgrade: true } }));
  if (rules.scadutreeScaling) {
    commands.push(command("dlc-scaling", "Scaling", request.dlcScaling ? "Turn DLC scaling off" : "Turn DLC scaling on",
      `Scadutree Blessing ${request.scadutreeLevel}`, { kind: "patch", patch: { dlcScaling: !request.dlcScaling } }));
  }
  for (const grouping of ["automatic", "weapon", "loadout"] as const) {
    if (grouping === request.resultGrouping) continue;
    commands.push(command(`grouping-${grouping}`, "Results",
      grouping === "automatic" ? "Group results automatically" : grouping === "weapon" ? "Group results per weapon" : "Group results per loadout",
      undefined, { kind: "patch", patch: { resultGrouping: grouping } }));
  }
  commands.push(command("reset-stats", "Character", "Reset stats", fixedStats ? "All eight stats to 1" : `${request.className} base values`, { kind: "resetStats" }));
  if (!fixedStats) {
    commands.push(command("optimize-class", "Character", "Optimize class", "Lowest level for the entered stats", { kind: "optimizeClass" }));
    commands.push(command("locked-mode", "Limits", lockedStatMode ? "Stop using locked result stats" : "Use locked result stats", undefined,
      { kind: "lockedMode", value: !lockedStatMode }));
    if (lockedStatMode || [request.lockStr, request.lockDex, request.lockInt, request.lockFai, request.lockArc].some((value) => value !== null)) {
      commands.push(command("clear-locks", "Limits", "Clear locks", undefined, { kind: "clearLocks" }));
    }
  }
  commands.push(command("reset-filters", "Weapon", "Reset weapon filters", undefined, { kind: "resetFilters" }));
  return commands;
}

function suggestions(context: CommandContext): Command[] {
  const preferred = ["search", "cancel-search", "two-handing", "exact-upgrades", "explore-upgrades", "reset-filters", "workspace-compare", "workspace-paths"];
  const commands = stateCommands(context);
  return preferred
    .map((id) => commands.find((entry) => entry.id === id))
    .filter((entry): entry is Command => Boolean(entry));
}

function command(id: string, group: string, label: string, detail: string | undefined, action: CommandAction): Command {
  return { id, group, label, detail, action };
}

function current(value: string): string {
  return `Now ${value}`;
}
