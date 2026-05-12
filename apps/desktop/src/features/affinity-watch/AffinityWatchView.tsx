import { Pause, Play } from "lucide-react";
import { useEffect, useMemo } from "react";
import { api, hasTauriRuntime } from "../../lib/api";
import { fixed1, statLine } from "../../lib/format";
import { buildOptimizeRequest, clampHorizon, stableSignature } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { AffinityWatchPayloadDto } from "../../lib/types";

export function AffinityWatchView() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const horizon = useDesktopStore((state) => state.horizon);
  const setHorizon = useDesktopStore((state) => state.setHorizon);
  const payload = useDesktopStore((state) => state.affinityPayload);
  const setAffinityPayload = useDesktopStore((state) => state.setAffinityPayload);
  const isAffinityBusy = useDesktopStore((state) => state.isAffinityBusy);
  const setAffinityBusy = useDesktopStore((state) => state.setAffinityBusy);
  const activeAffinityJobId = useDesktopStore((state) => state.activeAffinityJobId);
  const setActiveAffinityJobId = useDesktopStore((state) => state.setActiveAffinityJobId);
  const affinityProgress = useDesktopStore((state) => state.affinityProgress);
  const setAffinityProgress = useDesktopStore((state) => state.setAffinityProgress);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const base = useMemo(
    () => buildOptimizeRequest(catalog, request, lockedStatMode),
    [catalog, lockedStatMode, request],
  );
  const effectiveHorizon = clampHorizon(request, horizon);
  const signature = stableSignature({ selected, objective: request.objective, level: base.characterLevel, horizon: effectiveHorizon });

  useEffect(() => {
    let unlistenProgress: (() => void) | undefined;
    let unlistenFinished: (() => void) | undefined;
    api.onAffinityWatchProgress((event) => {
      if (!activeAffinityJobId || event.jobId === activeAffinityJobId) setAffinityProgress(event);
    }).then((unlisten) => {
      unlistenProgress = unlisten;
    });
    api.onAffinityWatchFinished((event) => {
      if (activeAffinityJobId && event.jobId !== activeAffinityJobId) return;
      if (event.error) setError(event.error);
      if (event.cancelled) pushNotice({ scope: "affinity_watch", tone: "warning", message: "Affinity watch stopped." });
      else setAffinityPayload(event.payload, signature);
      setAffinityBusy(false);
      setActiveAffinityJobId(null);
      setAffinityProgress(null);
    }).then((unlisten) => {
      unlistenFinished = unlisten;
    });
    return () => {
      unlistenProgress?.();
      unlistenFinished?.();
    };
  }, [activeAffinityJobId, pushNotice, setActiveAffinityJobId, setAffinityBusy, setAffinityPayload, setAffinityProgress, setError, signature]);

  async function refresh() {
    if (!selected) {
      pushNotice({ scope: "affinity_watch", tone: "warning", message: "Pick a selected result first." });
      return;
    }
    if (effectiveHorizon <= 0) {
      pushNotice({ scope: "affinity_watch", tone: "warning", message: "Combat stats are already capped. There is no forward horizon to inspect." });
      return;
    }
    setAffinityBusy(true);
    setAffinityProgress(null);
    try {
      const legal = await api.affinitiesForWeapon(selected.weaponName);
      if (legal.length === 0) {
        pushNotice({ scope: "affinity_watch", tone: "warning", message: "No legal affinities are available for the selected weapon." });
        setAffinityBusy(false);
        return;
      }
      if (hasTauriRuntime()) {
        const { jobId } = await api.startAffinityWatch(base, selected, effectiveHorizon);
        setActiveAffinityJobId(jobId);
      } else {
        const next = await api.buildAffinityWatch(base, selected, effectiveHorizon);
        setAffinityPayload(next, signature);
        setAffinityBusy(false);
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
      setAffinityBusy(false);
    }
  }

  async function stop() {
    if (activeAffinityJobId) await api.cancelAffinityWatch(activeAffinityJobId);
  }

  return (
    <section className="workspace-panel affinity-panel">
      <div className="workspace-header">
        <div>
          <h1>Affinity Watch</h1>
          <span>{selected ? `${selected.weaponName} across Current +${effectiveHorizon}` : "Requires selected result"}</span>
        </div>
        <div className="header-controls">
          <label>
            Current + N
            <input type="number" min={1} max={200} value={horizon} onChange={(event) => setHorizon(clamp(Number(event.target.value), 1, 200))} />
          </label>
          <button type="button" onClick={isAffinityBusy ? stop : refresh}>
            {isAffinityBusy ? <Pause size={15} /> : <Play size={15} />}
            {isAffinityBusy ? "Stop" : "Refresh"}
          </button>
        </div>
      </div>
      <Progress
        checked={affinityProgress?.checked ?? 0}
        total={affinityProgress?.total ?? (payload?.lines.length || 1)}
        busy={isAffinityBusy}
        label={affinityProgress ? `${affinityProgress.affinity} Lv ${affinityProgress.level}` : "Idle"}
      />
      <AffinityChart payload={payload} />
      <div className="affinity-ranking">
        {payload?.lines.map((line, index) => (
          <div className="affinity-row" key={line.affinity}>
            <b>{index + 1}</b>
            <strong>{line.affinity}</strong>
            <span>{fixed1(line.startMetric)}</span>
            <span>{fixed1(line.endMetric)}</span>
            <small>{line.finalBuild ? statLine(line.finalBuild) : "-"}</small>
          </div>
        ))}
      </div>
      <div className="crossover-table">
        {payload?.breakpoints.map((point) => (
          <div key={`${point.level}-${point.incomingAffinity}`}>
            <span>Level {point.level}</span>
            <strong>{point.outgoingAffinity} to {point.incomingAffinity}</strong>
            <span>{fixed1(point.outgoingMetric)} / {fixed1(point.incomingMetric)}</span>
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

function AffinityChart({ payload }: { payload: AffinityWatchPayloadDto | null }) {
  const values = payload?.lines.flatMap((line) => line.points.map((point) => point.metric).filter((metric): metric is number => metric !== null)) ?? [];
  const min = Math.min(...values, 0);
  const max = Math.max(...values, 1);
  return (
    <div className="path-chart affinity-chart">
      {payload?.lines.map((line) => (
        <div className="spark-line" key={line.affinity}>
          {line.points.map((point) => (
            <span
              key={`${line.affinity}-${point.level}`}
              style={{ height: `${metricHeight(point.metric, min, max)}%` }}
              title={`${line.affinity} ${point.level}: ${fixed1(point.metric)}`}
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
