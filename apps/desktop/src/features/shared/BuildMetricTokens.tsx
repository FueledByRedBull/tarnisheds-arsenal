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

const COMBAT_STATS = [
  ["STR", "Strength", "strStat"],
  ["DEX", "Dexterity", "dex"],
  ["INT", "Intelligence", "intStat"],
  ["FAI", "Faith", "fai"],
  ["ARC", "Arcane", "arc"],
] as const;

/** Short stat names, shown once in a table header so rows can carry bare values. */
export const COMBAT_STAT_LABELS = COMBAT_STATS.map(([short]) => short);

type Token = { short: string; name: string; value: string };

// One renderer for every token row. Ranking rows drop the short labels: their header shows them.
function Tokens({ label, className, tokens, labels = true, active = false }: {
  label: string;
  className: string;
  tokens: Token[];
  labels?: boolean;
  active?: boolean;
}) {
  return (
    <span className={`metric-token-grid ${className}`} role="list" aria-label={label}>
      {tokens.map(({ short, name, value }) => (
        <span className={`metric-token${active ? " active" : ""}`} role="listitem" aria-label={name} title={name} key={short}>
          {labels ? <small aria-hidden="true">{short}</small> : null}
          <b aria-hidden="true">{value}</b>
        </span>
      ))}
    </span>
  );
}

export function ScalingTokens({ scaling, extended, labels }: {
  scaling: ScalingDto | null | undefined;
  extended: boolean;
  labels?: boolean;
}) {
  return (
    <Tokens label="Attribute scaling" className="scaling-token-grid" labels={labels} tokens={SCALING_STATS.map(([short, full, key]) => {
      const grade = scaling ? scalingLetter(scaling[key], extended) : "-";
      return { short, name: `${full} scaling: ${grade}`, value: grade };
    })} />
  );
}

export function StatTokens({ row }: { row: SolvedBuildDto }) {
  return (
    <Tokens label="Combat stats" className="stat-token-grid row-combat-stats" labels={false} tokens={COMBAT_STATS.map(([short, full, key]) => (
      { short, name: `${full} ${row.stats[key]}`, value: String(row.stats[key]) }
    ))} />
  );
}

// Only the statuses a build actually inflicts: seven tiles that are mostly zero bury the one
// that matters. Zero is a modeled value here, so "No status buildup" states it plainly.
export function StatusTokens({ row }: { row: SolvedBuildDto }) {
  const inflicted = STATUS_STATS.filter(([, , key]) => row[key] > 0);
  if (!inflicted.length) return <span className="status-none">No status buildup</span>;
  return (
    <Tokens label="Status buildup" className="status-token-grid" active tokens={inflicted.map(([short, full, key]) => (
      { short, name: `${full} buildup: ${compactNumber(row[key])}`, value: compactNumber(row[key]) }
    ))} />
  );
}
