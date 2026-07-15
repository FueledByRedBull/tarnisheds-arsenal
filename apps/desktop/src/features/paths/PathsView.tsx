import { Pause, Play } from "lucide-react";
import { useMemo, useRef } from "react";
import { api, hasTauriRuntime } from "../../lib/api";
import { cachedPathPreview } from "../../lib/analysis-cache";
import { usePathJob, useRequestBudget } from "../../lib/hooks";
import { fixed1, objectiveLabel } from "../../lib/format";
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
  const paths = useDesktopStore((state) => state.paths);
  const setPaths = useDesktopStore((state) => state.setPaths);
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
  const fallbackRequest = useRef<AbortController | null>(null);
  const { base } = useRequestBudget(catalog, request, lockedStatMode);
  const effectiveHorizon = clampHorizon(request, horizon);
  const signature = stableSignature({
    selected,
    target,
    objective: request.objective,
    level: base.characterLevel,
    horizon: effectiveHorizon,
  });

  usePathJob({
    activePathJobId,
    isPathBusy,
    generation: pathGeneration,
    setPathProgress,
    finish: finishPathPreview,
  });

  async function refresh() {
    let fallbackController: AbortController | null = null;
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
    const generation = beginPath(signature);
    try {
      const requests = [
        { base, solved: selected, levelsAhead: effectiveHorizon, title: "Selected" },
        ...(target ? [{ base, solved: target, levelsAhead: effectiveHorizon, title: "Compare" }] : []),
      ];
      if (hasTauriRuntime()) {
        const { jobId } = await api.startPathPreview(requests);
        const current = useDesktopStore.getState();
        if (
          current.pathGeneration !== generation ||
          current.activePathSignature !== signature
        ) {
          await api.cancelPathPreview(jobId);
          return;
        }
        setActivePathJobId(jobId);
      } else {
        fallbackRequest.current?.abort();
        fallbackController = new AbortController();
        fallbackRequest.current = fallbackController;
        const next = await Promise.all(
          requests.map((entry) => cachedPathPreview(
            entry.base,
            entry.solved,
            entry.levelsAhead,
            entry.title,
            fallbackController?.signal,
          )),
        );
        const current = useDesktopStore.getState();
        if (
          current.pathGeneration !== generation ||
          current.activePathSignature !== signature
        ) {
          fallbackController.abort();
          return;
        }
        setPaths(next, signature);
        setPathBusy(false);
      }
    } catch (error) {
      const current = useDesktopStore.getState();
      if (fallbackController?.signal.aborted) {
        if (current.pathGeneration === generation) setPathBusy(false);
        return;
      }
      if (
        current.pathGeneration === generation &&
        current.activePathSignature === signature
      ) {
        setError(error instanceof Error ? error.message : String(error));
        setPathBusy(false);
      }
    } finally {
      if (fallbackRequest.current === fallbackController) fallbackRequest.current = null;
    }
  }

  async function stop() {
    fallbackRequest.current?.abort();
    fallbackRequest.current = null;
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
    if (payload.error) current.setError(payload.error);
    if (!payload.cancelled) current.setPaths(payload.paths, signature);
    else current.pushNotice({ scope: "paths", tone: "warning", message: "Path preview stopped." });
    current.setPathBusy(false);
    current.setActivePathJobId(null);
    current.setPathProgress(null);
  }

  return (
    <section className="workspace-panel paths-panel">
      <div className="workspace-header">
        <div>
          <h1>Paths</h1>
          <span>{selected ? `Current +${effectiveHorizon} ${target ? "selected and compare lanes" : "selected lane"}` : "Requires selected result"}</span>
          {selected ? <small className="selected-summary">{selected.weaponName} / {selected.affinity} / +{selected.upgrade} · {objectiveLabel(request.objective)} · data {catalog?.dataManifest.datasetVersion ?? "unknown"}{target ? ` · vs ${target.weaponName} / ${target.affinity} / +${target.upgrade}` : ""}</small> : null}
        </div>
        <div className="header-controls">
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
          <button type="button" onClick={isPathBusy ? stop : refresh} disabled={!selected && !isPathBusy}>
            {isPathBusy ? <Pause size={15} /> : <Play size={15} />}
            {isPathBusy ? "Stop" : "Start"}
          </button>
        </div>
      </div>
      <Progress checked={pathProgress?.checked ?? 0} total={pathProgress?.total ?? (paths.length || 1)} busy={isPathBusy} />
      <div className="path-lanes">
        <LaneSummary title="Selected" path={paths.find((path) => path.title === "Selected")} row={selected} />
        <LaneSummary title="Compare" path={paths.find((path) => path.title === "Compare")} row={target} />
      </div>
      <PathChart paths={paths} objective={objectiveLabel(request.objective)} />
      <div className="step-table path-step-table" role="grid" aria-label="Path steps">
        <div className="step-row path-step-row table-header" role="row">
          {["Lane", "Level", objectiveLabel(request.objective), "Gain", "Added", "Gap", "Stats"].map((label) => <span role="columnheader" key={label}>{label}</span>)}
        </div>
        {paths.flatMap((path) =>
          path.steps.map((step, index) => {
            const previous = index > 0 ? path.steps[index - 1].metric : null;
            const gain = step.metric !== null && previous !== null ? step.metric - previous : null;
            return (
              <div key={`${path.title}-${step.level}`} className="step-row path-step-row" role="row">
                <span role="gridcell">{path.title}</span>
                <b role="gridcell">{step.level}</b>
                <strong role="gridcell">{fixed1(step.metric)}</strong>
                <span role="gridcell">{fixed1(gain)}</span>
                <span role="gridcell">{step.addedStat ?? "start"}</span>
                <span role="gridcell">{step.requirementGap}</span>
                <span role="gridcell">STR {step.stats.strStat} DEX {step.stats.dex} INT {step.stats.intStat} FAI {step.stats.fai} ARC {step.stats.arc}</span>
              </div>
            );
          }),
        )}
      </div>
    </section>
  );
}

function LaneSummary({ title, path, row }: { title: string; path: PathPreviewDto | undefined; row: SolvedBuildDto | null }) {
  const solved = path?.solved ?? row;
  return (
    <div className="path-lane">
      <strong>{title}</strong>
      {solved ? (
        <>
          <span>{solved.weaponName} / {solved.affinity} / {solved.aowName ?? "Native"} / +{solved.upgrade}</span>
          <small>{path ? `${path.steps.length} steps, final ${fixed1(path.steps.at(-1)?.metric)}` : "Ready to trace"}</small>
        </>
      ) : (
        <span>No compare lane selected.</span>
      )}
    </div>
  );
}

function Progress({ checked, total, busy }: { checked: number; total: number; busy: boolean }) {
  const pct = Math.min(100, Math.max(0, (checked / Math.max(total, 1)) * 100));
  return (
    <div className="workspace-progress">
      <span>{busy ? `Tracing paths ${checked}/${total}` : "Idle"}</span>
      <div><i style={{ width: `${pct}%` }} /></div>
    </div>
  );
}

function PathChart({ paths, objective }: { paths: PathPreviewDto[]; objective: string }) {
  const values = paths.flatMap((path) => path.steps.map((step) => step.metric).filter((metric): metric is number => metric !== null));
  const min = Math.min(...values, 0);
  const max = Math.max(...values, 1);
  const levels = paths.flatMap((path) => path.steps.map((step) => step.level));
  const firstLevel = levels.length ? Math.min(...levels) : null;
  const lastLevel = levels.length ? Math.max(...levels) : null;
  return (
    <figure className="path-chart" aria-label={`${objective} by level for ${paths.length} path lanes`}>
      <figcaption><strong>{objective}</strong><span>Level →</span></figcaption>
      <div className="chart-axis" aria-hidden="true">
        <span>{fixed1(max)}</span><span>{firstLevel === null ? "No levels" : `Level ${firstLevel} to ${lastLevel}`}</span><span>{fixed1(min)}</span>
      </div>
      <div className="chart-legend" aria-hidden="true">
        {paths.map((path, index) => <span className={`series-${index % 3}`} key={path.title}>{path.title}</span>)}
        <span className="breakpoint-key">◆ Stat breakpoint</span>
      </div>
      {paths.map((path, pathIndex) => (
        <div className={`spark-line series-${pathIndex % 3}`} key={path.title} aria-hidden="true">
          {path.steps.map((step) => (
            <span
              className={step.addedStat ? "breakpoint" : undefined}
              key={`${path.title}-${step.level}`}
              style={{ height: `${metricHeight(step.metric, min, max)}%` }}
              title={`${path.title} ${step.level}: ${fixed1(step.metric)}`}
            />
          ))}
        </div>
      ))}
      <table className="sr-only">
        <caption>{objective} path values by character level</caption>
        <thead><tr><th>Lane</th><th>Level</th><th>{objective}</th></tr></thead>
        <tbody>{paths.flatMap((path) => path.steps.map((step) => <tr key={`${path.title}-accessible-${step.level}`}><td>{path.title}</td><td>{step.level}</td><td>{fixed1(step.metric)}</td></tr>))}</tbody>
      </table>
    </figure>
  );
}

function metricHeight(metric: number | null, min: number, max: number): number {
  if (metric === null) return 8;
  return 12 + ((metric - min) / Math.max(max - min, 1)) * 88;
}

function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
