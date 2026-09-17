import { useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { fixed1, statLine } from "../../lib/format";
import { stableSignature } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { ArBleedFrontierPointDto, OptimizeRequestDto, SolvedBuildDto } from "../../lib/types";
import { runSearchFromStore } from "../../lib/workflows";

type LoadoutTradeoffsProps = {
  base: OptimizeRequestDto;
  current: OptimizeRequestDto;
  selected: SolvedBuildDto;
};

type TradeoffChoice = {
  index: number;
  point: ArBleedFrontierPointDto;
  labels: string[];
};

const CRITERIA: ReadonlyArray<readonly [string, number | null]> = [
  ["Max AR", null],
  ["Within 1%", 100],
  ["Within 3%", 300],
  ["Within 5%", 500],
  ["Max bleed", null],
];

const STAT_FIELDS = [
  ["STR", "strStat"],
  ["DEX", "dex"],
  ["INT", "intStat"],
  ["FAI", "fai"],
  ["ARC", "arc"],
] as const;

export function LoadoutTradeoffs({ base, current, selected }: LoadoutTradeoffsProps) {
  const allCombatLocksActive = [base.lockStr, base.lockDex, base.lockInt, base.lockFai, base.lockArc]
    .every((value) => typeof value === "number");
  const contextSignature = stableSignature({ base, selected });
  const contextRef = useRef(contextSignature);
  contextRef.current = contextSignature;
  const runGeneration = useRef(0);
  const controllerRef = useRef<AbortController | null>(null);
  const [points, setPoints] = useState<ArBleedFrontierPointDto[] | null>(null);
  const [status, setStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const [maxArSacrifice, setMaxArSacrifice] = useState("0");
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);

  useEffect(() => {
    controllerRef.current?.abort();
    controllerRef.current = null;
    runGeneration.current += 1;
    setPoints(null);
    setStatus("idle");
    setError(null);
    setSelectedIndex(null);
    return () => {
      controllerRef.current?.abort();
      controllerRef.current = null;
      runGeneration.current += 1;
    };
  }, [contextSignature]);

  const choices = points ? buildChoices(points) : [];
  const labelsByIndex = new Map(choices.map((choice) => [choice.index, choice.labels]));
  const allChoices = points?.map((point, index) => ({
    index,
    point,
    labels: labelsByIndex.get(index) ?? [],
  })) ?? [];
  const selectedPoint = points && selectedIndex !== null ? points[selectedIndex] ?? null : null;
  const selectedChoice = selectedIndex === null ? undefined : allChoices[selectedIndex];
  const sacrifice = parseSacrifice(maxArSacrifice);
  const sacrificeBps = sacrifice === null ? null : Math.round(sacrifice * 100);
  const thresholdIndex = points && sacrificeBps !== null
    ? largestBleedIndex(points, sacrificeBps)
    : null;

  async function compute() {
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    const generation = ++runGeneration.current;
    setStatus("loading");
    setPoints(null);
    setError(null);
    setSelectedIndex(null);
    try {
      const nextPoints = await api.arBleedFrontier(base, selected, controller.signal);
      if (
        controller.signal.aborted
        || generation !== runGeneration.current
        || contextRef.current !== contextSignature
      ) return;
      setPoints(nextPoints);
      setStatus("ready");
    } catch (nextError) {
      if (
        controller.signal.aborted
        || generation !== runGeneration.current
        || contextRef.current !== contextSignature
      ) return;
      setError(nextError instanceof Error ? nextError.message : String(nextError));
      setStatus("error");
    } finally {
      if (controllerRef.current === controller) controllerRef.current = null;
    }
  }

  function stop() {
    controllerRef.current?.abort();
    controllerRef.current = null;
    runGeneration.current += 1;
    setStatus("idle");
    setError(null);
  }

  useEffect(() => {
    setSelectedIndex(thresholdIndex);
  }, [points, sacrificeBps, thresholdIndex]);

  return (
    <section className="loadout-tradeoffs" aria-labelledby="loadout-tradeoffs-heading">
      <div className="tradeoff-header">
        <div>
          <h2 id="loadout-tradeoffs-heading">AR / Bleed tradeoffs</h2>
          <small>
            Fixed loadout: {selected.weaponName} / {selected.affinity} / {selected.aowName ?? "Unspecified skill"} / +{selected.upgrade}.
            The frontier keeps this query&apos;s class budget, floors, locks, handling, and world settings.{" "}
          </small>
          {allCombatLocksActive || (status === "ready" && points?.length === 1) ? (
            <small>
              {allCombatLocksActive
                ? "All five combat locks are active, so there is no stat allocation left to vary."
                : "Only one non-dominated AR / bleed outcome exists under these constraints; other allocations may tie."}
            </small>
          ) : null}
        </div>
        <button type="button" onClick={status === "loading" ? stop : compute} disabled={status === "ready"}>
          {status === "loading" ? "Stop" : points ? "Computed for this context" : "Compute trade-offs"}
        </button>
      </div>

      <div className="tradeoff-controls">
        <label>
          Max AR sacrifice (%)
          <input
            type="number"
            min={0}
            max={100}
            step={0.01}
            value={maxArSacrifice}
            onChange={(event) => setMaxArSacrifice(event.target.value)}
            aria-describedby="tradeoff-sacrifice-help"
          />
        </label>
        <small id="tradeoff-sacrifice-help">
          Selects the point with the largest bleed buildup within the entered AR sacrifice.
        </small>
        {sacrifice === null ? <span role="alert">Enter 0 to 100 in steps of 0.01%.</span> : null}
      </div>

      <div className="analysis-state" role="status" aria-live="polite">
        {status === "idle" ? "Ready to calculate this loadout once." : null}
        {status === "loading" ? "Calculating the exact frontier…" : null}
        {status === "error" ? `Trade-off calculation failed: ${error}` : null}
        {status === "ready" && !points?.length ? "No AR / bleed trade-off points were returned." : null}
        {status === "ready" && points?.length ? `${points.length} exact trade-off point${points.length === 1 ? "" : "s"} ready.` : null}
      </div>

      {points?.length ? (
        <>
          <TradeoffTable caption="Trade-off options" choices={choices} selectedIndex={selectedIndex} onSelect={setSelectedIndex} shortlist />

          <details className="tradeoff-all">
            <summary>Explore all tradeoffs ({points.length})</summary>
            <TradeoffPlot points={points} selectedIndex={selectedIndex} onSelect={setSelectedIndex}
              current={STAT_FIELDS.every(([, field]) => current[field] === selected.stats[field]) ? selected : null} />
            <TradeoffTable caption="All exact AR / bleed trade-off points" choices={allChoices} selectedIndex={selectedIndex} onSelect={setSelectedIndex} />
          </details>

          <TradeoffInspection point={selectedPoint} labels={selectedChoice?.labels ?? []} current={current} />
        </>
      ) : null}
    </section>
  );
}

function buildChoices(points: ArBleedFrontierPointDto[]): TradeoffChoice[] {
  const byIndex = new Map<number, TradeoffChoice>();
  for (const [criterionIndex, [label, maxLossBps]] of CRITERIA.entries()) {
    const index = maxLossBps === null
      ? criterionIndex === 0 ? (points.length ? 0 : null) : (points.length ? points.length - 1 : null)
      : largestBleedIndex(points, maxLossBps);
    if (index === null) continue;
    const existing = byIndex.get(index);
    if (existing) {
      existing.labels.push(label);
    } else {
      byIndex.set(index, { index, point: points[index], labels: [label] });
    }
  }
  return Array.from(byIndex.values());
}

function largestBleedIndex(points: ArBleedFrontierPointDto[], maxLossBps: number): number | null {
  let index: number | null = null;
  for (let pointIndex = 0; pointIndex < points.length; pointIndex += 1) {
    const point = points[pointIndex];
    if (point.minimumArLossBps <= maxLossBps) index = pointIndex;
  }
  return index;
}

function parseSacrifice(value: string): number | null {
  if (!/^\d+(?:\.\d{1,2})?$/.test(value.trim())) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 && parsed <= 100 ? parsed : null;
}

function TradeoffTable({
  caption,
  choices,
  selectedIndex,
  onSelect,
  shortlist = false,
}: {
  caption: string;
  choices: TradeoffChoice[];
  selectedIndex: number | null;
  onSelect: (index: number) => void;
  shortlist?: boolean;
}) {
  return (
    <table className={shortlist ? "tradeoff-table tradeoff-shortlist" : "tradeoff-table"}>
      <caption>{caption}</caption>
      <thead><tr><th scope="col">Select an option</th><th scope="col">AR</th><th scope="col">Bleed buildup</th><th scope="col">AR sacrificed</th></tr></thead>
      <tbody>{choices.map((choice) => <TradeoffRow key={choice.index} choice={choice} selected={choice.index === selectedIndex} onSelect={onSelect} />)}</tbody>
    </table>
  );
}

function TradeoffRow({
  choice,
  selected,
  onSelect,
}: {
  choice: TradeoffChoice;
  selected: boolean;
  onSelect: (index: number) => void;
}) {
  const { point } = choice;
  const label = choice.labels.join(" · ") || `Point ${choice.index + 1}`;
  return (
    <tr className={selected ? "selected" : undefined} aria-selected={selected}>
      <th scope="row">
        <button
          type="button"
          className="tradeoff-select"
          aria-pressed={selected}
          aria-label={`${label}: ${fixed1(point.bleedGain)} bleed gain, ${fixed1(point.arLossPercent)}% AR loss`}
          onClick={() => onSelect(choice.index)}
        >
          {label}
        </button>
      </th>
      <td>{fixed1(point.result.ar.total)}</td>
      <td>{fixed1(point.result.bleedBuildup)}</td>
      <td>{formatSacrifice(point)}</td>
    </tr>
  );
}

function TradeoffPlot({
  current,
  points,
  selectedIndex,
  onSelect,
}: {
  current: SolvedBuildDto | null;
  points: ArBleedFrontierPointDto[];
  selectedIndex: number | null;
  onSelect: (index: number) => void;
}) {
  const arValues = points.map((point) => point.result.ar.total);
  const bleedValues = points.map((point) => point.result.bleedBuildup);
  if (current) { arValues.push(current.ar.total); bleedValues.push(current.bleedBuildup); }
  const minAr = Math.min(...arValues);
  const maxAr = Math.max(...arValues);
  const minBleed = Math.min(...bleedValues);
  const maxBleed = Math.max(...bleedValues);
  return (
    <figure className="tradeoff-plot">
      <figcaption>
        <strong>Exact points</strong>
        <small>AR horizontal / bleed vertical. {current ? "Outlined square: current allocation." : "Current allocation is not shown without a matching evaluated result."}</small>
      </figcaption>
      <svg viewBox="0 0 1000 230" role="group" aria-label="Bleed buildup against AR; selectable tradeoff points">
        <line x1="42" x2="42" y1="16" y2="196" className="tradeoff-axis" />
        <line x1="42" x2="980" y1="196" y2="196" className="tradeoff-axis" />
        <text x="42" y="218" className="tradeoff-axis-label">{fixed1(minAr)} AR</text>
        <text x="980" y="218" textAnchor="end" className="tradeoff-axis-label">{fixed1(maxAr)} AR</text>
        <text x="50" y="18" className="tradeoff-axis-label">{fixed1(maxBleed)} bleed</text>
        <text x="50" y="188" className="tradeoff-axis-label">{fixed1(minBleed)} bleed</text>
        {current ? <g aria-label="Current allocation">
          <rect x={42 + ratio(current.ar.total, minAr, maxAr) * 938 - 6} y={196 - ratio(current.bleedBuildup, minBleed, maxBleed) * 180 - 6}
            width="12" height="12" fill="none" stroke="var(--parchment)" strokeWidth="2" />
          <title>Current allocation: {fixed1(current.ar.total)} AR / {fixed1(current.bleedBuildup)} bleed</title>
        </g> : null}
        {points.map((point, index) => {
          const x = 42 + ratio(point.result.ar.total, minAr, maxAr) * 938;
          const y = 196 - ratio(point.result.bleedBuildup, minBleed, maxBleed) * 180;
          const isSelected = index === selectedIndex;
          return (
            <circle
              key={index}
              cx={x}
              cy={y}
              r={isSelected ? 9 : 7}
              className={isSelected ? "tradeoff-point selected" : "tradeoff-point"}
              role="button"
              tabIndex={0}
              aria-label={`Point ${index + 1}: ${fixed1(point.result.ar.total)} AR, ${fixed1(point.result.bleedBuildup)} bleed`}
              aria-pressed={isSelected}
              onClick={() => onSelect(index)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onSelect(index);
                }
              }}
            >
              <title>Point {index + 1}: {fixed1(point.result.ar.total)} AR / {fixed1(point.result.bleedBuildup)} bleed</title>
            </circle>
          );
        })}
      </svg>
    </figure>
  );
}

function ratio(value: number, min: number, max: number): number {
  return max === min ? 0.5 : (value - min) / (max - min);
}

function TradeoffInspection({
  point,
  labels,
  current,
}: {
  point: ArBleedFrontierPointDto | null;
  labels: string[];
  current: OptimizeRequestDto;
}) {
  if (!point) return <div className="empty-state compact">Select an exact trade-off point to inspect it.</div>;
  return (
    <div className="tradeoff-inspection" aria-live="polite">
      <h3>{labels.length ? labels.join(" · ") : "Selected exact point"}</h3>
      <p>Gain <strong>{fixed1(point.bleedGain)} bleed buildup</strong> for <strong>{fixed1(point.arLoss)} AR ({point.arLossPercent.toFixed(2)}%)</strong> compared with the maximum-AR allocation.</p>
      <p><strong>Loadout</strong> {point.result.weaponName} / {point.result.affinity} / {point.result.aowName ?? "Unspecified skill"} / +{point.result.upgrade}</p>
      <p><strong>Full stat spread</strong> {fullStatLine(current, point.result)}</p>
      <p><strong>Changes from your current stats</strong> {formatStatDelta(current, point.result)}</p>
      <button type="button" onClick={() => {
        const state = useDesktopStore.getState();
        state.patchRequest({ objective: "max_ar" });
        state.useRowAsLocks(point.result);
        state.setWorkspace("rankings");
        void runSearchFromStore();
      }}>Use exact allocation</button>
      <small>Modeled AR and buildup; this does not predict bleed procs or damage after enemy defenses.</small>
    </div>
  );
}

function fullStatLine(current: OptimizeRequestDto, result: SolvedBuildDto): string {
  return `VIG ${current.vig} / MND ${current.mnd} / END ${current.end} / ${statLine(result)}`;
}

function formatStatDelta(current: OptimizeRequestDto, result: SolvedBuildDto): string {
  const fixed = [["VIG", current.vig, current.vig], ["MND", current.mnd, current.mnd], ["END", current.end, current.end]] as const;
  return [...fixed, ...STAT_FIELDS.map(([label, field]) => [label, current[field], result.stats[field]] as const)]
    .map(([label, from, to]) => {
      const delta = to - from;
      return `${label} ${delta >= 0 ? "+" : ""}${delta}`;
    })
    .join(" / ");
}

function formatSacrifice(point: ArBleedFrontierPointDto): string {
  return point.arLoss === 0 ? "None" : `${fixed1(point.arLoss)} (${point.arLossPercent.toFixed(2)}%)`;
}
