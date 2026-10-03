import { hasAowDamage, metricForObjective } from "./format";
import { ObjectiveId, SolvedBuildDto } from "./types";

// Display-only views of a ranked result list. The optimizer's order stays the rank; sorting
// another column reorders rows without renumbering them, and ties keep rank order.

export type RankSortKey = "rank" | "ar" | "skill" | "score";

export interface RankedEntry {
  row: SolvedBuildDto;
  rank: number;
}

export function sortValue(row: SolvedBuildDto, key: Exclude<RankSortKey, "rank">, objective: ObjectiveId, aowSupported: boolean): number | null {
  if (key === "ar") return row.ar.total;
  if (key === "skill") return hasAowDamage(row, aowSupported) ? row.aowFullSequenceDamage : null;
  return metricForObjective(row, objective, aowSupported);
}

// Rank ascends and values descend by default; `reverse` flips either. Unavailable values
// always sort last so a reversed column never leads with them.
export function sortRanked(
  rows: SolvedBuildDto[],
  key: RankSortKey,
  reverse: boolean,
  objective: ObjectiveId,
  aowSupported: boolean,
): RankedEntry[] {
  const entries = rows.map((row, rank) => ({ row, rank }));
  if (key === "rank") return reverse ? entries.reverse() : entries;
  return entries
    .map((entry) => ({ entry, value: sortValue(entry.row, key, objective, aowSupported) }))
    .sort((a, b) => {
      if (a.value === null || b.value === null) {
        return a.value === b.value ? a.entry.rank - b.entry.rank : a.value === null ? 1 : -1;
      }
      return (reverse ? a.value - b.value : b.value - a.value) || a.entry.rank - b.entry.rank;
    })
    .map(({ entry }) => entry);
}

export interface RankMovement {
  /** Places gained since the previous search; null when the loadout was not ranked before. */
  places: number | null;
  /** Change in the objective's value, when both searches ranked by the same objective. */
  metricDelta: number | null;
}

export const loadoutKey = (row: SolvedBuildDto) => `${row.weaponName}\u0000${row.affinity}\u0000${row.aowName ?? ""}`;

export function rankMovements(
  rows: SolvedBuildDto[],
  baseline: { objective: ObjectiveId; rows: SolvedBuildDto[] } | null,
  objective: ObjectiveId,
  aowSupported: boolean,
): RankMovement[] | null {
  if (!baseline) return null;
  // The same loadout can be ranked at several upgrades. Match the upgrade first, and follow a
  // loadout across an upgrade change only when it is ranked once in both searches.
  const exactKey = (row: SolvedBuildDto) => `${loadoutKey(row)}\u0000${row.upgrade}`;
  const count = (list: SolvedBuildDto[]) => list.reduce(
    (counts, row) => counts.set(loadoutKey(row), (counts.get(loadoutKey(row)) ?? 0) + 1), new Map<string, number>());
  const [countBefore, countNow] = [count(baseline.rows), count(rows)];
  const previous = new Map<string, { rank: number; row: SolvedBuildDto }>();
  baseline.rows.forEach((row, rank) => {
    for (const key of [exactKey(row), loadoutKey(row)]) if (!previous.has(key)) previous.set(key, { rank, row });
  });
  const sameObjective = baseline.objective === objective;
  return rows.map((row, rank) => {
    const key = loadoutKey(row);
    const before = previous.get(exactKey(row))
      ?? (countBefore.get(key) === 1 && countNow.get(key) === 1 ? previous.get(key) : undefined);
    if (!before) return { places: null, metricDelta: null };
    const now = metricForObjective(row, objective, aowSupported);
    const then = sameObjective ? metricForObjective(before.row, objective, aowSupported) : null;
    return { places: before.rank - rank, metricDelta: now !== null && then !== null ? now - then : null };
  });
}
