import { Pause, Play } from "lucide-react";
import { useMemo } from "react";
import { api } from "../../lib/api";
import { contiguousMetricSegments, metricRatio, paddedMetricDomain } from "../../lib/chart";
import { useAffinityJob, useRequestBudget } from "../../lib/hooks";
import { fixed1, objectiveLabel, statLine } from "../../lib/format";
import { clampHorizon, stableSignature } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { AffinityWatchFinishedDto, AffinityWatchPayloadDto } from "../../lib/types";

export function AffinityWatchView() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const horizon = useDesktopStore((state) => state.affinityHorizon);
  const setHorizon = useDesktopStore((state) => state.setAffinityHorizon);
  const payload = useDesktopStore((state) => state.affinityPayload);
  const isAffinityBusy = useDesktopStore((state) => state.isAffinityBusy);
  const setAffinityBusy = useDesktopStore((state) => state.setAffinityBusy);
  const beginAffinity = useDesktopStore((state) => state.beginAffinity);
  const affinityGeneration = useDesktopStore((state) => state.affinityGeneration);
  const activeAffinityJobId = useDesktopStore((state) => state.activeAffinityJobId);
  const setActiveAffinityJobId = useDesktopStore((state) => state.setActiveAffinityJobId);
  const affinityProgress = useDesktopStore((state) => state.affinityProgress);
  const setAffinityProgress = useDesktopStore((state) => state.setAffinityProgress);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const { base } = useRequestBudget(catalog, request, lockedStatMode);
  const effectiveHorizon = clampHorizon(request, horizon);
  const signature = stableSignature({ selected, objective: request.objective, level: base.characterLevel, horizon: effectiveHorizon });

  useAffinityJob({
    activeAffinityJobId,
    isAffinityBusy,
    generation: affinityGeneration,
    setAffinityProgress,
    finish: finishAffinityWatch,
  });

  async function refresh() {
    if (!selected) {
      pushNotice({ scope: "affinity_watch", tone: "warning", message: "Pick a selected result first." });
      return;
    }
    if (effectiveHorizon <= 0) {
      pushNotice({ scope: "affinity_watch", tone: "warning", message: "Combat stats are already capped. There is no forward horizon to inspect." });
      return;
    }
    const generation = beginAffinity(signature);
    try {
      const legal = await api.affinitiesForWeapon(request.profileId, selected.weaponName);
      let current = useDesktopStore.getState();
      if (
        current.affinityGeneration !== generation ||
        current.activeAffinitySignature !== signature
      ) return;
      if (legal.length === 0) {
        pushNotice({ scope: "affinity_watch", tone: "warning", message: "No legal affinities are available for the selected weapon." });
        setAffinityBusy(false);
        return;
      }
      const { jobId } = await api.startAffinityWatch(base, selected, effectiveHorizon);
      current = useDesktopStore.getState();
      if (
        current.affinityGeneration !== generation ||
        current.activeAffinitySignature !== signature
      ) {
        await api.cancelAffinityWatch(jobId);
        return;
      }
      setActiveAffinityJobId(jobId);
    } catch (error) {
      const current = useDesktopStore.getState();
      if (
        current.affinityGeneration === generation &&
        current.activeAffinitySignature === signature
      ) {
        setError(error instanceof Error ? error.message : String(error));
        setAffinityBusy(false);
      }
    }
  }

  async function stop() {
    if (!activeAffinityJobId) setAffinityBusy(false);
    if (activeAffinityJobId) await api.cancelAffinityWatch(activeAffinityJobId);
  }

  function finishAffinityWatch(event: AffinityWatchFinishedDto, generation: number) {
    const current = useDesktopStore.getState();
    if (
      generation !== current.affinityGeneration ||
      current.activeAffinitySignature !== signature ||
      event.jobId !== current.activeAffinityJobId
    ) return;
    if (event.error) current.setError(event.error);
    if (event.cancelled) current.pushNotice({ scope: "affinity_watch", tone: "warning", message: "Affinity watch stopped." });
    else current.setAffinityPayload(event.payload, signature);
    current.setAffinityBusy(false);
    current.setActiveAffinityJobId(null);
    current.setAffinityProgress(null);
  }

  return (
    <section className="workspace-panel affinity-panel">
      <div className="workspace-header analysis-workspace-header">
        <div className="workspace-heading-copy">
          <h1>Affinity Watch</h1>
          <span>{selected ? `${selected.weaponName} across Current +${effectiveHorizon}` : "Requires selected result"}</span>
          {selected ? <small className="selected-summary">{selected.affinity} / {selected.aowName ?? "Native"} / +{selected.upgrade} · {objectiveLabel(request.objective)} · data {catalog?.dataManifest.datasetVersion ?? "unknown"}</small> : null}
        </div>
        <div className="header-controls">
          <label>
            Current + N
            <input type="number" min={1} max={200} value={horizon} onChange={(event) => setHorizon(clamp(Number(event.target.value), 1, 200))} />
          </label>
          <button type="button" onClick={isAffinityBusy ? stop : refresh} disabled={!selected && !isAffinityBusy}>
            {isAffinityBusy ? <Pause size={15} /> : <Play size={15} />}
            {isAffinityBusy ? "Stop" : "Start"}
          </button>
        </div>
      </div>
      <Progress
        checked={affinityProgress?.checked ?? 0}
        total={affinityProgress?.total ?? (payload?.lines.length || 1)}
        busy={isAffinityBusy}
        label={affinityProgress ? `${affinityProgress.affinity} Lv ${affinityProgress.level}` : "Idle"}
      />
      <AffinityChart payload={payload} objective={objectiveLabel(request.objective)} />
      <div className="affinity-ranking" role="grid" aria-label="Affinity watch rankings">
        <div className="affinity-row table-header" role="row">
          {["Rank", "Affinity", "Start", "End", "Final stats"].map((label) => <span role="columnheader" key={label}>{label}</span>)}
        </div>
        {payload?.lines.map((line, index) => (
          <div className="affinity-row" key={line.affinity} role="row">
            <b role="gridcell">{index + 1}</b>
            <strong role="gridcell">{line.affinity}</strong>
            <span role="gridcell">{fixed1(line.startMetric)}</span>
            <span role="gridcell">{fixed1(line.endMetric)}</span>
            <small role="gridcell">{line.finalBuild ? statLine(line.finalBuild) : "-"}</small>
          </div>
        ))}
      </div>
      <div className="crossover-table" role="table" aria-label="Affinity watch breakpoints">
        {payload?.breakpoints.map((point) => (
          <div key={`${point.level}-${point.incomingAffinity}`} role="row">
            <span role="cell">Level {point.level}</span>
            <strong role="cell">{point.outgoingAffinity} to {point.incomingAffinity}</strong>
            <span role="cell">{fixed1(point.outgoingMetric)} / {fixed1(point.incomingMetric)}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function Progress({ checked, total, busy, label }: { checked: number; total: number; busy: boolean; label: string }) {
  const pct = Math.min(100, Math.max(0, (checked / Math.max(total, 1)) * 100));
  return (
    <div className="workspace-progress">
      <span>{busy ? `Tracing ${label} (${checked}/${total})` : label}</span>
      <div><i style={{ width: `${pct}%` }} /></div>
    </div>
  );
}

function AffinityChart({ payload, objective }: { payload: AffinityWatchPayloadDto | null; objective: string }) {
  const values = payload?.lines.flatMap((line) => line.points.map((point) => point.metric).filter((metric): metric is number => metric !== null)) ?? [];
  const domain = paddedMetricDomain(values);
  const observedMin = values.length ? Math.min(...values) : 0;
  const observedMax = values.length ? Math.max(...values) : 1;
  const levels = payload?.lines.flatMap((line) => line.points.map((point) => point.level)) ?? [];
  const firstLevel = levels.length ? Math.min(...levels) : null;
  const lastLevel = levels.length ? Math.max(...levels) : null;
  const plotFirstLevel = firstLevel ?? 0;
  const plotLastLevel = lastLevel ?? plotFirstLevel;
  const hasLines = Boolean(payload?.lines.length && firstLevel !== null && lastLevel !== null);
  return (
    <figure className="affinity-chart" aria-label={`${objective} by level for ${payload?.lines.length ?? 0} affinities`}>
      <figcaption>
        <span><small>Affinity crossover map</small><strong>{objective}</strong></span>
        <span>{hasLines ? `Level ${firstLevel} to ${lastLevel}` : "Awaiting analysis"}</span>
      </figcaption>
      {hasLines ? (
        <>
          <div className="affinity-legend" aria-hidden="true">
            {payload?.lines.map((line, index) => (
              <span key={line.affinity}>
                <i style={{ background: affinityColor(index) }} />
                <strong>{line.affinity}</strong>
                <small>{fixed1(line.startMetric)} → {fixed1(line.endMetric)}</small>
              </span>
            ))}
            <span className="affinity-crossover-key"><i /> Best-affinity crossover</span>
          </div>
          <div className="affinity-plot" aria-hidden="true">
            <div className="affinity-y-axis">
              <span>{fixed1(observedMax)}</span>
              <span>{fixed1((observedMin + observedMax) / 2)}</span>
              <span>{fixed1(observedMin)}</span>
            </div>
            <svg viewBox="0 0 1000 220" preserveAspectRatio="none">
              {[22, 110, 198].map((y) => <line className="affinity-grid-line" x1="0" x2="1000" y1={y} y2={y} key={y} />)}
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
                <g className="affinity-series-group" key={line.affinity}>
                  {contiguousMetricSegments(line.points).map((segment, segmentIndex) => (
                    <polyline
                      className="affinity-series"
                      points={linePoints(segment, plotFirstLevel, plotLastLevel, domain)}
                      stroke={affinityColor(index)}
                      key={`${line.affinity}-segment-${segmentIndex}`}
                    />
                  ))}
                  {line.points.filter((point) => point.metric !== null).map((point) => (
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
          <span>Start Affinity Watch to reveal scaling curves and exact crossover points.</span>
        </div>
      )}
      <table className="sr-only">
        <caption>{objective} affinity values by character level</caption>
        <thead><tr><th>Affinity</th><th>Level</th><th>{objective}</th></tr></thead>
        <tbody>{payload?.lines.flatMap((line) => line.points.map((point) => <tr key={`${line.affinity}-accessible-${point.level}`}><td>{line.affinity}</td><td>{point.level}</td><td>{fixed1(point.metric)}</td></tr>))}</tbody>
      </table>
    </figure>
  );
}

const AFFINITY_COLORS = [
  "#e0b967",
  "#79bfd0",
  "#8fc19c",
  "#d28a77",
  "#b59ad8",
  "#d7d2b6",
  "#d76969",
  "#729fe0",
  "#b7cb72",
  "#d889b3",
  "#63b7a2",
  "#e39252",
  "#8f86d8",
];

function affinityColor(index: number): string {
  return AFFINITY_COLORS[index % AFFINITY_COLORS.length];
}

function chartX(level: number, firstLevel: number, lastLevel: number): number {
  if (lastLevel === firstLevel) return 500;
  return 12 + ((level - firstLevel) / (lastLevel - firstLevel)) * 976;
}

function chartY(metric: number | null, domain: ReturnType<typeof paddedMetricDomain>): number {
  const ratio = metricRatio(metric, domain);
  return ratio === null ? 206 : 206 - ratio * 192;
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

function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
