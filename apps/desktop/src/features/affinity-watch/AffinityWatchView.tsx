import { Pause, Play } from "lucide-react";
import { memo } from "react";
import { api } from "../../lib/api";
import { analysisStatus } from "../../lib/analysis-status";
import { AnalysisProgress } from "../shared/AnalysisProgress";
import { chartX, chartY, contiguousMetricSegments, niceTicks, paddedMetricDomain } from "../../lib/chart";
import { AFFINITY_RUN, useAnalysisRun, useBuiltRequest } from "../../lib/hooks";
import { fixed1, objectiveLabel, objectiveUnit, statLine } from "../../lib/format";
import { clampHorizon, stableSignature } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { AffinityWatchPayloadDto } from "../../lib/types";
import { DraftNumberInput } from "../query-strip/DraftNumberInput";
import { WorkspaceHeading } from "../shared/WorkspaceHeading";
import { WorkspaceGate } from "../shared/WorkspaceGate";
import { ChartDataTable } from "../../lib/ChartDataTable";

export function AffinityWatchView() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const horizon = useDesktopStore((state) => state.affinityHorizon);
  const setHorizon = useDesktopStore((state) => state.setAffinityHorizon);
  const payload = useDesktopStore((state) => state.affinityPayload);
  const isAffinityBusy = useDesktopStore((state) => state.isAffinityBusy);
  const affinityProgress = useDesktopStore((state) => state.affinityProgress);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const base = useBuiltRequest(catalog, request, lockedStatMode);
  const effectiveHorizon = clampHorizon(request, horizon);
  const signature = stableSignature({ base, selected, horizon: effectiveHorizon });
  const affinitySignature = useDesktopStore((state) => state.affinitySignature);
  const analysis = useAnalysisRun(AFFINITY_RUN, signature);
  const status = analysisStatus({
    busy: isAffinityBusy,
    resultSignature: affinitySignature,
    requestSignature: signature,
    hasResult: payload !== null,
    outcome: analysis.outcome,
  });

  async function refresh() {
    if (useDesktopStore.getState().resultsStale) return;
    if (!selected) {
      pushNotice({ scope: "affinity_watch", tone: "warning", message: "Pick a selected result first." });
      return;
    }
    if (effectiveHorizon <= 0) {
      pushNotice({ scope: "affinity_watch", tone: "warning", message: "Combat stats are already capped. There is no forward horizon to inspect." });
      return;
    }
    await analysis.start(
      () => api.startAffinityWatch(base, selected, effectiveHorizon),
      (finished) => useDesktopStore.getState().setAffinityPayload(finished.payload, signature),
    );
  }

  if (resultsStale) {
    return <WorkspaceGate panelClass="affinity-panel" title="Affinity Watch" requirement="Requires current ranked results"
      heading="Update Rankings before continuing" message="The selected build belongs to the previous query." />;
  }

  return (
    <section className="workspace-panel affinity-panel">
      <div className="workspace-header analysis-workspace-header">
        <WorkspaceHeading title="Affinity Watch" aboutLabel="About Affinity Watch" about={<>
          Each affinity optimizes its own combat stats at the same level budget, ignoring exact stat locks but keeping
          minimums. The selected skill and upgrade stay fixed, and affinities the skill cannot use are left out. A crossover
          marks the level where another affinity becomes the best.
        </>}>
          {selected
            ? <small className="selected-summary">{selected.weaponName} / {selected.affinity} / {selected.aowName ?? "Unspecified skill"} / +{selected.upgrade} · {objectiveLabel(request.objective)} over the next {effectiveHorizon} levels</small>
            : <span>Requires selected result</span>}
        </WorkspaceHeading>
        <div className="header-controls">
          <label>
            Levels ahead
            <DraftNumberInput value={horizon} min={1} max={200} onCommit={setHorizon} />
          </label>
          <button type="button" className="analysis-action" onClick={() => void (isAffinityBusy ? analysis.stop() : refresh())} disabled={!selected && !isAffinityBusy}>
            {isAffinityBusy ? <Pause size={15} /> : <Play size={15} />}
            {isAffinityBusy ? "Stop" : "Watch affinities"}
          </button>
        </div>
      </div>
      <AnalysisProgress
        checked={affinityProgress?.checked ?? 0}
        total={affinityProgress?.total ?? (payload?.lines.length || 1)}
        status={status}
        running="Analyzing affinities"
        completed={`Completed · ${payload?.lines.length ?? 0} affinit${payload?.lines.length === 1 ? "y" : "ies"}`}
      />
      <AffinityChart payload={payload} objective={objectiveLabel(request.objective)} unit={objectiveUnit(request.objective)} />
      {/* Before a run the chart says what to do; a header over no rows would only add noise. */}
      {payload?.lines.length ? <div className="affinity-ranking" role="table" aria-label="Affinity watch rankings">
        <div className="affinity-row table-header" role="row">
          {["Rank at end", "Affinity", `Start (${objectiveUnit(request.objective)})`, `End (${objectiveUnit(request.objective)})`, "Final stats"].map((label) => <span role="columnheader" key={label}>{label}</span>)}
        </div>
        {payload.lines.map((line, index) => (
          <div className="affinity-row" key={line.affinity} role="row">
            <b role="cell">{index + 1}</b>
            <strong role="cell">{line.affinity}</strong>
            <span role="cell">{endpointMetric(line.startMetric)}</span>
            <span role="cell">{endpointMetric(line.endMetric)}</span>
            <small role="cell">{line.finalBuild ? statLine(line.finalBuild) : "Unavailable"}</small>
          </div>
        ))}
      </div> : null}
      <div className="crossover-table" role="table" aria-label="Affinity watch breakpoints">
        {payload?.breakpoints.length ? (
          <div role="row" className="table-header">
            {["Level", "Best affinity changes", "Old / new and lead"].map((label) => <span role="columnheader" key={label}>{label}</span>)}
          </div>
        ) : null}
        {payload?.breakpoints.map((point) => (
          <div key={`${point.level}-${point.incomingAffinity}`} role="row">
            <span role="cell">Level {point.level}</span>
            <strong role="cell">{point.outgoingAffinity} to {point.incomingAffinity}</strong>
            <span role="cell">{fixed1(point.outgoingMetric)} / {fixed1(point.incomingMetric)} {objectiveUnit(request.objective)} · +{fixed1(point.lead)} {objectiveUnit(request.objective)} ({fixed1(point.leadPercent)}%) · {leadQuality(point.quality)}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

// The native side grades a crossover's lead as tie, narrow (under 1%) or clear.
function leadQuality(quality: string): string {
  return quality === "tie" ? "a tie" : quality === "narrow" ? "narrow lead" : quality === "clear" ? "clear lead" : quality;
}

function endpointMetric(value: number | null): string {
  return value === null ? "Unavailable" : fixed1(value);
}

// Memoised: progress polls re-render the view, but the chart changes only with its payload.
const AffinityChart = memo(function AffinityChart({ payload, objective, unit }: { payload: AffinityWatchPayloadDto | null; objective: string; unit: string }) {
  const values = payload?.lines.flatMap((line) => line.points.map((point) => point.metric).filter((metric): metric is number => metric !== null)) ?? [];
  const domain = paddedMetricDomain(values);
  const axisMetrics = niceTicks(domain);
  const levels = payload?.lines.flatMap((line) => line.points.map((point) => point.level)) ?? [];
  const firstLevel = levels.length ? Math.min(...levels) : null;
  const lastLevel = levels.length ? Math.max(...levels) : null;
  const plotFirstLevel = firstLevel ?? 0;
  const plotLastLevel = lastLevel ?? plotFirstLevel;
  const hasLines = Boolean(payload?.lines.length && firstLevel !== null && lastLevel !== null);
  return (
    <figure className="affinity-chart" aria-label={`${objective} by character level for ${payload?.lines.length ?? 0} affinities`}>
      <figcaption>
        <span><strong>{objective.endsWith(unit) ? objective : `${objective} (${unit})`}</strong></span>
        <span>{hasLines ? `Level ${firstLevel} to ${lastLevel}` : "Awaiting analysis"}</span>
      </figcaption>
      {hasLines ? (
        <>
          <div className="affinity-legend chart-legend" role="group" aria-label="Affinity chart legend">
            {payload?.lines.map((line, index) => (
              <span key={line.affinity}>
                <i style={{ background: affinityColor(index) }} />
                <strong>{line.affinity}</strong>
                <small>{endpointMetric(line.startMetric)} → {endpointMetric(line.endMetric)} {unit}</small>
              </span>
            ))}
            <span className="affinity-crossover-key"><i /> Best-affinity crossover</span>
          </div>
          <div className="affinity-plot" aria-hidden="true">
            <div className="affinity-y-axis">
              {axisMetrics.map((metric) => <span key={metric} style={{ top: `${chartY(metric, domain) / 2.2}%` }}>{metric}</span>)}
            </div>
            <svg viewBox="0 0 1000 220" preserveAspectRatio="none">
              {axisMetrics.map((metric) => {
                const y = chartY(metric, domain);
                return <line className="affinity-grid-line" x1="0" x2="1000" y1={y} y2={y} key={metric} />;
              })}
              {payload?.breakpoints.map((point) => {
                const x = chartX(point.level, plotFirstLevel, plotLastLevel);
                return (
                  <g className="affinity-crossover-marker" key={`${point.level}-${point.incomingAffinity}`}>
                    <line x1={x} x2={x} y1="14" y2="206" />
                    <rect x={x - 4} y="16" width="8" height="8" transform={`rotate(45 ${x} 20)`} />
                  </g>
                );
              })}
              {payload?.lines.map((line, index) => (
                <g className={`affinity-series-group series-${index}`} key={line.affinity}>
                  {contiguousMetricSegments(line.points).map((segment, segmentIndex) => (
                    <polyline
                      className="affinity-series"
                      points={linePoints(segment, plotFirstLevel, plotLastLevel, domain)}
                      stroke={affinityColor(index)}
                      key={`${line.affinity}-segment-${segmentIndex}`}
                    />
                  ))}
                  {/* Hover markers are sampled to about 50 per line; the data table keeps every value. */}
                  {line.points.filter((point, pointIndex) => point.metric !== null
                    && (pointIndex % Math.ceil(line.points.length / 50) === 0 || pointIndex === line.points.length - 1)).map((point) => (
                    <circle
                      className="affinity-series-point"
                      cx={chartX(point.level, plotFirstLevel, plotLastLevel)}
                      cy={chartY(point.metric, domain)}
                      fill={affinityColor(index)}
                      key={`${line.affinity}-${point.level}`}
                      r="3.5"
                    >
                      <title>{line.affinity} level {point.level}: {fixed1(point.metric)}</title>
                    </circle>
                  ))}
                </g>
              ))}
            </svg>
            <div className="affinity-x-axis">
              <span>Lv {firstLevel}</span>
              <span>Character level</span>
              <span>Lv {lastLevel}</span>
            </div>
          </div>
        </>
      ) : (
        <div className="affinity-chart-empty">
          <strong>Compare every legal affinity over future levels</strong>
          <span>Watch affinities to reveal scaling curves and exact crossover points.</span>
        </div>
      )}
      <ChartDataTable caption={`${objective} (${unit}) affinity values by character level`} headers={["Affinity", "Level", `${objective} (${unit})`]}
        rows={payload?.lines.flatMap((line) => line.points.map((point) => [line.affinity, String(point.level), `${fixed1(point.metric)} ${unit}`])) ?? []} />
    </figure>
  );
});

// Series hues stay clear of the status colours (danger red, warning ember, success green).
const AFFINITY_COLORS = [
  "#e0b967",
  "#79bfd0",
  "#b59ad8",
  "#d7d2b6",
  "#729fe0",
  "#d889b3",
  "#b7cb72",
  "#8f86d8",
  "#a7c7e7",
  "#c9a0dc",
];

function affinityColor(index: number): string {
  return AFFINITY_COLORS[index % AFFINITY_COLORS.length];
}

function linePoints(
  points: AffinityWatchPayloadDto["lines"][number]["points"],
  firstLevel: number,
  lastLevel: number,
  domain: ReturnType<typeof paddedMetricDomain>,
): string {
  return points
    .filter((point) => point.metric !== null)
    .map((point) => `${chartX(point.level, firstLevel, lastLevel)},${chartY(point.metric, domain)}`)
    .join(" ");
}
