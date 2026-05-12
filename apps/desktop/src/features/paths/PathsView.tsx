import { Pause, Play } from "lucide-react";
import { useEffect, useMemo } from "react";
import { api, hasTauriRuntime } from "../../lib/api";
import { fixed1 } from "../../lib/format";
import { buildOptimizeRequest, clampHorizon, stableSignature } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { PathPreviewDto } from "../../lib/types";

export function PathsView() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const target = useDesktopStore((state) => state.compareTarget);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const horizon = useDesktopStore((state) => state.horizon);
  const setHorizon = useDesktopStore((state) => state.setHorizon);
  const paths = useDesktopStore((state) => state.paths);
  const setPaths = useDesktopStore((state) => state.setPaths);
  const isPathBusy = useDesktopStore((state) => state.isPathBusy);
  const setPathBusy = useDesktopStore((state) => state.setPathBusy);
  const activePathJobId = useDesktopStore((state) => state.activePathJobId);
  const setActivePathJobId = useDesktopStore((state) => state.setActivePathJobId);
  const pathProgress = useDesktopStore((state) => state.pathProgress);
  const setPathProgress = useDesktopStore((state) => state.setPathProgress);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const base = useMemo(
    () => buildOptimizeRequest(catalog, request, lockedStatMode),
    [catalog, lockedStatMode, request],
  );
  const effectiveHorizon = clampHorizon(request, horizon);
  const signature = stableSignature({
    selected,
    target,
    objective: request.objective,
    level: base.characterLevel,
    horizon: effectiveHorizon,
  });

  useEffect(() => {
    let unlistenProgress: (() => void) | undefined;
    let unlistenFinished: (() => void) | undefined;
    api.onPathProgress((payload) => {
      if (!activePathJobId || payload.jobId === activePathJobId) setPathProgress(payload);
    }).then((unlisten) => {
      unlistenProgress = unlisten;
    });
    api.onPathFinished((payload) => {
      if (activePathJobId && payload.jobId !== activePathJobId) return;
      if (payload.error) setError(payload.error);
      if (!payload.cancelled) setPaths(payload.paths, signature);
      else pushNotice({ scope: "paths", tone: "warning", message: "Path preview stopped." });
      setPathBusy(false);
      setActivePathJobId(null);
      setPathProgress(null);
    }).then((unlisten) => {
      unlistenFinished = unlisten;
    });
    return () => {
      unlistenProgress?.();
      unlistenFinished?.();
    };
  }, [activePathJobId, pushNotice, setActivePathJobId, setError, setPathBusy, setPathProgress, setPaths, signature]);

  async function refresh() {
    if (!selected || !target) {
      pushNotice({ scope: "paths", tone: "warning", message: "Pick a selected result and comparison target first." });
      return;
    }
    if (effectiveHorizon <= 0) {
      pushNotice({ scope: "paths", tone: "warning", message: "Combat stats are already capped. There is no forward path to trace." });
      return;
    }
    if (effectiveHorizon < horizon) {
      pushNotice({ scope: "paths", tone: "info", message: `Horizon capped at Current +${effectiveHorizon}.` });
    }
    setPathBusy(true);
    setPathProgress(null);
    try {
      const requests = [
        { base, solved: selected, levelsAhead: effectiveHorizon, title: "Selected" },
        { base, solved: target, levelsAhead: effectiveHorizon, title: "Compare" },
      ];
      if (hasTauriRuntime()) {
        const { jobId } = await api.startPathPreview(requests);
        setActivePathJobId(jobId);
      } else {
        const next = await Promise.all(requests.map((entry) => api.buildPathPreview(entry.base, entry.solved, entry.levelsAhead, entry.title)));
        setPaths(next, signature);
        setPathBusy(false);
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
      setPathBusy(false);
    }
  }

  async function stop() {
    if (activePathJobId) await api.cancelPathPreview(activePathJobId);
  }

  return (
    <section className="workspace-panel paths-panel">
      <div className="workspace-header">
        <div>
          <h1>Paths</h1>
          <span>{selected && target ? `Current +${effectiveHorizon} selected and compare lanes` : "Requires selected and compare target"}</span>
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
          <button type="button" onClick={isPathBusy ? stop : refresh}>
            {isPathBusy ? <Pause size={15} /> : <Play size={15} />}
            {isPathBusy ? "Stop" : "Refresh"}
          </button>
        </div>
      </div>
      <Progress checked={pathProgress?.checked ?? 0} total={pathProgress?.total ?? (paths.length || 1)} busy={isPathBusy} />
      <div className="path-lanes">
        <LaneSummary title="Selected" path={paths.find((path) => path.title === "Selected")} />
        <LaneSummary title="Compare" path={paths.find((path) => path.title === "Compare")} />
      </div>
      <PathChart paths={paths} />
      <div className="step-table path-step-table">
        {paths.flatMap((path) =>
          path.steps.map((step, index) => {
            const previous = index > 0 ? path.steps[index - 1].metric : null;
            const gain = step.metric !== null && previous !== null ? step.metric - previous : null;
            return (
              <div key={`${path.title}-${step.level}`} className="step-row path-step-row">
                <span>{path.title}</span>
                <b>{step.level}</b>
                <strong>{fixed1(step.metric)}</strong>
                <span>{fixed1(gain)}</span>
                <span>{step.addedStat ?? "start"}</span>
                <span>{step.requirementGap}</span>
                <span>STR {step.stats.strStat} DEX {step.stats.dex} INT {step.stats.intStat} FAI {step.stats.fai} ARC {step.stats.arc}</span>
              </div>
            );
          }),
        )}
      </div>
    </section>
  );
}

function LaneSummary({ title, path }: { title: string; path: PathPreviewDto | undefined }) {
  return (
    <div className="path-lane">
      <strong>{title}</strong>
      {path ? (
        <>
          <span>{path.solved.weaponName} / {path.solved.affinity} / {path.solved.aowName ?? "Native"} / +{path.solved.upgrade}</span>
          <small>{path.steps.length} steps, final {fixed1(path.steps.at(-1)?.metric)}</small>
        </>
      ) : (
        <span>No lane loaded.</span>
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

function PathChart({ paths }: { paths: PathPreviewDto[] }) {
  const values = paths.flatMap((path) => path.steps.map((step) => step.metric).filter((metric): metric is number => metric !== null));
  const min = Math.min(...values, 0);
  const max = Math.max(...values, 1);
  return (
    <div className="path-chart">
      {paths.map((path) => (
        <div className="spark-line" key={path.title}>
          {path.steps.map((step) => (
            <span
              key={`${path.title}-${step.level}`}
              style={{ height: `${metricHeight(step.metric, min, max)}%` }}
              title={`${path.title} ${step.level}: ${fixed1(step.metric)}`}
            />
          ))}
        </div>
      ))}
    </div>
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
