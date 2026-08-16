import { ArrowDownUp, ChevronLeft, ChevronRight, Download, LockKeyhole, RefreshCcw, Sparkles } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { downloadCsv, rankingsCsvFilename, rankingsToCsv } from "../../lib/csv";
import { compactNumber, fixed1, metricForObjective, objectiveLabel } from "../../lib/format";
import { buildOptimizeRequest, derivedLevel, rowFingerprint } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { ScalingDto, SolvedBuildDto } from "../../lib/types";
import { runSearchFromStore, runSearchRequestForRows } from "../../lib/workflows";
import { DamageTokens, ScalingTokens, StatusTokens } from "../shared/BuildMetricTokens";
import packageInfo from "../../../package.json";

export function RankingsBoard() {
  const rows = useDesktopStore((state) => state.rows);
  const selected = useDesktopStore((state) => state.selected);
  const selectRow = useDesktopStore((state) => state.selectRow);
  const useRowAsLocks = useDesktopStore((state) => state.useRowAsLocks);
  const catalog = useDesktopStore((state) => state.catalog);
  const request = useDesktopStore((state) => state.request);
  const patchRequest = useDesktopStore((state) => state.patchRequest);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const objective = useDesktopStore((state) => state.request.objective);
  const [isExporting, setExporting] = useState(false);
  const [exportLimit, setExportLimit] = useState<25 | 100 | 500 | 2000>(25);
  const exportCache = useRef<{ signature: string; rows: SolvedBuildDto[] } | null>(null);
  const [reverseRank, setReverseRank] = useState(false);
  const [horizontalScroll, setHorizontalScroll] = useState({ overflow: false, left: false, right: false });
  const resultBoard = useRef<HTMLDivElement>(null);
  const rankedRows = useMemo(() => {
    const entries = rows.map((row, rank) => ({ row, rank }));
    return reverseRank ? entries.reverse() : entries;
  }, [reverseRank, rows]);
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
  const profileRules = catalog?.dataManifest.rules;
  const separateUpgradeCaps = profileRules?.separateUpgradeCaps ?? true;
  const scadutreeAvailable = profileRules?.scadutreeScaling ?? true;
  const extendedScalingGrades = profileRules?.extendedScalingGrades ?? false;

  useEffect(() => {
    const board = resultBoard.current;
    if (!board) return;
    const update = () => {
      const max = Math.max(0, board.scrollWidth - board.clientWidth);
      setHorizontalScroll({
        overflow: max > 1,
        left: board.scrollLeft > 1,
        right: board.scrollLeft < max - 1,
      });
    };
    update();
    board.addEventListener("scroll", update, { passive: true });
    const observer = new ResizeObserver(update);
    observer.observe(board);
    return () => {
      board.removeEventListener("scroll", update);
      observer.disconnect();
    };
  }, [rows.length]);

  async function lockAndRerun(row: SolvedBuildDto) {
    useRowAsLocks(row);
    await runSearchFromStore();
  }

  async function exportCsv() {
    setExporting(true);
    setError(null);
    try {
      const requestedRows = exportLimit;
      const exportRequest = {
        ...buildOptimizeRequest(catalog, request, lockedStatMode),
        topK: requestedRows,
      };
      const signature = JSON.stringify(exportRequest);
      let exportRows: SolvedBuildDto[];
      if (!resultsStale && requestedRows <= rows.length) {
        exportRows = rows.slice(0, requestedRows);
      } else if (exportCache.current?.signature === signature) {
        exportRows = exportCache.current.rows;
      } else {
        exportRows = await runSearchRequestForRows(exportRequest);
        exportCache.current = { signature, rows: exportRows };
      }
      if (!catalog) throw new Error("Catalog metadata is unavailable; the export was not created.");
      downloadCsv(rankingsCsvFilename(request.profileId), rankingsToCsv(exportRows, {
        profileId: request.profileId,
        appVersion: packageInfo.version,
        schemaVersion: String(catalog.dataManifest.schemaVersion),
        datasetVersion: catalog.dataManifest.datasetVersion,
        modelVersion: catalog.dataManifest.modelVersion,
        objective: request.objective,
        assumptions: [
          request.twoHanding ? "two-handed" : "one-handed",
          scadutreeAvailable
            ? request.dlcScaling ? `Scadutree ${request.scadutreeLevel}` : "no DLC attack scaling"
            : "Scadutree scaling unavailable for this profile",
          "raw values; enemy defense and negation not applied",
          ...(request.objective === "max_ar_plus_bleed"
            ? ["status resistance growth and proc damage excluded"]
            : []),
          ...(request.objective === "aow_first_hit" || request.objective === "aow_full_sequence"
            ? ["stamina is reported but not optimized; unsupported effects remain warnings"]
            : []),
          "temporary buff stacking not universal",
        ].join("; "),
        separateUpgradeCaps,
        aowModelSupported: catalog.dataManifest.capabilities.aowDamage && catalog.dataManifest.capabilities.aowRoutes,
        extendedScalingGrades,
      }));
      pushNotice({
        scope: "rankings",
        tone: "success",
        message: `Exported ${exportRows.length} ranked rows to your Downloads folder.`,
      });
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setExporting(false);
    }
  }

  async function runStarterExample() {
    patchRequest({
      weaponTypeKey: null,
      weaponName: "Uchigatana",
      affinity: "Standard",
      aowName: null,
      standardMaxUpgrade: 3,
      exactUpgrade: true,
      objective: "max_ar",
    });
    await runSearchFromStore();
  }

  function scrollResults(direction: -1 | 1) {
    resultBoard.current?.scrollBy({
      left: direction * 360,
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
    });
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
            type="button"
            title="Reverse rank display; click again to return to best first"
            aria-pressed={reverseRank}
            onClick={() => setReverseRank((value) => !value)}
          >
            <ArrowDownUp size={15} />
            <span className="sr-only">{reverseRank ? "Show best rank first" : "Show lowest rank first"}</span>
          </button>
          {horizontalScroll.overflow ? (
            <>
              <button type="button" title="Show columns hidden to the left" disabled={!horizontalScroll.left} onClick={() => scrollResults(-1)}>
                <ChevronLeft size={16} /><span className="sr-only">Show columns hidden to the left</span>
              </button>
              <button type="button" title="Show columns hidden to the right" disabled={!horizontalScroll.right} onClick={() => scrollResults(1)}>
                <ChevronRight size={16} /><span className="sr-only">Show columns hidden to the right</span>
              </button>
            </>
          ) : null}
          <div className="export-limit" role="group" aria-label="CSV row count">
            {([25, 100, 500, 2000] as const).map((limit) => (
              <button
                key={limit}
                type="button"
                className={exportLimit === limit ? "active" : ""}
                aria-pressed={exportLimit === limit}
                title={limit === 2000 ? "Export up to the 2,000-row safety limit" : `Export up to ${limit} rows`}
                onClick={() => setExportLimit(limit)}
                disabled={isSearching || isExporting}
              >
                {limit === 2000 ? "Max" : limit}
              </button>
            ))}
          </div>
          <button
            className="export-csv-button"
            type="button"
            title={`Export up to ${exportLimit.toLocaleString()} rows to CSV`}
            onClick={exportCsv}
            disabled={isSearching || isExporting}
          >
            <Download size={16} />
            <span>{isExporting ? "Exporting..." : "Export CSV"}</span>
          </button>
        </div>
      </div>
      {resultsStale && rows.length > 0 ? (
        <div className="stale-results-banner" id="stale-results-message" role="status">
          <div>
            <strong>Inputs changed</strong>
            <span>These rankings are retained from the previous query until the updated search finishes.</span>
          </div>
          <button type="button" onClick={() => void runSearchFromStore()} disabled={isSearching}>
            <RefreshCcw size={14} />{isSearching ? "Updating..." : "Run updated search"}
          </button>
        </div>
      ) : null}
      <div className="query-summary" aria-label="Active search summary">
        <span className="query-summary-title"><Sparkles size={14} />{resultsStale ? "Pending query" : "Active query"}</span>
        <span>{objectiveLabel(request.objective)}</span>
        <span>Level {derivedLevel(catalog, request)}</span>
        <span>
          {request.exactUpgrade ? "Exact" : "Up to"} +{request.standardMaxUpgrade}
          {separateUpgradeCaps ? ` / +${request.somberMaxUpgrade}` : ""}
        </span>
        <span>{request.twoHanding ? "Two-handed" : "One-handed"}</span>
        <span>
          {scadutreeAvailable
            ? request.dlcScaling ? `DLC blessing +${request.scadutreeLevel}` : "Base-game scaling"
            : "No Scadutree scaling"}
        </span>
        <span>{constraintCount} active constraint{constraintCount === 1 ? "" : "s"}</span>
        <small>{catalog?.dataManifest.label ?? "Loading dataset"}</small>
      </div>
      <details className="mechanics-glossary">
        <summary>Metric glossary</summary>
        <dl>
          <div><dt>AR</dt><dd>Raw attack rating before enemy defense and negation.</dd></div>
          <div><dt>Split</dt><dd>AR divided across physical, magic, fire, lightning, and holy damage.</dd></div>
          <div><dt>AoW 1st / Full</dt><dd>First damaging hit or one complete legal Ash of War route.</dd></div>
          <div><dt>Scaling</dt><dd>Attribute contribution grade at the shown reinforcement level.</dd></div>
          <div><dt>Native</dt><dd>The weapon's fixed skill rather than an applied Ash of War.</dd></div>
          <div><dt>Lock</dt><dd>Copies the result's loadout, upgrade, and combat stats into the next exact search.</dd></div>
        </dl>
      </details>
      <div className="top-cards">
        {[0, 1, 2].map((idx) => (
          <TopCard
            key={idx}
            row={rows[idx] ?? null}
            index={idx}
            active={Boolean(rows[idx]) && rowFingerprint(rows[idx]) === rowFingerprint(selected)}
            objective={objective}
            onSelect={() => rows[idx] && selectRow(rows[idx])}
            onLock={() => rows[idx] && lockAndRerun(rows[idx])}
          />
        ))}
      </div>
      <div
        ref={resultBoard}
        className="result-board full-grid"
        role="grid"
        aria-label={resultsStale ? "Ranked builds from the previous query" : "Ranked builds"}
        aria-describedby={resultsStale ? "stale-results-message" : undefined}
      >
        <div className="result-head result-head-full" role="row">
          {[
            ["#", "Rank"],
            ["Weapon", "Weapon and reinforcement type"],
            ["Setup", "Affinity and Ash of War"],
            ["Upg", "Reinforcement level"],
            ["Scaling", "Attribute scaling at this reinforcement level"],
            ["AR / Elements / Status", "Raw attack rating by damage type and status buildup"],
            ["Raw skill", "Raw skill damage before enemy defense or negation"],
            [`${objectiveLabel(objective)} score`, "Value used by the active ranking objective"],
            ["Lock", "Use this result as exact search locks"],
          ].map(([header, title]) => (
            <span role="columnheader" title={title} key={header}>{header}</span>
          ))}
        </div>
        {rows.length === 0 ? <EmptyRows onExample={runStarterExample} busy={isSearching} /> : null}
        {rankedRows.map(({ row, rank }) => (
          <ResultRow
            key={`${rowFingerprint(row)}-${rank}`}
            index={rank}
            row={row}
            active={rowFingerprint(selected) === rowFingerprint(row)}
            objective={objective}
            scaling={row.effectiveScaling ?? null}
            extendedScalingGrades={extendedScalingGrades}
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
        <div className="top-card-empty">
          <span>#{index + 1}</span>
          <span className="top-card-empty-copy">
            <strong>No result yet</strong>
            <small>Run a search to fill this slot.</small>
          </span>
        </div>
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
  extendedScalingGrades,
  onClick,
  onLock,
}: {
  row: SolvedBuildDto;
  index: number;
  active: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  scaling: ScalingDto | null;
  extendedScalingGrades: boolean;
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
      <span role="gridcell" className="rank-cell">{index + 1}</span>
      <span role="gridcell" className="weapon-cell"><strong>{row.weaponName}</strong><small>{row.isSomber ? "Somber" : "Standard"}</small></span>
      <span role="gridcell" className="setup-cell"><strong>{row.affinity}</strong><small>{row.aowName ?? "Native"}</small></span>
      <span role="gridcell">+{row.upgrade}</span>
      <span role="gridcell" className="scaling-cell"><ScalingTokens scaling={scaling} extended={extendedScalingGrades} /></span>
      <span role="gridcell" className="result-metric-cell ar-status-cell"><strong>AR {compactNumber(row.ar.total)}</strong><DamageTokens ar={row.ar} /><StatusTokens row={row} /></span>
      <span role="gridcell" className="result-metric-cell"><strong>{compactNumber(row.aowFullSequenceDamage)}</strong><small>First {compactNumber(row.aowFirstHitDamage)}</small></span>
      <span role="gridcell">{fixed1(metricForObjective(row, objective))}</span>
      <span role="gridcell">
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

function EmptyRows({ onExample, busy }: { onExample: () => void; busy: boolean }) {
  return (
    <div className="empty-state">
      <strong>No rankings loaded</strong>
      <span>Press Search to rank every legal setup under the active query.</span>
      <small>Open loadout fields keep all compatible options eligible.</small>
      <button type="button" onClick={onExample} disabled={busy}>{busy ? "Searching…" : "Try Samurai +3 example"}</button>
    </div>
  );
}
