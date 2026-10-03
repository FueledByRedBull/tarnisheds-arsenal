import { fixed1, metricForObjective, objectiveLabel } from "./format";
import type { OptimizeRequestDto, SolvedBuildDto } from "./types";

const statNames = ["STR", "DEX", "INT", "FAI", "ARC"];
const damageTypes = ["physical", "magic", "fire", "lightning", "holy"] as const;

/** How far `row` leads `runnerUp` on the ranked objective; null when either is unavailable. */
export function leadOver(row: SolvedBuildDto, runnerUp: SolvedBuildDto, objective: OptimizeRequestDto["objective"], aowModelSupported = true): number | null {
  const metric = metricForObjective(row, objective, aowModelSupported);
  const runnerMetric = metricForObjective(runnerUp, objective, aowModelSupported);
  return metric === null || runnerMetric === null ? null : metric - runnerMetric;
}

export function explainBuild(row: SolvedBuildDto, request: OptimizeRequestDto, aowModelSupported = true, runnerUp: SolvedBuildDto | null = null): string[] {
  const metric = metricForObjective(row, request.objective, aowModelSupported);
  const lines = [request.objective === "max_ar_plus_bleed"
    ? `This search ranks bleed buildup first (${fixed1(row.bleedBuildup)}), then AR (${fixed1(row.ar.total)}). It does not add them together or predict bleed-proc damage.`
    : metric === null ? "Skill damage is unavailable for this loadout; no modeled result is available."
      : `This build scores ${fixed1(metric)} for ${objectiveLabel(request.objective)} under the current search constraints.`];
  if (runnerUp) lines.push(runnerUpLine(row, runnerUp, request.objective, aowModelSupported));
  const damage = damageTypes.map(key => [key, row.ar[key]] as const).filter(([, value]) => value > 0);
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

// What separates this build from the next one: the lead on the ranked objective, then the
// largest AR component difference, which is where an affinity or scaling choice shows up.
function runnerUpLine(row: SolvedBuildDto, runnerUp: SolvedBuildDto, objective: OptimizeRequestDto["objective"], aowModelSupported: boolean): string {
  const rival = `${runnerUp.weaponName} (${runnerUp.affinity}, +${runnerUp.upgrade})`;
  const lead = leadOver(row, runnerUp, objective, aowModelSupported);
  if (lead === null) return `The next build, ${rival}, has no comparable ${objectiveLabel(objective)} value.`;
  if (Math.abs(lead) < 0.05) return `It ties ${rival} at the displayed precision; the exact ranking and tie order place it first.`;
  const unit = objective === "max_ar_plus_bleed" ? "bleed buildup" : objectiveLabel(objective);
  const [type, difference] = damageTypes
    .map(key => [key, row.ar[key] - runnerUp.ar[key]] as const)
    .reduce((largest, entry) => Math.abs(entry[1]) > Math.abs(largest[1]) ? entry : largest);
  const split = Math.abs(difference) < 0.05 ? "" : ` The largest AR difference is ${difference > 0 ? "+" : ""}${fixed1(difference)} ${type}.`;
  return `It leads the next build, ${rival}, by ${fixed1(lead)} ${unit}.${split}`;
}
