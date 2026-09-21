import { Pause, Play } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { analysisStatus, analysisStatusLabel, type AnalysisOutcome } from "../../lib/analysis-status";
import { contiguousMetricSegments, metricRatio, paddedMetricDomain } from "../../lib/chart";
import { usePathJob, useRequestBudget } from "../../lib/hooks";
import { fixed1, objectiveLabel, objectiveUnit } from "../../lib/format";
import { clampHorizon, stableSignature } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { PathFinishedDto, PathPreviewDto, SolvedBuildDto } from "../../lib/types";

export function PathsView() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const target = useDesktopStore((state) => state.compareTarget);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const horizon = useDesktopStore((state) => state.pathHorizon);
  const setHorizon = useDesktopStore((state) => state.setPathHorizon);
  const pathMode = useDesktopStore((state) => state.pathMode);
  const setPathMode = useDesktopStore((state) => state.setPathMode);
  const paths = useDesktopStore((state) => state.paths);
  const isPathBusy = useDesktopStore((state) => state.isPathBusy);
  const setPathBusy = useDesktopStore((state) => state.setPathBusy);
  const beginPath = useDesktopStore((state) => state.beginPath);
  const pathGeneration = useDesktopStore((state) => state.pathGeneration);
  const activePathJobId = useDesktopStore((state) => state.activePathJobId);
  const setActivePathJobId = useDesktopStore((state) => state.setActivePathJobId);
  const pathProgress = useDesktopStore((state) => state.pathProgress);
  const setPathProgress = useDesktopStore((state) => state.setPathProgress);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const { base } = useRequestBudget(catalog, request, lockedStatMode);
  const effectiveHorizon = clampHorizon(request, horizon);
  const signature = stableSignature({
    base,
    selected,
    target,
    horizon: effectiveHorizon,
    pathMode,
  });
  const [runOutcome, setRunOutcome] = useState<AnalysisOutcome>(null);
  useEffect(() => setRunOutcome(null), [signature]);
  const pathSignature = useDesktopStore((state) => state.pathSignature);
  const status = analysisStatus({
    busy: isPathBusy,
    resultSignature: pathSignature,
    requestSignature: signature,
    hasResult: paths.length > 0,
    outcome: runOutcome,
  });

  const startPathJob = usePathJob({
    isPathBusy,
    generation: pathGeneration,
    setPathProgress,
    onStarted: (jobId, generation) => {
      const current = useDesktopStore.getState();
      if (!current.isPathBusy || current.pathGeneration !== generation || current.activePathSignature !== signature) {
        throw new DOMException("Calculation stopped.", "AbortError");
      }
      setActivePathJobId(jobId);
    },
  });

  async function refresh() {
    if (!selected) {
      pushNotice({ scope: "paths", tone: "warning", message: "Pick a selected result first." });
      return;
    }
    if (effectiveHorizon <= 0) {
      pushNotice({ scope: "paths", tone: "warning", message: "Combat stats are already capped. There is no forward path to trace." });
      return;
    }
    setRunOutcome(null);
    const generation = beginPath(signature);
    if (effectiveHorizon < horizon) {
      pushNotice({ scope: "paths", tone: "info", message: `Horizon capped at Current +${effectiveHorizon}.` });
    }
    try {
      const requests = [
        { base, solved: selected, levelsAhead: effectiveHorizon, title: "Selected", mode: pathMode },
        ...(target ? [{ base, solved: target, levelsAhead: effectiveHorizon, title: "Compare", mode: pathMode }] : []),
      ];
      const finished = await startPathJob(() => api.startPathPreview(requests), generation);
      finishPathPreview(finished, generation);
    } catch (error) {
      const current = useDesktopStore.getState();
      if (
        current.isPathBusy &&
        current.pathGeneration === generation &&
        current.activePathSignature === signature
      ) {
        if (error instanceof DOMException && error.name === "AbortError") {
          current.pushNotice({ scope: "paths", tone: "warning", message: "Path preview stopped." });
          setRunOutcome("cancelled");
        } else {
          setError(error instanceof Error ? error.message : String(error));
          setRunOutcome("failed");
        }
        setPathBusy(false);
        setActivePathJobId(null);
        setPathProgress(null);
      }
    }
  }

  async function stop() {
    setRunOutcome("cancelled");
    if (!activePathJobId) setPathBusy(false);
    if (activePathJobId) {
      try {
        await api.cancelPathPreview(activePathJobId);
      } catch (error) {
        const current = useDesktopStore.getState();
        if (current.isPathBusy && current.activePathJobId === activePathJobId) {
          setError(error instanceof Error ? error.message : String(error));
          setRunOutcome("failed");
        }
      }
    }
  }

  function finishPathPreview(payload: PathFinishedDto, generation: number) {
    const current = useDesktopStore.getState();
    if (
      generation !== current.pathGeneration ||
      current.activePathSignature !== signature ||
      payload.jobId !== current.activePathJobId
    ) return;
    if (payload.error) {
      current.setError(payload.error);
      setRunOutcome("failed");
    } else if (!payload.cancelled) {
      current.setPaths(payload.paths, signature);
      setRunOutcome(null);
    } else {
      current.pushNotice({ scope: "paths", tone: "warning", message: "Path preview stopped." });
      setRunOutcome("cancelled");
    }
    current.setPathBusy(false);
    current.setActivePathJobId(null);
    current.setPathProgress(null);
  }

  return (
    <section className="workspace-panel paths-panel">
      <div className="workspace-header analysis-workspace-header">
        <div className="workspace-heading-copy">
          <h1>{pathMode === "no_respec" ? "No-respec Paths" : "Optimum Envelope"}</h1>
          <span>{selected ? `Current +${effectiveHorizon} ${target ? "selected and compare lanes" : "selected lane"}` : "Requires selected result"}</span>
          {selected ? <small className="selected-summary">{selected.weaponName} / {selected.affinity} / +{selected.upgrade} · {objectiveLabel(request.objective)} · data {catalog?.dataManifest.datasetVersion ?? "unknown"}{target ? ` · vs ${target.weaponName} / ${target.affinity} / +${target.upgrade}` : ""}</small> : null}
        </div>
        <div className="header-controls">
          <div className="segmented" aria-label="Path mode">
            <button type="button" className={pathMode === "no_respec" ? "active" : ""} aria-pressed={pathMode === "no_respec"} onClick={() => setPathMode("no_respec")}>No respec</button>
            <button type="button" className={pathMode === "optimum_envelope" ? "active" : ""} aria-pressed={pathMode === "optimum_envelope"} onClick={() => setPathMode("optimum_envelope")}>Envelope</button>
          </div>
          <label>
            Current + N
            <input
              type="number"
              min={1}
              max={200}
              value={horizon}
              onChange={(event) => setHorizon(clamp(Number(event.target.value), 1, 200))}
            />
          </label>
          <button type="button" className="analysis-action" onClick={isPathBusy ? stop : refresh} disabled={!selected && !isPathBusy}>
            {isPathBusy ? <Pause size={15} /> : <Play size={15} />}
            {isPathBusy ? "Stop" : "Trace paths"}
          </button>
        </div>
      </div>
      <Progress checked={pathProgress?.checked ?? 0} total={pathProgress?.total ?? (paths.length || 1)} status={status} resultCount={paths.length} />
      <small className="path-mode-note">{pathMode === "no_respec" ? "Terminal allocation is globally optimized; the point-by-point order is greedy and never removes a stat." : "Each level is independently optimized and may mark respec when the best allocation moves points."}</small>
      <div className="path-lanes">
        <LaneSummary title="Selected" path={paths.find((path) => path.title === "Selected")} row={selected} />
        <LaneSummary title="Compare" path={paths.find((path) => path.title === "Compare")} row={target} />
      </div>
      <PathChart key={`chart:${pathSignature ?? "empty"}`} paths={paths} objective={objectiveLabel(request.objective)} unit={objectiveUnit(request.objective)} />
      <PathSteps key={pathSignature ?? "empty"} paths={paths} objective={request.objective} />
    </section>
  );
}

function PathSteps({ paths, objective }: { paths: PathPreviewDto[]; objective: Parameters<typeof objectiveLabel>[0] }) {
  const [page, setPage] = useState(0);
  const levels = [...new Set(paths.flatMap(path => path.steps.map(step => step.level)))].sort((a, b) => a - b);
  const pageCount = Math.max(1, Math.ceil(levels.length / 10));
  const currentPage = Math.min(page, pageCount - 1);
  const shownLevels = levels.slice(currentPage * 10, currentPage * 10 + 10);
  if (!levels.length) return null;
  return (
    <div className="path-steps">
      <div className="path-step-pagination" aria-label="Path level pages">
        <button type="button" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>Previous levels</button>
        <label>
          Levels
          <select aria-label="Path level range" value={currentPage} onChange={event => setPage(Number(event.target.value))}>
            {Array.from({ length: pageCount }, (_, index) => (
              <option key={index} value={index}>{levels[index * 10]}–{levels[Math.min(index * 10 + 9, levels.length - 1)]}</option>
            ))}
          </select>
        </label>
        <button type="button" disabled={currentPage + 1 === pageCount} onClick={() => setPage(currentPage + 1)}>Next levels</button>
      </div>
      <div className="step-table path-step-table" role="table" aria-label="Path steps">
        <div className="step-row path-step-row table-header" role="row" style={{ gridTemplateColumns: `54px repeat(${paths.length}, minmax(0, 1fr))` }}>
          <span role="columnheader">Level</span>
          {paths.map(path => <span role="columnheader" key={path.title}>{path.title}<small>{path.solved.weaponName} / {path.solved.affinity}</small></span>)}
        </div>
        {shownLevels.map(level => (
          <div className="step-row path-step-row" role="row" key={level} style={{ gridTemplateColumns: `54px repeat(${paths.length}, minmax(0, 1fr))` }}>
            <b role="cell">{level}</b>
            {paths.map(path => {
              const index = path.steps.findIndex(step => step.level === level);
              const step = path.steps[index];
              const previous = path.steps[index - 1]?.metric ?? null;
              const gain = step?.metric != null && previous !== null ? step.metric - previous : null;
              return <span role="cell" className="path-step-build" key={path.title}>
                {step ? <>
                  <strong>{fixed1(step.metric)} {objectiveUnit(objective)}</strong>
                  <small>
                    {index === 0 ? "Starting stats" : gain === null ? "Gain unavailable" : `Gain ${fixed1(gain)}`}
                    {index > 0 ? ` | ${step.addedStat === "respec" ? "Respec required" : step.addedStat ? `Added ${step.addedStat.toUpperCase()}` : "No stat added"}` : ""}
                    {step.requirementGap > 0 ? ` | Requirement gap ${step.requirementGap}` : ""}
                  </small>
                  <span>STR {step.stats.strStat} / DEX {step.stats.dex} / INT {step.stats.intStat} / FAI {step.stats.fai} / ARC {step.stats.arc}</span>
                </> : "Unavailable"}
              </span>;
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function LaneSummary({ title, path, row }: { title: string; path: PathPreviewDto | undefined; row: SolvedBuildDto | null }) {
  const solved = path?.solved ?? row;
  return (
    <div className="path-lane">
      <strong>{title}</strong>
      {solved ? (
        <>
          <span>{solved.weaponName} / {solved.affinity} / {solved.aowName ?? "Unspecified skill"} / +{solved.upgrade}</span>
          <small>{path ? `${path.steps.length} steps, final ${fixed1(path.steps.at(-1)?.metric)}` : "Ready to trace"}</small>
        </>
      ) : (
        <span>No compare lane selected.</span>
      )}
    </div>
  );
}

function Progress({ checked, total, status, resultCount }: { checked: number; total: number; status: ReturnType<typeof analysisStatus>; resultCount: number }) {
  const displayChecked = status === "completed" ? total : checked;
  const pct = Math.min(100, Math.max(0, (displayChecked / Math.max(total, 1)) * 100));
  const label = status === "running"
    ? `Tracing paths ${checked}/${total}`
    : status === "completed"
      ? `Completed · ${resultCount} lane${resultCount === 1 ? "" : "s"}`
      : analysisStatusLabel(status);
  return (
    <div className={`workspace-progress analysis-progress status-${status}`} data-analysis-status={status}>
      <span role="status">{label}</span>
      <div><i style={{ width: `${pct}%` }} /></div>
    </div>
  );
}

function PathChart({ paths, objective, unit }: { paths: PathPreviewDto[]; objective: string; unit: string }) {
  const [pointIndex, setPointIndex] = useState(0);
  const values = paths.flatMap((path) => path.steps.map((step) => step.metric).filter((metric): metric is number => metric !== null));
  const domain = paddedMetricDomain(values);
  const levels = [...new Set(paths.flatMap((path) => path.steps.map((step) => step.level)))].sort((a, b) => a - b);
  const firstLevel = levels[0] ?? 0;
  const lastLevel = levels.at(-1) ?? firstLevel;
  const selectedIndex = Math.min(pointIndex, Math.max(0, levels.length - 1));
  const selectedLevel = levels[selectedIndex];
  const x = (level: number) => lastLevel === firstLevel ? 500 : 12 + (level - firstLevel) / (lastLevel - firstLevel) * 976;
  const y = (metric: number | null) => 206 - (metricRatio(metric, domain) ?? 0) * 192;
  const axisMetrics = [domain.max, (domain.min + domain.max) / 2, domain.min];
  const axisLevels = [...new Set(Array.from({ length: 5 }, (_, index) => Math.round(firstLevel + (lastLevel - firstLevel) * index / 4)))];
  const inspected = paths.map(path => ({ title: path.title, step: path.steps.find(step => step.level === selectedLevel) }));
  const valueText = `Level ${selectedLevel}; ${inspected.map(({ title, step }) => `${title} ${step?.metric == null ? "unavailable" : `${fixed1(step.metric)} ${unit}`}`).join("; ")}`;
  return (
    <figure className="path-chart" aria-label={`${objective} by character level for ${paths.length} path lanes`}>
      <figcaption>
        <span><small>Metric by character level</small><strong>{objective} ({unit})</strong></span>
        <span>{levels.length ? `Level ${firstLevel} to ${lastLevel}` : "Awaiting analysis"}</span>
      </figcaption>
      {levels.length ? <>
        <div className="chart-legend" aria-label="Path chart legend">
          {paths.map((path, index) => <span className={`series-${index}`} key={path.title}>{path.title}</span>)}
          <span className="path-marker-key">○ Allocation changes (sampled)</span>
        </div>
        <div className="path-plot">
          <div className="path-y-axis" aria-hidden="true">
            {axisMetrics.map((metric, index) => <span key={index} style={{ top: `${y(metric) / 2.2}%` }}>{fixed1(metric)}</span>)}
          </div>
          <svg viewBox="0 0 1000 220" preserveAspectRatio="none" aria-hidden="true" onPointerMove={event => {
            const bounds = event.currentTarget.getBoundingClientRect();
            const level = firstLevel + ((event.clientX - bounds.left) / bounds.width * 1000 - 12) / 976 * (lastLevel - firstLevel);
            setPointIndex(levels.reduce((best, candidate, index) => Math.abs(candidate - level) < Math.abs(levels[best] - level) ? index : best, 0));
          }}>
            {axisMetrics.map((metric, index) => <line className="path-grid-line" x1="12" x2="988" y1={y(metric)} y2={y(metric)} key={index} />)}
            <line className="path-cursor" x1={x(selectedLevel)} x2={x(selectedLevel)} y1="14" y2="206" />
            {paths.map((path, pathIndex) => {
              let lastMarker = -Infinity;
              const markers = path.steps.filter((step, index) => {
                if (step.metric === null || !index || !step.addedStat || step.addedStat === path.steps[index - 1].addedStat || step.level - lastMarker < Math.max(1, (lastLevel - firstLevel) / 8)) return false;
                lastMarker = step.level;
                return true;
              });
              const point = inspected[pathIndex].step;
              return <g className={`path-series-group series-${pathIndex}`} key={path.title}>
                {contiguousMetricSegments(path.steps).map((segment, index) => <g key={index}>
                  <polyline className="path-series" points={segment.map(step => `${x(step.level)},${y(step.metric)}`).join(" ")} />
                  {segment.length === 1 ? <circle className="path-isolated-point" cx={x(segment[0].level)} cy={y(segment[0].metric)} r="4" /> : null}
                </g>)}
                {markers.map(step => <circle className="path-change-marker" key={step.level} cx={x(step.level)} cy={y(step.metric)} r="4" />)}
                {point?.metric != null ? <circle className="path-selected-point" cx={x(point.level)} cy={y(point.metric)} r="5" /> : null}
              </g>;
            })}
          </svg>
          <div className="path-x-axis" aria-hidden="true">
            {axisLevels.map(level => <span key={level} style={{ left: `${x(level) / 10}%` }}>{level}</span>)}
          </div>
        </div>
        <label className="path-inspect-control">
          Character level
          <input type="range" aria-label="Inspect path level" aria-valuetext={valueText} min={0} max={levels.length - 1} value={selectedIndex} onChange={event => setPointIndex(Number(event.target.value))} />
        </label>
        <div className="path-point-summary" aria-live="polite" aria-atomic="true">
          <strong>Level {selectedLevel}</strong>
          {inspected.map(({ title, step }, index) => <span className={`series-${index}`} key={title}>
            {title} <b>{step?.metric == null ? "Unavailable" : `${fixed1(step.metric)} ${unit}`}</b>
            <small>{step?.addedStat === "respec" ? "Respec required" : step?.addedStat ? `Added ${step.addedStat.toUpperCase()}` : step?.level === firstLevel ? "Starting stats" : "No stat added"}</small>
          </span>)}
        </div>
        <small className="path-marker-note">Markers show changes in added stat or respec; nearby changes are omitted. Inspect any level above.</small>
      </> : <p className="path-chart-empty">Trace paths to compare progress by character level.</p>}
      <table className="sr-only">
        <caption>{objective} ({unit}) path values by character level</caption>
        <thead><tr><th>Lane</th><th>Level</th><th>{objective} ({unit})</th></tr></thead>
        <tbody>{paths.flatMap((path) => path.steps.map((step) => <tr key={`${path.title}-accessible-${step.level}`}><td>{path.title}</td><td>{step.level}</td><td>{fixed1(step.metric)} {unit}</td></tr>))}</tbody>
      </table>
    </figure>
  );
}

function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
