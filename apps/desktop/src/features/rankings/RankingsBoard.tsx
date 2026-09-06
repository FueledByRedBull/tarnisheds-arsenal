import { ArrowDownUp, ChevronLeft, ChevronRight, Download, LockKeyhole, Pin, RefreshCcw, Sparkles } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { downloadCsv, rankingsCsvFilename, rankingsToCsv } from "../../lib/csv";
import { compactNumber, fixed1, metricForObjective, objectiveLabel, statLine } from "../../lib/format";
import { buildOptimizeRequest, derivedLevel, rowFingerprint } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { SearchProgressDto, SolvedBuildDto } from "../../lib/types";
import { runSearchFromStore, runSearchRequestForRows } from "../../lib/workflows";
import packageInfo from "../../../package.json";
import { ScalingTokens } from "../shared/BuildMetricTokens";

export function RankingsBoard() {
  const rows = useDesktopStore((state) => state.rows);
  const selected = useDesktopStore((state) => state.selected);
  const selectRow = useDesktopStore((state) => state.selectRow);
  const useRowAsLocks = useDesktopStore((state) => state.useRowAsLocks);
  const compareBench = useDesktopStore((state) => state.compareBench);
  const toggleCompareBench = useDesktopStore((state) => state.toggleCompareBench);
  const catalog = useDesktopStore((state) => state.catalog);
  const request = useDesktopStore((state) => state.request);
  const patchRequest = useDesktopStore((state) => state.patchRequest);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  const objective = useDesktopStore((state) => state.request.objective);
  const isExporting = useDesktopStore((state) => state.isExporting);
  const setExporting = useDesktopStore((state) => state.setExporting);
  const exportController = useRef<AbortController | null>(null);
  const [exportProgress, setExportProgress] = useState<SearchProgressDto | null>(null);
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
  ].filter(Boolean).length + request.filters.entries.length;
  const profileRules = catalog?.dataManifest.rules;
  const separateUpgradeCaps = profileRules?.separateUpgradeCaps ?? true;
  const scadutreeAvailable = profileRules?.scadutreeScaling ?? true;
  const extendedScalingGrades = profileRules?.extendedScalingGrades ?? false;

  useEffect(() => () => exportController.current?.abort(), [catalog, request, lockedStatMode]);

  useEffect(() => {
    const board = resultBoard.current;
    if (!board) return;
    const update = () => {
      const max = Math.max(0, board.scrollWidth - board.clientWidth);
      const next = {
        overflow: max > 1,
        left: board.scrollLeft > 1,
        right: board.scrollLeft < max - 1,
      };
      setHorizontalScroll((previous) => previous.overflow === next.overflow
        && previous.left === next.left && previous.right === next.right ? previous : next);
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
    if (useDesktopStore.getState().isSearching || useDesktopStore.getState().isExporting) return;
    const controller = new AbortController();
    exportController.current = controller;
    setExporting(true);
    setExportProgress(null);
    setError(null);
    try {
      const requestedRows = exportLimit;
      const exportRequest = {
        ...buildOptimizeRequest(catalog, request, lockedStatMode),
        topK: requestedRows,
      };
      const signature = JSON.stringify([catalog?.dataManifest, exportRequest]);
      let exportRows: SolvedBuildDto[];
      if (!resultsStale && requestedRows <= rows.length) {
        exportRows = rows.slice(0, requestedRows);
      } else if (exportCache.current?.signature === signature) {
        exportRows = exportCache.current.rows;
      } else {
        exportRows = await runSearchRequestForRows(exportRequest, controller.signal, setExportProgress);
        if (controller.signal.aborted) return;
        exportCache.current = { signature, rows: exportRows };
      }
      if (controller.signal.aborted) return;
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
      if (!controller.signal.aborted) setError(error instanceof Error ? error.message : String(error));
    } finally {
      if (exportController.current === controller) setExporting(false);
    }
  }

  async function runStarterExample() {
    patchRequest({
      weaponTypeKey: null,
      weaponName: "Uchigatana",
      affinity: "Standard",
      aowName: null,
      filters: { version: 1, entries: [] },
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
          <h1>Rankings</h1>
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
            onClick={() => isExporting ? exportController.current?.abort() : void exportCsv()}
            disabled={isSearching}
          >
            <Download size={16} />
            <span>{isExporting ? "Cancel export" : "Export CSV"}</span>
          </button>
        </div>
      </div>
      {isExporting ? <div className="estimate-strip" role="status">
        <span>Exporting CSV</span>
        <strong>{exportProgress ? `${exportProgress.checked.toLocaleString()} / ${exportProgress.total.toLocaleString()}` : "Preparing search..."}</strong>
      </div> : null}
      {resultsStale && rows.length > 0 ? (
        <div className="stale-results-banner" id="stale-results-message" role="status">
          <div>
            <strong>Inputs changed</strong>
            <span>These rankings are retained from the previous query until the updated search finishes.</span>
          </div>
          <button type="button" onClick={() => void runSearchFromStore()} disabled={isSearching || isExporting}>
            <RefreshCcw size={14} />{isSearching ? "Updating..." : "Run updated search"}
          </button>
        </div>
      ) : null}
      <div className="query-summary" aria-label="Active search summary">
        <span className="query-summary-title"><Sparkles size={14} />{resultsStale ? "Pending query" : "Active query"}</span>
        <span>{objectiveLabel(request.objective)}</span>
        <span>{catalog?.dataManifest.capabilities.classBudget === false ? "Stat total" : "Level"} {derivedLevel(catalog, request)}</span>
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
      <div
        ref={resultBoard}
        className="result-board full-grid"
        role="grid"
        aria-label={resultsStale ? "Ranked builds from the previous query" : "Ranked builds"}
        aria-describedby={resultsStale ? "stale-results-message" : undefined}
      >
        <div className={`result-head result-head-full ${objective !== "max_ar" ? "with-score" : ""}`} role="row">
          {[
            ["#", "Rank"],
            ["Weapon", "Weapon and reinforcement type"],
            ["Setup", "Affinity and Ash of War"],
            ["Upg", "Reinforcement level"],
            ["AR", "Raw attack rating before enemy defense and negation"],
            ["Raw skill", "Raw skill damage before enemy defense or negation"],
            ...(objective !== "max_ar" ? [[`${objectiveLabel(objective)} score`, "Value used by the active ranking objective"]] : []),
            ["Actions", "Pin for comparison or use this result as exact search locks"],
          ].map(([header, title]) => (
            <span role="columnheader" title={title} key={header}>{header}</span>
          ))}
        </div>
        {rows.length === 0 ? <EmptyRows onExample={runStarterExample} busy={isSearching || isExporting} classBudget={catalog?.dataManifest.capabilities.classBudget !== false} /> : null}
        {rankedRows.map(({ row, rank }) => (
          <ResultRow
            key={`${rowFingerprint(row)}-${rank}`}
            index={rank}
            row={row}
            active={rowFingerprint(selected) === rowFingerprint(row)}
            objective={objective}
            lockDisabled={isExporting}
            aowModelSupported={Boolean(catalog?.dataManifest.capabilities.aowDamage && catalog.dataManifest.capabilities.aowRoutes)}
            extendedScalingGrades={extendedScalingGrades}
            onClick={() => selectRow(row)}
            onLock={() => lockAndRerun(row)}
            pinned={compareBench.some((entry) => rowFingerprint(entry) === rowFingerprint(row))}
            onPin={() => toggleCompareBench(row)}
          />
        ))}
      </div>
    </section>
  );
}

function ResultRow({
  row,
  index,
  active,
  objective,
  lockDisabled,
  aowModelSupported,
  extendedScalingGrades,
  onClick,
  onLock,
  pinned,
  onPin,
}: {
  row: SolvedBuildDto;
  index: number;
  active: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  aowModelSupported: boolean;
  extendedScalingGrades: boolean;
  onClick: () => void;
  onLock: () => void;
  lockDisabled: boolean;
  pinned: boolean;
  onPin: () => void;
}) {
  return (
    <div
      className={`result-row result-row-full ${objective !== "max_ar" ? "with-score" : ""} ${active ? "active" : ""}`}
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
      <span role="gridcell" className="weapon-cell">
        <strong>{row.weaponName}</strong>
        <small>{row.isSomber ? "Somber" : "Standard"}</small>
        <span className="row-detail-label">Combat stats</span>
        <span className="row-combat-stats">{statLine(row)}</span>
      </span>
      <span role="gridcell" className="setup-cell">
        <strong>{row.affinity}</strong>
        <small>{row.aowName ?? "Unspecified skill"}</small>
        <span className="row-detail-label">Weapon scaling</span>
        <ScalingTokens scaling={row.effectiveScaling} extended={extendedScalingGrades} />
      </span>
      <span role="gridcell">+{row.upgrade}</span>
      <span role="gridcell" className="result-metric-cell ar-status-cell"><strong>{compactNumber(row.ar.total)}</strong></span>
      <span role="gridcell" className="result-metric-cell" title={aowModelSupported ? undefined : "Raw skill damage is unavailable for this profile."}>
        {aowModelSupported
          ? <><strong>{compactNumber(row.aowFullSequenceDamage)}</strong><small>First {compactNumber(row.aowFirstHitDamage)}</small></>
          : <strong>Unavailable</strong>}
      </span>
      {objective !== "max_ar" ? <span role="gridcell" className="objective-score">{fixed1(metricForObjective(row, objective))}</span> : null}
      <span role="gridcell">
        <button
          className="inline-lock"
          type="button"
          aria-pressed={pinned}
          aria-label={`${pinned ? "Unpin" : "Compare"} ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
          onClick={(event) => {
            event.stopPropagation();
            onPin();
          }}
        >
          <Pin size={15} aria-hidden="true" />
        </button>
        <button
          className="inline-lock"
          type="button"
          aria-label={`Lock ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
          disabled={lockDisabled}
          onClick={(event) => {
            event.stopPropagation();
            onLock();
          }}
        >
          <LockKeyhole size={15} aria-hidden="true" />
        </button>
      </span>
    </div>
  );
}

function EmptyRows({ onExample, busy, classBudget }: { onExample: () => void; busy: boolean; classBudget: boolean }) {
  return (
    <div className="empty-state">
      <strong>No rankings loaded</strong>
      <span>Press Search to rank every legal setup under the active query.</span>
      <small>Open loadout fields keep all compatible options eligible.</small>
      {classBudget ? (
        <button className="inline-lock" type="button" onClick={onExample} disabled={busy}>{busy ? "Searching…" : "Try Uchigatana +3 example"}</button>
      ) : <small>Convergence uses your entered combat stats exactly.</small>}
    </div>
  );
}
