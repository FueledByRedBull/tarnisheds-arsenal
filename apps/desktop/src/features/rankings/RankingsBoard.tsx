import { ArrowLeft, ArrowRight, Crosshair, LockKeyhole } from "lucide-react";
import { WheelEvent, useRef } from "react";
import { compactNumber, fixed1, metricForObjective, statLine } from "../../lib/format";
import { rowFingerprint } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { SolvedBuildDto } from "../../lib/types";
import { runSearchFromStore } from "../../lib/workflows";

export function RankingsBoard() {
  const rows = useDesktopStore((state) => state.rows);
  const selected = useDesktopStore((state) => state.selected);
  const selectRow = useDesktopStore((state) => state.selectRow);
  const useRowAsLocks = useDesktopStore((state) => state.useRowAsLocks);
  const objective = useDesktopStore((state) => state.request.objective);
  const boardRef = useRef<HTMLDivElement | null>(null);

  async function lockAndRerun(row: SolvedBuildDto) {
    useRowAsLocks(row);
    await runSearchFromStore();
  }

  return (
    <section className="workspace-panel rankings-panel">
      <div className="workspace-header">
        <div>
          <h1>Build Board</h1>
          <span>{rows.length} ranked rows</span>
        </div>
        <div className="result-scroll-actions">
          <button type="button" title="Scroll table left" onClick={() => scrollResultBoard(boardRef.current, -1)}>
            <ArrowLeft size={16} />
          </button>
          <button type="button" title="Scroll table right" onClick={() => scrollResultBoard(boardRef.current, 1)}>
            <ArrowRight size={16} />
          </button>
        </div>
      </div>
      <div className="top-cards">
        {[0, 1, 2].map((idx) => (
          <TopCard
            key={idx}
            row={rows[idx] ?? null}
            index={idx}
            active={rowFingerprint(rows[idx] ?? null) === rowFingerprint(selected)}
            objective={objective}
            onFocus={() => rows[idx] && selectRow(rows[idx])}
            onLock={() => rows[idx] && lockAndRerun(rows[idx])}
          />
        ))}
      </div>
      <div className="result-board full-grid" ref={boardRef} onWheel={scrollResultBoardWithWheel}>
        <div className="result-head result-head-full">
          <span>#</span>
          <span>Weapon</span>
          <span>Affinity</span>
          <span>AoW</span>
          <span>Upg</span>
          <span>STR</span>
          <span>DEX</span>
          <span>INT</span>
          <span>FAI</span>
          <span>ARC</span>
          <span>Split</span>
          <span>AR</span>
          <span>Bleed</span>
          <span>Frost</span>
          <span>AoW 1st</span>
          <span>AoW Full</span>
          <span>Score</span>
          <span>Lock</span>
        </div>
        {rows.length === 0 ? <EmptyRows /> : null}
        {rows.map((row, index) => (
          <ResultRow
            key={`${rowFingerprint(row)}-${index}`}
            index={index}
            row={row}
            active={rowFingerprint(selected) === rowFingerprint(row)}
            objective={objective}
            onClick={() => selectRow(row)}
            onLock={() => lockAndRerun(row)}
          />
        ))}
      </div>
    </section>
  );
}

function scrollResultBoard(element: HTMLDivElement | null, direction: -1 | 1) {
  if (!element) {
    return;
  }
  element.scrollBy({
    left: direction * Math.max(320, element.clientWidth * 0.8),
    behavior: "smooth",
  });
}

function scrollResultBoardWithWheel(event: WheelEvent<HTMLDivElement>) {
  if (!event.shiftKey && Math.abs(event.deltaX) <= Math.abs(event.deltaY)) {
    return;
  }
  event.currentTarget.scrollLeft += event.deltaX || event.deltaY;
}

function TopCard({
  row,
  index,
  active,
  objective,
  onFocus,
  onLock,
}: {
  row: SolvedBuildDto | null;
  index: number;
  active: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  onFocus: () => void;
  onLock: () => void;
}) {
  return (
    <div className={`top-card ${active ? "active" : ""}`}>
      <span>#{index + 1}</span>
      {row ? (
        <>
          <strong>{row.weaponName}</strong>
          <small>{row.affinity} / {row.aowName ?? "Native"} / +{row.upgrade}</small>
          <b>{fixed1(metricForObjective(row, objective))}</b>
          <small>{statLine(row)}</small>
          <div className="card-actions">
            <button type="button" onClick={onFocus}><Crosshair size={14} />Focus</button>
            <button type="button" onClick={onLock}><LockKeyhole size={14} />Lock</button>
          </div>
        </>
      ) : (
        <>
          <strong>No result yet</strong>
          <small>Run a search to fill this slot.</small>
        </>
      )}
    </div>
  );
}

function ResultRow({
  row,
  index,
  active,
  objective,
  onClick,
  onLock,
}: {
  row: SolvedBuildDto;
  index: number;
  active: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  onClick: () => void;
  onLock: () => void;
}) {
  return (
    <div
      className={`result-row result-row-full ${active ? "active" : ""}`}
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") onClick();
      }}
    >
      <span className="rank-cell">{index + 1}</span>
      <span className="weapon-cell"><strong>{row.weaponName}</strong><small>{row.isSomber ? "Somber" : "Standard"}</small></span>
      <span>{row.affinity}</span>
      <span>{row.aowName ?? "Native"}</span>
      <span>+{row.upgrade}</span>
      <span>{row.stats.strStat}</span>
      <span>{row.stats.dex}</span>
      <span>{row.stats.intStat}</span>
      <span>{row.stats.fai}</span>
      <span>{row.stats.arc}</span>
      <span className="tiny-split">P {compactNumber(row.ar.physical)} M {compactNumber(row.ar.magic)} F {compactNumber(row.ar.fire)} L {compactNumber(row.ar.lightning)} H {compactNumber(row.ar.holy)}</span>
      <span>{compactNumber(row.ar.total)}</span>
      <span>{compactNumber(row.bleedBuildup)}</span>
      <span>{compactNumber(row.frostBuildup)}</span>
      <span>{compactNumber(row.aowFirstHitDamage)}</span>
      <span>{compactNumber(row.aowFullSequenceDamage)}</span>
      <span>{fixed1(metricForObjective(row, objective))}</span>
      <span>
        <button
          className="inline-lock"
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            onLock();
          }}
        >
          Lock
        </button>
      </span>
    </div>
  );
}

function EmptyRows() {
  return (
    <div className="empty-state">
      <strong>No rankings loaded</strong>
      <span>Run a search to populate the board.</span>
    </div>
  );
}
