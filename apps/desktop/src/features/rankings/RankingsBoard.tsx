import { ArrowDown, ArrowDownUp, ArrowUp, ChevronLeft, ChevronRight, Download, LockKeyhole, Pin, RefreshCcw } from "lucide-react";
import { memo, ReactNode, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { downloadCsv, rankingsCsvFilename, rankingsToCsv } from "../../lib/csv";
import { compactNumber, fixed1, hasAowDamage, metricForObjective, objectiveLabel } from "../../lib/format";
import { buildOptimizeRequest, rowFingerprint } from "../../lib/session";
import { reducedMotion, settleRanking } from "../../lib/motion";
import { RankedEntry, RankMovement, RankSortKey, rankMovements, sortRanked } from "../../lib/ranking-view";
import { useDesktopStore } from "../../lib/state";
import { ObjectiveId, SearchProgressDto, SolvedBuildDto } from "../../lib/types";
import { runSearchFromStore, runSearchRequestForRows } from "../../lib/workflows";
import packageInfo from "../../../package.json";
import { STAT_KEYS, ScalingTokens, StatTokens } from "../shared/BuildMetricTokens";
import { SkeletonRows } from "../shared/SkeletonRows";

export function RankingsBoard() {
  const rows = useDesktopStore((state) => state.rows);
  const selected = useDesktopStore((state) => state.selected);
  const selectRow = useDesktopStore((state) => state.selectRow);
  const applyRowLocks = useDesktopStore((state) => state.useRowAsLocks);
  const compareBench = useDesktopStore((state) => state.compareBench);
  const toggleCompareBench = useDesktopStore((state) => state.toggleCompareBench);
  const catalog = useDesktopStore((state) => state.catalog);
  const request = useDesktopStore((state) => state.request);
  const patchRequest = useDesktopStore((state) => state.patchRequest);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const searchGeneration = useDesktopStore((state) => state.searchGeneration);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const pushNotice = useDesktopStore((state) => state.pushNotice);
  const setError = useDesktopStore((state) => state.setError);
  // The board stays rendered while another workspace is shown (App.tsx), so it follows that here.
  const shown = useDesktopStore((state) => state.activeWorkspace === "rankings");
  const objective = useDesktopStore((state) => state.request.objective);
  const isExporting = useDesktopStore((state) => state.isExporting);
  const setExporting = useDesktopStore((state) => state.setExporting);
  const exportController = useRef<AbortController | null>(null);
  const [exportProgress, setExportProgress] = useState<SearchProgressDto | null>(null);
  const [exportLimit, setExportLimit] = useState<25 | 100 | 500 | 2000>(25);
  const exportCache = useRef<{ signature: string; rows: SolvedBuildDto[] } | null>(null);
  const rankBaseline = useDesktopStore((state) => state.rankBaseline);
  const [sort, setSort] = useState<{ key: RankSortKey; reverse: boolean }>({ key: "rank", reverse: false });
  const [horizontalScroll, setHorizontalScroll] = useState({ overflow: false, left: false, right: false });
  const resultBoard = useRef<HTMLDivElement>(null);
  const aowSupported = Boolean(catalog?.dataManifest.capabilities.aowDamage && catalog.dataManifest.capabilities.aowRoutes);
  const rankedRows = useMemo(
    () => sortRanked(rows, sort.key, sort.reverse, objective, aowSupported),
    [aowSupported, objective, rows, sort],
  );
  const movements = useMemo(
    () => rankMovements(rows, rankBaseline, objective, aowSupported),
    [aowSupported, objective, rankBaseline, rows],
  );
  const sortBy = (key: RankSortKey) => setSort((current) => ({ key, reverse: current.key === key ? !current.reverse : false }));
  const ariaSort = (key: RankSortKey) => sort.key !== key ? undefined
    : (key === "rank") !== sort.reverse ? "ascending" as const : "descending" as const;
  const profileRules = catalog?.dataManifest.rules;
  const separateUpgradeCaps = profileRules?.separateUpgradeCaps ?? true;
  const scadutreeAvailable = profileRules?.scadutreeScaling ?? true;
  const extendedScalingGrades = profileRules?.extendedScalingGrades ?? false;

  // Leaving Rankings cancels an export, like any input or profile change.
  useEffect(() => () => exportController.current?.abort(), [catalog, request, lockedStatMode, searchGeneration, shown]);

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
    // The observer's first callback measures after layout. Measuring here would force a full
    // layout of the board inside the click that reveals it.
    board.addEventListener("scroll", update, { passive: true });
    const observer = new ResizeObserver(update);
    observer.observe(board);
    return () => {
      board.removeEventListener("scroll", update);
      observer.disconnect();
    };
  }, [rows.length]);

  const lockAndRerun = useCallback(async (row: SolvedBuildDto) => {
    applyRowLocks(row);
    await runSearchFromStore();
  }, [applyRowLocks]);
  const selectedKey = rowFingerprint(selected);
  const pinnedKeys = useMemo(() => new Set(compareBench.map(rowFingerprint)), [compareBench]);

  const rowsContainer = useRef<HTMLDivElement>(null);
  // Rows stay in rank order in the DOM. A sort offsets them (they are relatively positioned, so
  // only their positions update) and gives keyboard and screen readers the new order through
  // reading-order; moving the row nodes instead restyled and laid out all of them (~15 ms per
  // sort). Transforms would avoid even that, but give each sticky rank cell its own layer.
  useLayoutEffect(() => {
    const container = rowsContainer.current;
    if (!container) return;
    placeRows(container, rankedRows);
    // Row heights follow the board width, so a resize re-places a sorted board.
    const observer = new ResizeObserver(() => placeRows(container, rankedRows));
    observer.observe(container);
    return () => observer.disconnect();
  }, [rankedRows]);

  // A sort or an updated search re-forges the ranking. Results that land while another
  // workspace is shown re-forge when the board is shown again.
  const laidOut = useRef({ rows, rankedRows });
  useLayoutEffect(() => {
    const container = rowsContainer.current;
    if (!shown) return;
    const previous = laidOut.current;
    laidOut.current = { rows, rankedRows };
    if (container && previous.rankedRows !== rankedRows) settleRanking(container, previous.rows !== rows, resultBoard.current);
  }, [rankedRows, rows, shown]);

  // Hidden workspaces lose their display and replay the stage entrance in CSS when shown; this
  // one is only skipped while hidden (styles.css), so it replays the entrance itself.
  const panel = useRef<HTMLElement>(null);
  const wasShown = useRef(shown);
  useLayoutEffect(() => {
    if (shown && !wasShown.current && !reducedMotion()) {
      panel.current?.animate(
        [{ opacity: 0, transform: "translateY(7px) scale(0.997)" }, { opacity: 1, transform: "none" }],
        { duration: 260, easing: "cubic-bezier(0.2, 0.75, 0.2, 1)" },
      );
    }
    wasShown.current = shown;
  }, [shown]);

  async function exportCsv() {
    if (useDesktopStore.getState().isSearching || useDesktopStore.getState().isExporting) return;
    const controller = new AbortController();
    const isCurrent = () => {
      const current = useDesktopStore.getState();
      return !controller.signal.aborted && current.searchGeneration === searchGeneration
        && current.activeWorkspace === "rankings";
    };
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
        if (!isCurrent()) return;
        exportCache.current = { signature, rows: exportRows };
      }
      if (!isCurrent()) return;
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
      ...request,
      weaponTypeKey: null,
      weaponName: "Uchigatana",
      affinity: "Standard",
      aowName: null,
      somberFilter: "all",
      filters: { version: 1, entries: [] },
      standardMaxUpgrade: 3,
      exactUpgrade: true,
      objective: "max_ar",
    });
    const completed = await runSearchFromStore();
    if (completed && useDesktopStore.getState().rows.length === 0) {
      pushNotice({ scope: "rankings", tone: "warning", message: "No legal Uchigatana +3 build fits your retained character constraints. Review your stat locks, floors, and available level budget, then search again." });
    }
  }

  function scrollResults(direction: -1 | 1) {
    resultBoard.current?.scrollBy({
      left: direction * 360,
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
    });
  }

  return (
    <section className="workspace-panel rankings-panel" ref={panel} data-offstage={shown ? undefined : ""}>
      <div className="workspace-header">
        <div>
          <h1>Rankings</h1>
          <span>{rows.length} ranked rows</span>
        </div>
        <div className="result-scroll-actions">
          <button
            type="button"
            title="Reverse the current order; click again to restore it"
            aria-pressed={sort.reverse}
            onClick={() => setSort((current) => ({ ...current, reverse: !current.reverse }))}
          >
            <ArrowDownUp size={15} />
            <span className="sr-only">{sort.key === "rank"
              ? sort.reverse ? "Show best rank first" : "Show lowest rank first"
              : sort.reverse ? "Show highest first" : "Show lowest first"}</span>
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
          <span role="columnheader" aria-sort={ariaSort("rank")}>
            <SortButton label="Rank" direction={ariaSort("rank")} onClick={() => sortBy("rank")}>#</SortButton>
          </span>
          <span role="columnheader" title="Weapon, affinity, skill, and reinforcement level">Loadout</span>
          <span role="columnheader" className="token-column-head" title="Attribute scaling grade at this reinforcement level">
            Scaling<StatKeys />
          </span>
          <span role="columnheader" className="token-column-head" title="Combat stats of this build">
            Stats<StatKeys />
          </span>
          <span role="columnheader" aria-sort={ariaSort("ar")} title="Raw attack rating before enemy defense and negation">
            <SortButton direction={ariaSort("ar")} onClick={() => sortBy("ar")}>AR</SortButton>
          </span>
          <span role="columnheader" aria-sort={ariaSort("skill")} title="Raw skill damage for the full route, and its first damaging hit">
            <SortButton direction={ariaSort("skill")} onClick={() => sortBy("skill")}>Skill damage</SortButton>
          </span>
          {objective !== "max_ar" ? (
            <span role="columnheader" aria-sort={ariaSort("score")} title="Value used by the active ranking objective">
              <SortButton direction={ariaSort("score")} onClick={() => sortBy("score")}>{objectiveLabel(objective)}</SortButton>
            </span>
          ) : null}
          <span role="columnheader" title="Pin for comparison or use this result as exact search locks">
            <span className="sr-only">Actions</span>
          </span>
        </div>
        {rows.length === 0 ? (
          // A grid may only own rows, so the empty state sits in one full-width cell.
          <div role="row">
            <div role="gridcell">
              {isSearching ? (
                <div className="forging-state" role="status">
                  <span>Ranking every legal setup…</span>
                  <SkeletonRows count={8} />
                </div>
              ) : (
                <EmptyRows onExample={runStarterExample} busy={isExporting} classBudget={catalog?.dataManifest.capabilities.classBudget !== false} />
              )}
            </div>
          </div>
        ) : null}
        <div className="result-rows" role="rowgroup" ref={rowsContainer} data-searching={isSearching || undefined}>
        {rows.map((row, rank) => {
          const key = rowFingerprint(row);
          return (
            <ResultRow
              key={`${key}-${rank}`}
              index={rank}
              row={row}
              movement={movements?.[rank] ?? null}
              active={selectedKey === key}
              objective={objective}
              lockDisabled={isExporting}
              aowModelSupported={aowSupported}
              extendedScalingGrades={extendedScalingGrades}
              onSelect={selectRow}
              onLock={lockAndRerun}
              pinned={pinnedKeys.has(key)}
              onPin={toggleCompareBench}
            />
          );
        })}
        </div>
      </div>
    </section>
  );
}

// The direction mark is aria-hidden so the column keeps its name; aria-sort carries the order.
function SortButton({ label, direction, onClick, children }: {
  label?: string;
  direction: "ascending" | "descending" | undefined;
  onClick: () => void;
  children: ReactNode;
}) {
  const Arrow = direction === "ascending" ? ArrowUp : ArrowDown;
  return (
    <button type="button" className={`sort-header${direction ? " active" : ""}`} aria-label={label} onClick={onClick}>
      {children}
      {direction ? <Arrow size={11} aria-hidden="true" /> : null}
    </button>
  );
}

function movementText(movement: RankMovement | null): string {
  if (!movement) return "";
  if (movement.places === null) return "new since the previous search";
  if (movement.places === 0) return "";
  return `${movement.places > 0 ? "up" : "down"} ${Math.abs(movement.places)} since the previous search`;
}

function MetricDelta({ movement, objective }: { movement: RankMovement | null; objective: ObjectiveId }) {
  const delta = movement?.metricDelta ?? null;
  if (delta === null || Math.abs(delta) < 0.05) return null;
  return (
    <small className={`metric-delta ${delta > 0 ? "up" : "down"}`} title={`${objectiveLabel(objective)} ${delta > 0 ? "+" : ""}${fixed1(delta)} since the previous search`}>
      {delta > 0 ? "+" : ""}{fixed1(delta)}
    </small>
  );
}

// Shifts each row from its rank-order place to its sorted one. Rows stack without gaps, so both
// places are running sums of row heights; fractional heights keep 50 rows from drifting apart.
function placeRows(container: HTMLElement, ranked: RankedEntry[]) {
  const rows = [...container.children].filter((row): row is HTMLElement => row instanceof HTMLElement);
  // Rank order needs no measuring, so fresh results never force a layout here.
  const unsorted = ranked.every(({ rank }, position) => rank === position);
  const heights = unsorted ? [] : rows.map((row) => row.getBoundingClientRect().height);
  const natural: number[] = [];
  heights.reduce((top, height, rank) => { natural[rank] = top; return top + height; }, 0);
  let top = 0;
  ranked.forEach(({ rank }, position) => {
    const row = rows[rank];
    if (!row) return;
    const shift = unsorted ? 0 : top - natural[rank];
    row.style.top = shift ? `${shift}px` : "";
    row.style.setProperty("reading-order", String(position));
    if (!unsorted) top += heights[rank];
  });
}

// Arrow keys move between rows like a grid, in the order shown; Enter and Space still select.
function moveRowFocus(row: HTMLElement, key: string) {
  const top = (element: HTMLElement) => element.getBoundingClientRect().top;
  const rows = [...(row.parentElement?.querySelectorAll<HTMLElement>(".result-row-full") ?? [])]
    .sort((a, b) => top(a) - top(b));
  const index = rows.indexOf(row);
  const target = key === "Home" ? rows[0] : key === "End" ? rows.at(-1) : rows[index + (key === "ArrowDown" ? 1 : -1)];
  target?.focus();
}

// Memoised with row-taking handlers, so a selection, sort or query edit re-renders only the
// rows whose own props changed.
const ResultRow = memo(function ResultRow({
  row,
  index,
  movement,
  active,
  objective,
  lockDisabled,
  aowModelSupported,
  extendedScalingGrades,
  onSelect,
  onLock,
  pinned,
  onPin,
}: {
  row: SolvedBuildDto;
  index: number;
  movement: RankMovement | null;
  active: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  aowModelSupported: boolean;
  extendedScalingGrades: boolean;
  onSelect: (row: SolvedBuildDto) => void;
  onLock: (row: SolvedBuildDto) => void;
  lockDisabled: boolean;
  pinned: boolean;
  onPin: (row: SolvedBuildDto) => void;
}) {
  const aowAvailable = hasAowDamage(row, aowModelSupported);
  const metric = metricForObjective(row, objective, aowModelSupported);
  return (
    <div
      className={`result-row result-row-full ${objective !== "max_ar" ? "with-score" : ""} ${active ? "active" : ""}`}
      role="row"
      aria-selected={active}
      aria-label={`Select ${row.weaponName}, ${row.affinity}, rank ${index + 1}${movementText(movement) ? `, ${movementText(movement)}` : ""}`}
      title="Select this build"
      tabIndex={0}
      onClick={() => onSelect(row)}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) {
          return;
        }
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect(row);
        } else if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
          event.preventDefault();
          moveRowFocus(event.currentTarget, event.key);
        }
      }}
    >
      <span role="gridcell" className="rank-cell">
        {index + 1}
        {movement && movement.places !== 0 ? (
          <small className={`rank-move ${movement.places === null ? "new" : movement.places > 0 ? "up" : "down"}`} title={movementText(movement)}>
            {movement.places === null ? "New" : <>{movement.places > 0 ? <ArrowUp size={10} aria-hidden="true" /> : <ArrowDown size={10} aria-hidden="true" />}{Math.abs(movement.places)}</>}
          </small>
        ) : null}
      </span>
      <span role="gridcell" className="weapon-cell">
        <span className="weapon-line">
          <strong>{row.weaponName}</strong>
          {row.isSomber ? <span className="loadout-tag">Somber</span> : null}
        </span>
        <small className="loadout-line">
          <span className="loadout-affinity">{row.affinity}</span>
          <span className="loadout-skill">{row.aowName ?? "No skill"}</span>
          <span className="loadout-upgrade">+{row.upgrade}</span>
        </small>
      </span>
      <span role="gridcell" className="token-cell scaling-cell">
        <ScalingTokens scaling={row.effectiveScaling} extended={extendedScalingGrades} />
      </span>
      <span role="gridcell" className="token-cell">
        <StatTokens row={row} />
      </span>
      <span role="gridcell" className="result-metric-cell ar-cell">
        <strong>{fixed1(row.ar.total)}</strong>
        {objective === "max_ar" ? <MetricDelta movement={movement} objective={objective} /> : null}
      </span>
      <span role="gridcell" className="result-metric-cell skill-cell" title={aowAvailable ? undefined : "Skill damage isn't modeled for this loadout."}>
        {aowAvailable
          ? <><strong>{compactNumber(row.aowFullSequenceDamage)}</strong><small>1st hit {compactNumber(row.aowFirstHitDamage)}</small></>
          : <span className="result-unavailable">Unavailable</span>}
      </span>
      {objective !== "max_ar" ? (
        <span role="gridcell" className="objective-score">
          {metric === null ? "Unavailable" : fixed1(metric)}
          <MetricDelta movement={movement} objective={objective} />
        </span>
      ) : null}
      <span role="gridcell">
        <button
          className="inline-lock"
          type="button"
          aria-pressed={pinned}
          aria-label={`${pinned ? "Unpin" : "Compare"} ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
          onClick={(event) => {
            event.stopPropagation();
            onPin(row);
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
            void onLock(row);
          }}
        >
          <LockKeyhole size={15} aria-hidden="true" />
        </button>
      </span>
    </div>
  );
});

/** Column keys shown once in the header so rows can carry bare values. */
function StatKeys() {
  return (
    <span className="token-keys" aria-hidden="true">
      {STAT_KEYS.map((key) => <span key={key}>{key}</span>)}
    </span>
  );
}

function EmptyRows({ onExample, busy, classBudget }: { onExample: () => void; busy: boolean; classBudget: boolean }) {
  return (
    <div className="empty-state">
      <strong>No rankings loaded</strong>
      <span>Press Search to rank every legal setup under the active query.</span>
      <small>Open loadout fields keep all compatible options eligible.</small>
      {classBudget ? (
        <>
          <small>Character stats, floors, locks, and world settings are retained by the example.</small>
          <button className="inline-lock" type="button" onClick={onExample} disabled={busy}>Try Uchigatana +3 example</button>
        </>
      ) : <small>Convergence uses your entered combat stats exactly.</small>}
    </div>
  );
}
