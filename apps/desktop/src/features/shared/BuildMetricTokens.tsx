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

export function StatusTokens({ row }: { row: SolvedBuildDto }) {
  return (
    <span className="metric-token-grid status-token-grid" role="list" aria-label="Status buildup">
      {STATUS_STATS.map(([short, full, key]) => (
        <span
          className={`metric-token ${row[key] > 0 ? "active" : "zero"}`}
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
