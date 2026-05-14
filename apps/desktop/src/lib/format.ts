import { ObjectiveId, SolvedBuildDto } from "./types";

export function compactNumber(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "-";
  }
  return Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(value);
}

export function fixed1(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "-";
  }
  return value.toFixed(1);
}

export function objectiveLabel(objective: ObjectiveId): string {
  switch (objective) {
    case "max_physical_ar":
      return "Max Phys. AR";
    case "max_ar_plus_bleed":
      return "AR + Bleed";
    case "aow_first_hit":
      return "AoW First Hit";
    case "aow_full_sequence":
      return "AoW Sequence";
    default:
      return "Max AR";
  }
}

export function metricForObjective(row: SolvedBuildDto, objective: ObjectiveId): number {
  switch (objective) {
    case "max_physical_ar":
      return row.ar.physical;
    case "aow_first_hit":
      return row.aowFirstHitDamage;
    case "aow_full_sequence":
      return row.aowFullSequenceDamage;
    case "max_ar_plus_bleed":
      return row.score;
    default:
      return row.ar.total;
  }
}

export function statLine(row: SolvedBuildDto): string {
  return `STR ${row.stats.strStat} / DEX ${row.stats.dex} / INT ${row.stats.intStat} / FAI ${row.stats.fai} / ARC ${row.stats.arc}`;
}
