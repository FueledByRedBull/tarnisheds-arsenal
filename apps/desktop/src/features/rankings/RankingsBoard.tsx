import { Download, LockKeyhole, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { downloadCsv, rankingsCsvFilename, rankingsToCsv } from "../../lib/csv";
import { cachedWeaponScalingForUpgrade } from "../../lib/analysis-cache";
import { compactNumber, fixed1, metricForObjective, objectiveLabel } from "../../lib/format";
import { buildOptimizeRequest, derivedLevel, rowFingerprint, scalingLetter } from "../../lib/session";
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
  const [isExporting, setExporting] = useState(false);
  const [scalingByRow, setScalingByRow] = useState<Record<string, ScalingDto>>({});
  const constraintCount = [
    request.weaponTypeKey,
    request.weaponName,
    request.affinity,
    request.aowName,
    request.somberFilter !== "all" ? request.somberFilter : null,
    lockedStatMode ? "locked-stats" : null,
    request.minStr > 0 ? "min-str" : null,
    request.minDex > 0 ? "min-dex" : null,
    request.minInt > 0 ? "min-int" : null,
    request.minFai > 0 ? "min-fai" : null,
    request.minArc > 0 ? "min-arc" : null,
  ].filter(Boolean).length;

  useEffect(() => {
    const controller = new AbortController();
    async function loadScaling() {
      const pairs = await Promise.all(
        rows.map(async (row) => [
          rowFingerprint(row),
          await cachedWeaponScalingForUpgrade(
            row.weaponName,
            row.affinity,
            row.upgrade,
            controller.signal,
          ),
        ] as const),
      );
      if (!controller.signal.aborted) {
        setScalingByRow(Object.fromEntries(pairs));
      }
    }
    loadScaling().catch((error) => {
      if (!controller.signal.aborted) {
        setError(error instanceof Error ? error.message : String(error));
      }
    });
    return () => {
      controller.abort();
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
        </div>
      </div>
      <div className="query-summary" aria-label="Active search summary">
        <span className="query-summary-title"><Sparkles size={14} />Active query</span>
        <span>{objectiveLabel(request.objective)}</span>
        <span>Level {derivedLevel(catalog, request)}</span>
        <span>
          {request.exactUpgrade
            ? `Exact +${request.standardMaxUpgrade} / +${request.somberMaxUpgrade}`
            : `Up to +${request.standardMaxUpgrade} / +${request.somberMaxUpgrade}`}
        </span>
        <span>{request.twoHanding ? "Two-handed" : "One-handed"}</span>
        <span>{request.dlcScaling ? `DLC blessing +${request.scadutreeLevel}` : "Base-game scaling"}</span>
        <span>{constraintCount} active constraint{constraintCount === 1 ? "" : "s"}</span>
        <small>{catalog?.dataManifest.label ?? "Loading dataset"}</small>
      </div>
      <div className="top-cards">
        {[0, 1, 2].map((idx) => (
          <TopCard
            key={idx}
            row={rows[idx] ?? null}
            index={idx}
            active={rowFingerprint(rows[idx] ?? null) === rowFingerprint(selected)}
            objective={objective}
            onSelect={() => rows[idx] && selectRow(rows[idx])}
            onLock={() => rows[idx] && lockAndRerun(rows[idx])}
          />
        ))}
      </div>
      <div
        className="result-board full-grid"
        role="grid"
        aria-label="Ranked builds"
      >
        <div className="result-head result-head-full" role="row">
          {[
            ["#", "Rank"],
            ["Weapon", "Weapon and reinforcement type"],
            ["Setup", "Affinity and Ash of War"],
            ["Upg", "Reinforcement level"],
            ["Scaling", "Attribute scaling at this reinforcement level"],
            ["AR / Status", "Raw attack rating and status buildup"],
            ["Raw skill", "Raw skill damage before enemy defense or negation"],
            ["Score", "Value used by the active ranking objective"],
            ["Lock", "Use this result as exact search locks"],
          ].map(([header, title]) => (
            <span role="columnheader" title={title} key={header}>{header}</span>
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

function TopCard({
  row,
  index,
  active,
  objective,
  onSelect,
  onLock,
}: {
  row: SolvedBuildDto | null;
  index: number;
  active: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  onSelect: () => void;
  onLock: () => void;
}) {
  return (
    <div className={`top-card ${active ? "active" : ""}`}>
      {row ? (
        <>
          <button
            className="top-card-select"
            type="button"
            onClick={onSelect}
            aria-label={`Select ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
          >
            <span>#{index + 1}</span>
            <strong>{row.weaponName}</strong>
            <small>{row.affinity} · {row.aowName ?? "Native"} · +{row.upgrade}</small>
            <b>{fixed1(metricForObjective(row, objective))}</b>
          </button>
          <button
            className="top-card-lock"
            type="button"
            aria-label={`Lock ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
            onClick={onLock}
          >
            <LockKeyhole size={14} />Lock
          </button>
        </>
      ) : (
        <>
          <span>#{index + 1}</span>
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
      aria-label={`Select ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
      title="Select this build"
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
      <span className="setup-cell"><strong>{row.affinity}</strong><small>{row.aowName ?? "Native"}</small></span>
      <span>+{row.upgrade}</span>
      <span>{formatScaling(scaling)}</span>
      <span className="result-metric-cell"><strong>AR {compactNumber(row.ar.total)}</strong><small>B {compactNumber(row.bleedBuildup)} · F {compactNumber(row.frostBuildup)}</small></span>
      <span className="result-metric-cell"><strong>{compactNumber(row.aowFullSequenceDamage)}</strong><small>First {compactNumber(row.aowFirstHitDamage)}</small></span>
      <span>{fixed1(metricForObjective(row, objective))}</span>
      <span>
        <button
          className="inline-lock"
          type="button"
          aria-label={`Lock ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
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
      <span>Press Search to rank every legal setup under the active query.</span>
      <small>Open loadout fields keep all compatible options eligible.</small>
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
