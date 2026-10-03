import { compactNumber } from "../../lib/format";
import { scalingLetter } from "../../lib/session";
import type { ScalingDto, SolvedBuildDto } from "../../lib/types";

const SCALING_STATS = [
  ["STR", "Strength", "str"],
  ["DEX", "Dexterity", "dex"],
  ["INT", "Intelligence", "int"],
  ["FAI", "Faith", "fai"],
  ["ARC", "Arcane", "arc"],
] as const;

const STATUS_STATS = [
  ["BLD", "Bleed", "bleedBuildup"],
  ["FRS", "Frost", "frostBuildup"],
  ["PSN", "Poison", "poisonBuildup"],
  ["ROT", "Scarlet Rot", "scarletRotBuildup"],
  ["SLP", "Sleep", "sleepBuildup"],
  ["MAD", "Madness", "madnessBuildup"],
  ["DTH", "Death Blight", "deathBuildup"],
] as const;

export function ScalingTokens({
  scaling,
  extended,
}: {
  scaling: ScalingDto | null | undefined;
  extended: boolean;
}) {
  return (
    <span className="metric-token-grid scaling-token-grid" role="list" aria-label="Attribute scaling">
      {SCALING_STATS.map(([short, full, key]) => {
        const grade = scaling ? scalingLetter(scaling[key], extended) : "-";
        return (
          <span
            className="metric-token"
            role="listitem"
            aria-label={`${full} scaling: ${grade}`}
            title={`${full} scaling: ${grade}`}
            key={key}
          >
            <small aria-hidden="true">{short}</small>
            <b aria-hidden="true">{grade}</b>
          </span>
        );
      })}
    </span>
  );
}

const COMBAT_STATS = [
  ["STR", "Strength", "strStat"],
  ["DEX", "Dexterity", "dex"],
  ["INT", "Intelligence", "intStat"],
  ["FAI", "Faith", "fai"],
  ["ARC", "Arcane", "arc"],
] as const;

export const STAT_KEYS = COMBAT_STATS.map(([short]) => short);

export function StatTokens({ row }: { row: SolvedBuildDto }) {
  return (
    <span className="metric-token-grid stat-token-grid row-combat-stats" role="list" aria-label="Combat stats">
      {COMBAT_STATS.map(([short, full, key]) => (
        <span
          className="metric-token"
          role="listitem"
          aria-label={`${full} ${row.stats[key]}`}
          title={`${full} ${row.stats[key]}`}
          key={key}
        >
          <small aria-hidden="true">{short}</small>
          <b aria-hidden="true">{row.stats[key]}</b>
        </span>
      ))}
    </span>
  );
}

// Only the statuses a build actually inflicts: seven tiles that are mostly zero bury the one
// that matters. Zero is a modeled value here, so "No status buildup" states it plainly.
export function StatusTokens({ row }: { row: SolvedBuildDto }) {
  const inflicted = STATUS_STATS.filter(([, , key]) => row[key] > 0);
  if (!inflicted.length) return <span className="status-none">No status buildup</span>;
  return (
    <span className="metric-token-grid status-token-grid" role="list" aria-label="Status buildup">
      {inflicted.map(([short, full, key]) => (
        <span
          className="metric-token active"
          role="listitem"
          aria-label={`${full} buildup: ${compactNumber(row[key])}`}
          title={`${full} buildup: ${compactNumber(row[key])}`}
          key={key}
        >
          <small aria-hidden="true">{short}</small>
          <b aria-hidden="true">{compactNumber(row[key])}</b>
        </span>
      ))}
    </span>
  );
}
