import { ArrowLeft, ArrowRight, Crosshair, Download, LockKeyhole } from "lucide-react";
import { MutableRefObject, PointerEvent, WheelEvent, useEffect, useRef, useState } from "react";
import { downloadCsv, rankingsCsvFilename, rankingsToCsv } from "../../lib/csv";
import { cachedWeaponScalingForUpgrade } from "../../lib/analysis-cache";
import { compactNumber, fixed1, metricForObjective, statLine } from "../../lib/format";
import { buildOptimizeRequest, rowFingerprint, scalingLetter } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { ScalingDto, SolvedBuildDto } from "../../lib/types";
import { runSearchFromStore, runSearchRequestForRows } from "../../lib/workflows";

export function RankingsBoard() {
  const rows = useDesktopStore((state) => state.rows);
  const selected = useDesktopStore((state) => state.selected);
  const selectRow = useDesktopStore((state) => state.selectRow);
  const useRowAsLocks = useDesktopStore((state) => state.useRowAsLocks);
  const catalog = useDesktopStore((state) => state.catalog);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const objective = useDesktopStore((state) => state.request.objective);
  const boardRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef({
    active: false,
    dragged: false,
    pointerId: -1,
    scrollLeft: 0,
    startX: 0,
  });
  const [isExporting, setExporting] = useState(false);
  const [scalingByRow, setScalingByRow] = useState<Record<string, ScalingDto>>({});

  useEffect(() => {
    let cancelled = false;
    async function loadScaling() {
      const pairs = await Promise.all(
        rows.map(async (row) => [
          rowFingerprint(row),
          await cachedWeaponScalingForUpgrade(row.weaponName, row.affinity, row.upgrade),
        ] as const),
      );
      if (!cancelled) {
        setScalingByRow(Object.fromEntries(pairs));
      }
    }
    loadScaling().catch((error) => setError(error instanceof Error ? error.message : String(error)));
    return () => {
      cancelled = true;
    };
  }, [rows, setError]);

  async function lockAndRerun(row: SolvedBuildDto) {
    useRowAsLocks(row);
    await runSearchFromStore();
  }

  async function exportCsv() {
    setExporting(true);
    setError(null);
    try {
      const exportRequest = {
        ...buildOptimizeRequest(catalog, request, lockedStatMode),
        topK: 500,
      };
      const exportRows = await runSearchRequestForRows(exportRequest);
      downloadCsv(rankingsCsvFilename(), rankingsToCsv(exportRows));
      pushNotice({
        scope: "rankings",
        tone: "success",
        message: `Exported ${exportRows.length} ranked rows to CSV.`,
      });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setExporting(false);
    }
  }

  return (
    <section className="workspace-panel rankings-panel">
      <div className="workspace-header">
        <div>
          <h1>Build Board</h1>
          <span>{rows.length} ranked rows</span>
        </div>
        <div className="result-scroll-actions">
          <button
            className="export-csv-button"
            type="button"
            title="Export top 500 rows to CSV"
            onClick={exportCsv}
            disabled={isSearching || isExporting}
          >
            <Download size={16} />
            <span>{isExporting ? "Exporting..." : "Export CSV"}</span>
          </button>
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
            scaling={rows[idx] ? scalingByRow[rowKey(rows[idx])] ?? null : null}
            onFocus={() => rows[idx] && selectRow(rows[idx])}
            onLock={() => rows[idx] && lockAndRerun(rows[idx])}
          />
        ))}
      </div>
      <div
        className="result-board full-grid"
        ref={boardRef}
        onClickCapture={(event) => {
          if (dragRef.current.dragged) {
            event.preventDefault();
            event.stopPropagation();
            dragRef.current.dragged = false;
          }
        }}
        onPointerDown={(event) => startResultBoardDrag(event, dragRef)}
        onPointerCancel={(event) => stopResultBoardDrag(event, dragRef)}
        onPointerLeave={(event) => stopResultBoardDrag(event, dragRef)}
        onPointerMove={(event) => moveResultBoardDrag(event, dragRef)}
        onPointerUp={(event) => stopResultBoardDrag(event, dragRef)}
        onWheel={scrollResultBoardWithWheel}
        role="grid"
        aria-label="Ranked builds"
      >
        <div className="result-head result-head-full" role="row">
          {["#", "Weapon", "Affinity", "AoW", "Upg", "Scaling", "STR", "DEX", "INT", "FAI", "ARC", "Split", "AR", "Bleed", "Frost", "AoW 1st", "AoW Full", "Score", "Lock"].map((header) => (
            <span role="columnheader" key={header}>{header}</span>
          ))}
        </div>
        {rows.length === 0 ? <EmptyRows /> : null}
        {rows.map((row, index) => (
          <ResultRow
            key={`${rowFingerprint(row)}-${index}`}
            index={index}
            row={row}
            active={rowFingerprint(selected) === rowFingerprint(row)}
            objective={objective}
            scaling={scalingByRow[rowKey(row)] ?? null}
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

function startResultBoardDrag(
  event: PointerEvent<HTMLDivElement>,
  dragRef: MutableRefObject<{
    active: boolean;
    dragged: boolean;
    pointerId: number;
    scrollLeft: number;
    startX: number;
  }>,
) {
  if (event.button !== 0 || isInteractiveDragTarget(event.target)) {
    return;
  }
  dragRef.current = {
    active: true,
    dragged: false,
    pointerId: event.pointerId,
    scrollLeft: event.currentTarget.scrollLeft,
    startX: event.clientX,
  };
}

function moveResultBoardDrag(
  event: PointerEvent<HTMLDivElement>,
  dragRef: MutableRefObject<{
    active: boolean;
    dragged: boolean;
    pointerId: number;
    scrollLeft: number;
    startX: number;
  }>,
) {
  const drag = dragRef.current;
  if (!drag.active || drag.pointerId !== event.pointerId) {
    return;
  }
  const deltaX = event.clientX - drag.startX;
  if (Math.abs(deltaX) > 3) {
    if (!drag.dragged) {
      drag.dragged = true;
      event.currentTarget.setPointerCapture(event.pointerId);
      event.currentTarget.classList.add("dragging");
    }
    event.preventDefault();
    event.currentTarget.scrollLeft = drag.scrollLeft - deltaX;
  }
}

function stopResultBoardDrag(
  event: PointerEvent<HTMLDivElement>,
  dragRef: MutableRefObject<{
    active: boolean;
    dragged: boolean;
    pointerId: number;
    scrollLeft: number;
    startX: number;
  }>,
) {
  const drag = dragRef.current;
  if (!drag.active || drag.pointerId !== event.pointerId) {
    return;
  }
  if (event.currentTarget.hasPointerCapture(event.pointerId)) {
    event.currentTarget.releasePointerCapture(event.pointerId);
  }
  drag.active = false;
  event.currentTarget.classList.remove("dragging");
}

function isInteractiveDragTarget(target: EventTarget): boolean {
  return target instanceof Element && Boolean(target.closest("button, input, select, textarea, a"));
}

function TopCard({
  row,
  index,
  active,
  objective,
  scaling,
  onFocus,
  onLock,
}: {
  row: SolvedBuildDto | null;
  index: number;
  active: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  scaling: ScalingDto | null;
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
          <small>{formatScaling(scaling)}</small>
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
  scaling,
  onClick,
  onLock,
}: {
  row: SolvedBuildDto;
  index: number;
  active: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  scaling: ScalingDto | null;
  onClick: () => void;
  onLock: () => void;
}) {
  return (
    <div
      className={`result-row result-row-full ${active ? "active" : ""}`}
      role="row"
      aria-selected={active}
      aria-label={`Focus ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
      title="Click to focus this result"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) {
          return;
        }
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onClick();
        }
      }}
    >
      <span className="rank-cell">{index + 1}</span>
      <span className="weapon-cell"><strong>{row.weaponName}</strong><small>{row.isSomber ? "Somber" : "Standard"}</small></span>
      <span>{row.affinity}</span>
      <span>{row.aowName ?? "Native"}</span>
      <span>+{row.upgrade}</span>
      <span>{formatScaling(scaling)}</span>
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

function formatScaling(scaling: ScalingDto | null): string {
  if (!scaling) {
    return "-";
  }
  return `STR ${scalingLetter(scaling.str)} DEX ${scalingLetter(scaling.dex)} INT ${scalingLetter(scaling.int)} FAI ${scalingLetter(scaling.fai)} ARC ${scalingLetter(scaling.arc)}`;
}

function rowKey(row: SolvedBuildDto): string {
  return rowFingerprint(row) ?? `${row.weaponId}-${row.weaponName}-${row.affinity}-${row.upgrade}`;
}
