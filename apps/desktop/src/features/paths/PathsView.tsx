import { Pause, Play } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { analysisStatus, analysisStatusLabel, type AnalysisOutcome } from "../../lib/analysis-status";
import { metricRatio, paddedMetricDomain } from "../../lib/chart";
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

  usePathJob({
    activePathJobId,
    isPathBusy,
    generation: pathGeneration,
    setPathProgress,
    finish: finishPathPreview,
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
    if (effectiveHorizon < horizon) {
      pushNotice({ scope: "paths", tone: "info", message: `Horizon capped at Current +${effectiveHorizon}.` });
    }
    setRunOutcome(null);
    const generation = beginPath(signature);
    try {
      const requests = [
        { base, solved: selected, levelsAhead: effectiveHorizon, title: "Selected", mode: pathMode },
        ...(target ? [{ base, solved: target, levelsAhead: effectiveHorizon, title: "Compare", mode: pathMode }] : []),
      ];
      const { jobId } = await api.startPathPreview(requests);
      const current = useDesktopStore.getState();
      if (
        !current.isPathBusy ||
        current.pathGeneration !== generation ||
        current.activePathSignature !== signature
      ) {
        await api.cancelPathPreview(jobId);
        return;
      }
      setActivePathJobId(jobId);
    } catch (error) {
      const current = useDesktopStore.getState();
      if (
        current.isPathBusy &&
        current.pathGeneration === generation &&
        current.activePathSignature === signature
      ) {
        setError(error instanceof Error ? error.message : String(error));
        setRunOutcome("failed");
        setPathBusy(false);
      }
    }
  }

  async function stop() {
    setRunOutcome("cancelled");
    if (!activePathJobId) setPathBusy(false);
    if (activePathJobId) await api.cancelPathPreview(activePathJobId);
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
      <PathChart paths={paths} objective={objectiveLabel(request.objective)} unit={objectiveUnit(request.objective)} />
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
      <div className="step-table path-step-table" role="grid" aria-label="Path steps">
        <div className="step-row path-step-row table-header" role="row" style={{ gridTemplateColumns: `54px repeat(${paths.length}, minmax(0, 1fr))` }}>
          <span role="columnheader">Level</span>
          {paths.map(path => <span role="columnheader" key={path.title}>{path.title}<small>{path.solved.weaponName} / {path.solved.affinity}</small></span>)}
        </div>
        {shownLevels.map(level => (
          <div className="step-row path-step-row" role="row" key={level} style={{ gridTemplateColumns: `54px repeat(${paths.length}, minmax(0, 1fr))` }}>
            <b role="gridcell">{level}</b>
            {paths.map(path => {
              const index = path.steps.findIndex(step => step.level === level);
              const step = path.steps[index];
              const previous = path.steps[index - 1]?.metric ?? null;
              const gain = step?.metric != null && previous !== null ? step.metric - previous : null;
              return <span role="gridcell" className="path-step-build" key={path.title}>
                {step ? <>
                  <strong>{fixed1(step.metric)} {objectiveUnit(objective)}</strong>
                  <small>
                    {index === 0 ? "Starting stats" : gain === null ? "Gain unavailable" : `Gain ${fixed1(gain)}`}
                    {index > 0 ? ` | ${step.addedStat ? `Added ${step.addedStat.toUpperCase()}` : "No stat added"}` : ""}
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
  const values = paths.flatMap((path) => path.steps.map((step) => step.metric).filter((metric): metric is number => metric !== null));
  const domain = paddedMetricDomain(values);
  const observedMin = values.length ? Math.min(...values) : 0;
  const observedMax = values.length ? Math.max(...values) : 1;
  const levels = paths.flatMap((path) => path.steps.map((step) => step.level));
  const firstLevel = levels.length ? Math.min(...levels) : null;
  const lastLevel = levels.length ? Math.max(...levels) : null;
  return (
    <figure className="path-chart" aria-label={`${objective} by character level for ${paths.length} path lanes`}>
      <figcaption>
        <span><small>Metric by character level</small><strong>{objective} ({unit})</strong></span>
        <span>{firstLevel === null ? "Awaiting analysis" : `Level ${firstLevel} to ${lastLevel}`}</span>
      </figcaption>
      <div className="chart-axis" aria-hidden="true">
        <span>{fixed1(observedMax)}</span><span>Character level</span><span>{fixed1(observedMin)}</span>
      </div>
      <div className="chart-legend" aria-label="Path chart legend">
        {paths.map((path, index) => <span className={`series-${index % 3}`} key={path.title}>{path.title}</span>)}
        <span className="breakpoint-key">◆ Stat breakpoint</span>
      </div>
      {paths.map((path, pathIndex) => (
        <div
          className={`spark-line series-${pathIndex % 3}`}
          key={path.title}
          aria-hidden="true"
          style={{ gridTemplateColumns: `repeat(${Math.max(path.steps.length, 1)}, minmax(0, 1fr))` }}
        >
          {path.steps.map((step) => (
            <span
              className={[step.addedStat ? "breakpoint" : "", step.metric === null ? "missing" : ""].filter(Boolean).join(" ") || undefined}
              key={`${path.title}-${step.level}`}
              style={{ height: `${metricHeight(step.metric, domain)}%` }}
              title={`${path.title} level ${step.level}: ${step.metric === null ? "unavailable" : `${fixed1(step.metric)} ${unit}`}`}
            />
          ))}
        </div>
      ))}
      <table className="sr-only">
        <caption>{objective} ({unit}) path values by character level</caption>
        <thead><tr><th>Lane</th><th>Level</th><th>{objective} ({unit})</th></tr></thead>
        <tbody>{paths.flatMap((path) => path.steps.map((step) => <tr key={`${path.title}-accessible-${step.level}`}><td>{path.title}</td><td>{step.level}</td><td>{fixed1(step.metric)} {unit}</td></tr>))}</tbody>
      </table>
    </figure>
  );
}

function metricHeight(metric: number | null, domain: ReturnType<typeof paddedMetricDomain>): number {
  const ratio = metricRatio(metric, domain);
  if (ratio === null) return 8;
  return 12 + ratio * 88;
}

function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
