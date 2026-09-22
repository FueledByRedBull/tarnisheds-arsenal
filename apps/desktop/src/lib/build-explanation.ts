import { fixed1, metricForObjective, objectiveLabel } from "./format";
import { STAT_KEYS } from "./session";
import type { OptimizeRequestDto, SolvedBuildDto } from "./types";

const statNames = ["STR", "DEX", "INT", "FAI", "ARC"];

export function explainBuild(row: SolvedBuildDto, request: OptimizeRequestDto): string[] {
  const lines = [request.objective === "max_ar_plus_bleed"
    ? `This search ranks bleed buildup first (${fixed1(row.bleedBuildup)}), then AR (${fixed1(row.ar.total)}). It does not add them together or predict bleed-proc damage.`
    : `This build scores ${fixed1(metricForObjective(row, request.objective))} for ${objectiveLabel(request.objective)} under the current search constraints.`];
  const damage = (["physical", "magic", "fire", "lightning", "holy"] as const)
    .map(key => [key, row.ar[key]] as const).filter(([, value]) => value > 0);
  if (damage.length) lines.push(`Weapon AR comes from ${damage.map(([key, value]) => `${fixed1(value)} ${key}`).join(" + ")}. Enemy defenses and negation are not applied.`);
  const locks = [request.lockStr, request.lockDex, request.lockInt, request.lockFai, request.lockArc];
  const minimums = [request.minStr, request.minDex, request.minInt, request.minFai, request.minArc];
  const constrained = statNames.flatMap((name, index) => locks[index] !== null
    ? [`${name} locked at ${locks[index]}`]
    : minimums[index] > 0 ? [`${name} minimum ${minimums[index]}`] : []);
  if (constrained.length) lines.push(`Allocation constraints: ${constrained.join(", ")}.`);
  if (request.weaponName || request.affinity || request.aowName || request.filters.entries.length) {
    lines.push(`Equipment constraints: ${[request.weaponName, request.affinity, request.aowName,
      request.filters.entries.length ? `${request.filters.entries.length} include/exclude filters` : null].filter(Boolean).join(", ")}. Other equipment may win if you change those constraints.`);
  }
  lines.push("Displayed differences are rounded. The optimizer's exact ranking and tie order determine placement; equal displayed scores need not be exact ties.");
  return lines;
}

export function explainBuildComparison(baseline: SolvedBuildDto, candidate: SolvedBuildDto, request: Pick<OptimizeRequestDto, "objective">): string {
  const delta = metricForObjective(candidate, request.objective) - metricForObjective(baseline, request.objective);
  const arDelta = candidate.ar.total - baseline.ar.total;
  const stats = STAT_KEYS.flatMap((key, index) => {
    const change = candidate.stats[key] - baseline.stats[key];
    return change ? [`${statNames[index]} ${change > 0 ? "+" : ""}${change}`] : [];
  });
  return `${candidate.weaponName}: ${delta >= 0 ? "+" : ""}${fixed1(delta)} ${request.objective === "max_ar_plus_bleed" ? "bleed buildup" : objectiveLabel(request.objective)}, ${arDelta >= 0 ? "+" : ""}${fixed1(arDelta)} AR versus ${baseline.weaponName}. ${stats.length ? `Stat changes: ${stats.join(", ")}.` : "Combat stats are unchanged."} ${request.objective === "max_ar_plus_bleed" ? "Bleed ranks before AR; this does not estimate proc damage." : "These are raw modeled values before enemy defenses."}`;
}
