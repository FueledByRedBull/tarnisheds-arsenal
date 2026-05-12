import { GitCompareArrows, LockKeyhole, Radar, Route, Target } from "lucide-react";
import { compactNumber, fixed1, metricForObjective, statLine } from "../../lib/format";
import { budgetSnapshot } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { runSearchFromStore } from "../../lib/workflows";

export function Inspector() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const compareTarget = useDesktopStore((state) => state.compareTarget);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
  const useRowAsLocks = useDesktopStore((state) => state.useRowAsLocks);
  const snapshot = budgetSnapshot(catalog, request);

  async function lockSelected() {
    if (!selected) return;
    useRowAsLocks(selected);
    await runSearchFromStore();
  }

  return (
    <aside className="inspector">
      <div className="inspector-title">
        <Target size={17} />
        <span>Inspector</span>
      </div>
      <div className="detail-block">
        <span>Stat Budget</span>
        <strong>Level {snapshot.level} / +{snapshot.levelUps} level ups</strong>
        <small>Redistrib {snapshot.redistributable} / Free {snapshot.freePoints} / Total points {snapshot.total}</small>
      </div>
      <div className="detail-block">
        <span>Lock State</span>
        <strong>{lockedStatMode && request.lockStr !== null ? "Exact upgrade and stat locks active" : "Open or partial locks"}</strong>
        <small>
          {request.lockStr === null
            ? "No captured combat stat locks."
            : `STR ${request.lockStr} DEX ${request.lockDex} INT ${request.lockInt} FAI ${request.lockFai} ARC ${request.lockArc}`}
        </small>
      </div>
      {selected ? (
        <>
          <div className="selected-build">
            <strong>{selected.weaponName}</strong>
            <span>{selected.affinity} / {selected.aowName ?? "Native"} / +{selected.upgrade}</span>
          </div>
          <div className="metric-grid">
            <Metric label="Score" value={fixed1(metricForObjective(selected, request.objective))} />
            <Metric label="AR" value={compactNumber(selected.ar.total)} />
            <Metric label="Bleed" value={compactNumber(selected.bleedBuildup)} />
            <Metric label="AoW Seq" value={compactNumber(selected.aowFullSequenceDamage)} />
          </div>
          <div className="detail-block">
            <span>Combat Stats</span>
            <strong>{statLine(selected)}</strong>
          </div>
          <div className="detail-block split">
            <span>AR Split</span>
            <small>
              PHY {compactNumber(selected.ar.physical)} / MAG {compactNumber(selected.ar.magic)} /
              FIRE {compactNumber(selected.ar.fire)} / LIT {compactNumber(selected.ar.lightning)} /
              HOLY {compactNumber(selected.ar.holy)}
            </small>
          </div>
          <div className="inspector-actions stacked">
            <button type="button" onClick={lockSelected}><LockKeyhole size={15} />Use As Locks</button>
            <button type="button" onClick={() => setWorkspace("compare")}><GitCompareArrows size={15} />Open Compare</button>
            <button type="button" disabled={!compareTarget} onClick={() => setWorkspace("paths")}><Route size={15} />Run Paths</button>
            <button type="button" onClick={() => setWorkspace("affinity_watch")}><Radar size={15} />Run Affinity Watch</button>
          </div>
        </>
      ) : (
        <div className="empty-state compact">
          <strong>No build selected</strong>
          <span>Select a ranked row.</span>
        </div>
      )}
    </aside>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-tile">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
