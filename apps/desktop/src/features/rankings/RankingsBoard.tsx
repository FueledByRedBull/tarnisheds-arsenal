import { ArrowDown, ArrowDownUp, ArrowUp, ChevronLeft, ChevronRight, CircleHelp, Download, LockKeyhole, Pin, RefreshCcw } from "lucide-react";
import { memo, ReactNode, useCallback, useDeferredValue, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { downloadText, rankingsCsvFilename, rankingsToCsv } from "../../lib/csv";
import { aowModelSupported, compactNumber, errorMessage, fixed1, hasAowDamage, metricForObjective, objectiveLabel } from "../../lib/format";
import { buildOptimizeRequest, rowFingerprint, usesFixedStats } from "../../lib/session";
import { EASE_OUT, reducedMotion, settleRanking } from "../../lib/motion";
import { RankedEntry, RankMovement, RankSortKey, rankMovements, sortRanked } from "../../lib/ranking-view";
import { useDesktopStore } from "../../lib/state";
import { ObjectiveId, SearchProgressDto, SolvedBuildDto } from "../../lib/types";
import { runSearchFromStore, runSearchRequestForRows } from "../../lib/workflows";
import packageInfo from "../../../package.json";
import { COMBAT_STAT_LABELS, ScalingTokens, StatTokens } from "../shared/BuildMetricTokens";
import { Popover } from "../shared/Popover";
import { SkeletonRows } from "../shared/SkeletonRows";

// What each column means, said once in the column guide and read as each header's description.
const COLUMN_GUIDE = [
  ["loadout", "Loadout", "Weapon, affinity, skill, and reinforcement level."],
  ["scaling", "Scaling", "Attribute contribution grade at the shown reinforcement level."],
  ["stats", "Stats", "The combat stats this build uses."],
  ["ar", "AR", "Raw attack rating before enemy defense and negation. Build Detail splits it by damage type."],
  ["skill", "Skill damage", "Raw damage of one complete legal Ash of War route, with its first damaging hit. Native skills are the weapon's fixed skill."],
  ["score", "Objective", "The value the active ranking objective sorts by."],
  ["pin", "Pin (P)", "Adds the result to Compare as a target."],
  ["lock", "Lock stats (L)", "Copies the result's loadout, upgrade, and combat stats into the next search as exact stat locks."],
] as const;
const guideId = (key: string) => `column-guide-${key}`;

export function RankingsBoard() {
  const liveRows = useDesktopStore((state) => state.rows);
  // New results render in a deferred pass, which React splits into short tasks, and commit
  // with their movement baseline; the board keeps its searching look until then.
  const rows = useDeferredValue(liveRows);
  const rankBaseline = useDeferredValue(useDesktopStore((state) => state.rankBaseline));
  const liveSelected = useDesktopStore((state) => state.selected);
  // The selection results bring waits for their rows; a click on a row shows at once.
  const deferredSelected = useDeferredValue(liveSelected);
  const selected = rows === liveRows ? liveSelected : deferredSelected;
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
  // The board stays rendered while another workspace is shown (App.tsx), so it follows that here,
  // deferred like the other workspaces so that both switch in the same commit.
  const shown = useDeferredValue(useDesktopStore((state) => state.activeWorkspace === "rankings"));
  const objective = useDesktopStore((state) => state.request.objective);
  const isExporting = useDesktopStore((state) => state.isExporting);
  const setExporting = useDesktopStore((state) => state.setExporting);
  const exportController = useRef<AbortController | null>(null);
  const [exportProgress, setExportProgress] = useState<SearchProgressDto | null>(null);
  const exportCache = useRef<{ signature: string; rows: SolvedBuildDto[] } | null>(null);
  const [sort, setSort] = useState<{ key: RankSortKey; reverse: boolean }>({ key: "rank", reverse: false });
  const [horizontalScroll, setHorizontalScroll] = useState({ overflow: false, left: false, right: false });
  const resultBoard = useRef<HTMLDivElement>(null);
  const aowSupported = aowModelSupported(catalog?.dataManifest);
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
  // A column that would read "Unavailable" on every row says so once, in the header, instead.
  const skillColumn = rows.length === 0 || rows.some((row) => hasAowDamage(row, aowSupported));
  const gridClass = `${objective !== "max_ar" ? "with-score" : ""} ${skillColumn ? "" : "no-skill"}`;
  const profileRules = catalog?.dataManifest.rules;
  const separateUpgradeCaps = profileRules?.separateUpgradeCaps ?? true;
  const scadutreeAvailable = profileRules?.scadutreeScaling ?? true;
  const extendedScalingGrades = profileRules?.extendedScalingGrades ?? false;

  // Any input or profile change cancels an export; exportCsv cancels it on leaving Rankings.
  useEffect(() => () => exportController.current?.abort(), [catalog, request, lockedStatMode, searchGeneration]);

  // The board's scroll position and height as of its last scroll or resize, read without layout.
  const view = useRef({ top: 0, height: 0 });
  useEffect(() => {
    const board = resultBoard.current;
    if (!board) return;
    const update = () => {
      view.current = { top: board.scrollTop, height: board.clientHeight };
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
  // One tab stop for the whole grid: the selected row, or the first row before any selection.
  const tabStopKey = rows.some((row) => rowFingerprint(row) === selectedKey) ? selectedKey : rowFingerprint(rows[0] ?? null);

  const rowsContainer = useRef<HTMLDivElement>(null);
  // Rows stay in rank order in the DOM and a sort sets their flex order, so only the rows' own
  // styles and positions update (1.6 ms for 50 rows); reading-flow gives keyboard and screen
  // readers the order shown. Moving the row nodes restyled and laid out all of them (~15 ms), and
  // rendering each place's row into it rewrote every row's contents (8 ms).
  useLayoutEffect(() => {
    const rowElements = rowsContainer.current?.children;
    rankedRows.forEach(({ rank }, place) => {
      const row = rowElements?.[rank];
      if (row instanceof HTMLElement) row.style.order = String(place);
    });
  }, [rankedRows]);

  // Rows well outside the board's view rest (styles.css) while Rankings is hidden, while the
  // window is resized and as new results land, so a return, a width step or a landing renders
  // only the rows near the view instead of all 50; the others wake a few per frame afterwards.
  const nearRows = useRef(new Set<Element>());
  const wakeFrame = useRef(0);
  const resizing = useRef(0);

  // A sort or an updated search re-forges the ranking. Results that land while another
  // workspace is shown re-forge when the board is shown again.
  const laidOut = useRef({ rows, rankedRows });
  useLayoutEffect(() => {
    const container = rowsContainer.current;
    if (!shown) return;
    const previous = laidOut.current;
    laidOut.current = { rows, rankedRows };
    if (!container || previous.rankedRows === rankedRows) return;
    const freshResults = previous.rows !== rows;
    if (freshResults) {
      // Rows keep their places' heights when the count holds; otherwise the view's height
      // bounds how many rows can reach it.
      restRows(container, nearRows.current.size && previous.rows.length === rows.length
        ? rowsAtSamePlaces(container, nearRows.current, previous.rankedRows, rankedRows)
        : rowsWithinView(container, rankedRows, view.current));
      // Waking starts in the frame after the one that draws the results; a resize in
      // progress wakes them when it settles.
      if (!resizing.current) {
        cancelAnimationFrame(wakeFrame.current);
        wakeFrame.current = requestAnimationFrame(() => { wakeFrame.current = requestAnimationFrame(() => wakeRows(container, wakeFrame)); });
      }
    }
    settleRanking(container, freshResults, resultBoard.current);
  }, [rankedRows, rows, shown]);

  // Hidden workspaces lose their display and replay the stage entrance in CSS when shown; this
  // one is only skipped while hidden (styles.css), so it replays the entrance itself.
  const panel = useRef<HTMLElement>(null);
  const wasShown = useRef(shown);
  useLayoutEffect(() => {
    if (shown && !wasShown.current && !reducedMotion()) {
      panel.current?.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 140, easing: EASE_OUT });
    }
    const container = rowsContainer.current;
    if (container && shown !== wasShown.current) {
      cancelAnimationFrame(wakeFrame.current);
      if (!shown) restRows(container, nearRows.current);
      // Waking starts in the frame after the one that shows the board.
      else wakeFrame.current = requestAnimationFrame(() => { wakeFrame.current = requestAnimationFrame(() => wakeRows(container, wakeFrame)); });
    }
    wasShown.current = shown;
  }, [shown]);

  useEffect(() => {
    const board = resultBoard.current;
    const container = rowsContainer.current;
    if (!board || !container) return;
    const near = nearRows.current;
    near.clear();
    const offstage = () => panel.current?.hasAttribute("data-offstage") ?? true;
    const observer = new IntersectionObserver((entries) => {
      // A hidden board reports every row out of view, so it keeps the rows it last showed.
      if (offstage()) return;
      for (const { target, isIntersecting } of entries) {
        if (!isIntersecting) {
          near.delete(target);
        } else {
          near.add(target);
          target.removeAttribute("data-resting");
        }
      }
    }, { root: board, rootMargin: "96px 0px" });
    for (const row of container.children) observer.observe(row);
    const onResize = () => {
      if (offstage()) return;
      if (!resizing.current) {
        cancelAnimationFrame(wakeFrame.current);
        restRows(container, near);
      }
      clearTimeout(resizing.current);
      resizing.current = window.setTimeout(() => {
        resizing.current = 0;
        wakeRows(container, wakeFrame);
      }, 150);
    };
    window.addEventListener("resize", onResize);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", onResize);
    };
  }, [rows.length]);
  // A pending wake outlives a change in the row count, which new results can bring.
  useEffect(() => () => {
    clearTimeout(resizing.current);
    cancelAnimationFrame(wakeFrame.current);
  }, []);

  async function exportCsv(requestedRows: number) {
    if (useDesktopStore.getState().isSearching || useDesktopStore.getState().isExporting) return;
    const controller = new AbortController();
    const isCurrent = () => {
      const current = useDesktopStore.getState();
      return !controller.signal.aborted && current.searchGeneration === searchGeneration
        && current.activeWorkspace === "rankings";
    };
    // Watches the urgent store: the deferred `shown` can miss a visit elsewhere within one render.
    const stop = useDesktopStore.subscribe(() => { if (!isCurrent()) controller.abort(); });
    exportController.current = controller;
    setExporting(true);
    setExportProgress(null);
    setError(null);
    try {
      const exportRequest = {
        ...buildOptimizeRequest(catalog, request, lockedStatMode),
        topK: requestedRows,
      };
      const signature = JSON.stringify([catalog?.dataManifest, exportRequest]);
      let exportRows: SolvedBuildDto[];
      if (!resultsStale && requestedRows <= liveRows.length) {
        exportRows = liveRows.slice(0, requestedRows);
      } else if (exportCache.current?.signature === signature) {
        exportRows = exportCache.current.rows;
      } else {
        exportRows = await runSearchRequestForRows(exportRequest, controller.signal, setExportProgress);
        if (!isCurrent()) return;
        exportCache.current = { signature, rows: exportRows };
      }
      if (!isCurrent()) return;
      if (!catalog) throw new Error("Catalog metadata is unavailable; the export was not created.");
      downloadText(rankingsCsvFilename(request.profileId), rankingsToCsv(exportRows, {
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
        aowModelSupported: aowSupported,
        extendedScalingGrades,
      }), "text/csv;charset=utf-8");
      pushNotice({
        scope: "rankings",
        tone: "success",
        message: `Exported ${exportRows.length} ranked rows to your Downloads folder.`,
      });
    } catch (error) {
      if (!controller.signal.aborted) setError(errorMessage(error));
    } finally {
      stop();
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
      behavior: reducedMotion() ? "auto" : "smooth",
    });
  }

  return (
    <section className="workspace-panel rankings-panel" ref={panel} data-offstage={shown ? undefined : ""}>
      <div className="workspace-header">
        <div>
          <h1>Rankings</h1>
          {rows.length ? <span>{rows.length} ranked {rows.length === 1 ? "row" : "rows"}</span> : null}
          {skillColumn ? null : <small>Skill damage isn't modeled for these results.</small>}
        </div>
        <div className="result-scroll-actions">
          <Popover
            label="Column guide"
            trigger={<><CircleHelp size={15} aria-hidden="true" /><span>Column guide</span></>}
            triggerLabel="Column guide"
            triggerClassName="column-guide-trigger"
            panelClassName="editor-panel column-guide-panel"
          >
            <dl className="editor-body column-guide">
              {COLUMN_GUIDE.map(([key, term, definition]) => (
                <div key={key}><dt>{term}</dt><dd id={guideId(key)}>{definition}</dd></div>
              ))}
            </dl>
          </Popover>
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
          {isExporting ? (
            <button className="export-csv-button" type="button" onClick={() => exportController.current?.abort()}>
              <Download size={16} aria-hidden="true" /><span>Cancel export</span>
            </button>
          ) : (
            <Popover
              label="Export CSV"
              trigger={<><Download size={16} aria-hidden="true" /><span>Export CSV</span></>}
              triggerLabel="Export CSV"
              triggerClassName="export-csv-button"
              panelClassName="editor-panel export-panel"
              disabled={isSearching}
            >
              {(close) => (
                <div className="editor-body">
                  <div className="export-choices" role="group" aria-label="Rows to export">
                    {[25, 100, 500, 2000].map((limit) => (
                      <button type="button" key={limit} onClick={() => { close(); void exportCsv(limit); }}>
                        {limit === 2000 ? "2,000 rows (max)" : `${limit} rows`}
                      </button>
                    ))}
                  </div>
                  <p className="editor-note">Rows beyond the current results are ranked again for the file, which lists every assumption.</p>
                </div>
              )}
            </Popover>
          )}
        </div>
      </div>
      {isExporting ? <div className="estimate-strip" role="status">
        <span>Exporting CSV</span>
        <strong>{exportProgress ? `${exportProgress.checked.toLocaleString()} / ${exportProgress.total.toLocaleString()}` : "Preparing search…"}</strong>
      </div> : null}
      {resultsStale && rows.length > 0 ? (
        <div className="stale-results-banner" id="stale-results-message" role="status">
          <div>
            <strong>Inputs changed</strong>
            <span>These rankings are retained from the previous query until the updated search finishes.</span>
          </div>
          <button type="button" onClick={() => void runSearchFromStore()} disabled={isSearching || isExporting}>
            <RefreshCcw size={14} />{isSearching ? "Updating…" : "Run updated search"}
          </button>
        </div>
      ) : null}
      <div
        ref={resultBoard}
        className="result-board"
        role="grid"
        aria-label={resultsStale ? "Ranked builds from the previous query" : "Ranked builds"}
        aria-describedby={resultsStale ? "stale-results-message" : undefined}
      >
        <div className={`result-head ${gridClass}`} role="row">
          <span role="columnheader" aria-sort={ariaSort("rank")}>
            <SortButton label="Rank" direction={ariaSort("rank")} onClick={() => sortBy("rank")}>#</SortButton>
          </span>
          <span role="columnheader" title="Weapon, affinity, skill, and reinforcement level" aria-describedby={guideId("loadout")}>Loadout</span>
          <span role="columnheader" className="token-column-head" title="Attribute scaling grade at this reinforcement level" aria-describedby={guideId("scaling")}>
            Scaling<StatKeys />
          </span>
          <span role="columnheader" className="token-column-head" title="Combat stats of this build" aria-describedby={guideId("stats")}>
            Stats<StatKeys />
          </span>
          <span role="columnheader" aria-sort={ariaSort("ar")} title="Raw attack rating before enemy defense and negation">
            <SortButton direction={ariaSort("ar")} describedBy={guideId("ar")} onClick={() => sortBy("ar")}>AR</SortButton>
          </span>
          {skillColumn ? (
            <span role="columnheader" aria-sort={ariaSort("skill")} title="Raw skill damage for the full route, and its first damaging hit">
              <SortButton direction={ariaSort("skill")} describedBy={guideId("skill")} onClick={() => sortBy("skill")}>Skill damage</SortButton>
            </span>
          ) : null}
          {objective !== "max_ar" ? (
            <span role="columnheader" aria-sort={ariaSort("score")} title="Value used by the active ranking objective">
              <SortButton direction={ariaSort("score")} describedBy={guideId("score")} onClick={() => sortBy("score")}>{objectiveLabel(objective)}</SortButton>
            </span>
          ) : null}
          <span role="columnheader" title="Pin for comparison (P) or lock stats to this result (L)" aria-describedby={`${guideId("pin")} ${guideId("lock")}`}>
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
                <EmptyRows onExample={() => void runStarterExample()} busy={isExporting} classBudget={!usesFixedStats(catalog, request.className)} />
              )}
            </div>
          </div>
        ) : null}
        <div className="result-rows" role="rowgroup" ref={rowsContainer} data-searching={isSearching || rows !== liveRows || undefined}>
        {rows.map((row, rank) => {
          const key = rowFingerprint(row);
          return (
            <ResultRow
              // Keyed by place: new results update the rows already on screen instead of
              // rebuilding all 50 (~3,200 elements) and leaving the old ones as garbage.
              key={rank}
              index={rank}
              row={row}
              movement={movements?.[rank] ?? null}
              active={selectedKey === key}
              tabStop={tabStopKey === key}
              objective={objective}
              gridClass={gridClass}
              skillColumn={skillColumn}
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
function SortButton({ label, direction, describedBy, onClick, children }: {
  label?: string;
  direction: "ascending" | "descending" | undefined;
  describedBy?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  const Arrow = direction === "ascending" ? ArrowUp : ArrowDown;
  return (
    <button type="button" className={`sort-header${direction ? " active" : ""}`} aria-label={label} aria-describedby={describedBy} onClick={onClick}>
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

// Rests every row outside `near` except one holding focus. Without a row in view yet, none rest.
function restRows(container: HTMLElement, near: Set<Element>) {
  if (!near.size) return;
  for (const row of container.children) {
    if (!near.has(row) && !row.contains(document.activeElement)) row.setAttribute("data-resting", "");
  }
}

// The rows a new ranking shows at the places the `near` rows hold now: under a sort, new
// results can reorder which row is drawn there.
function rowsAtSamePlaces(container: HTMLElement, near: Set<Element>, previous: RankedEntry[], next: RankedEntry[]) {
  const rows = [...container.children];
  const placeOf: number[] = [];
  previous.forEach(({ rank }, place) => { placeOf[rank] = place; });
  const shownThere = new Set<Element>();
  for (const row of near) {
    const entry = next[placeOf[rows.indexOf(row)]];
    if (entry) shownThere.add(rows[entry.rank]);
  }
  return shownThere;
}

// The first rows shown, as many as could reach the view and the observer's margin: no row is
// shorter than 48 px (styles.css). Without a measured view, none.
function rowsWithinView(container: HTMLElement, order: RankedEntry[], view: { top: number; height: number }) {
  const rows = [...container.children];
  const count = view.height ? Math.ceil((view.top + view.height + 96) / 48) : 0;
  return new Set(order.slice(0, count).map(({ rank }) => rows[rank]));
}

// Wakes resting rows twelve per frame; a hidden board keeps them resting until it is shown.
function wakeRows(container: HTMLElement, frame: { current: number }) {
  frame.current = 0;
  if (container.closest("[data-offstage]")) return;
  const resting = [...container.querySelectorAll("[data-resting]")];
  resting.slice(0, 12).forEach((row) => row.removeAttribute("data-resting"));
  if (resting.length > 12) frame.current = requestAnimationFrame(() => wakeRows(container, frame));
}

// Arrow keys move between rows like a grid, in the order shown; Enter and Space still select.
function moveRowFocus(row: HTMLElement, key: string) {
  const order = (element: HTMLElement) => Number(element.style.order);
  const rows = [...(row.parentElement?.querySelectorAll<HTMLElement>(".result-row") ?? [])]
    .sort((a, b) => order(a) - order(b));
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
  tabStop,
  objective,
  gridClass,
  skillColumn,
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
  tabStop: boolean;
  objective: Parameters<typeof metricForObjective>[1];
  gridClass: string;
  skillColumn: boolean;
  aowModelSupported: boolean;
  extendedScalingGrades: boolean;
  onSelect: (row: SolvedBuildDto) => void;
  onLock: (row: SolvedBuildDto) => Promise<void>;
  lockDisabled: boolean;
  pinned: boolean;
  onPin: (row: SolvedBuildDto) => void;
}) {
  const aowAvailable = hasAowDamage(row, aowModelSupported);
  const metric = metricForObjective(row, objective, aowModelSupported);
  // The name carries the ranked number, so arrowing through rows reads the answer itself.
  const metricName = `${objective === "max_ar" ? "AR" : objectiveLabel(objective)} ${metric === null ? "unavailable" : fixed1(metric)}`;
  return (
    <div
      className={`result-row ${gridClass} ${active ? "active" : ""}`}
      role="row"
      aria-selected={active}
      aria-label={`Select ${row.weaponName}, ${row.affinity}, rank ${index + 1}, ${metricName}${movementText(movement) ? `, ${movementText(movement)}` : ""}`}
      aria-keyshortcuts="P L"
      tabIndex={tabStop ? 0 : -1}
      onClick={() => onSelect(row)}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) {
          return;
        }
        const key = event.key.toLowerCase();
        const plain = !event.ctrlKey && !event.metaKey && !event.altKey;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect(row);
        } else if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
          event.preventDefault();
          moveRowFocus(event.currentTarget, event.key);
        } else if (plain && key === "p") {
          event.preventDefault();
          onPin(row);
        } else if (plain && key === "l" && !lockDisabled) {
          event.preventDefault();
          void onLock(row);
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
      <span role="gridcell" className="scaling-cell">
        <ScalingTokens scaling={row.effectiveScaling} extended={extendedScalingGrades} labels={false} />
      </span>
      <span role="gridcell">
        <StatTokens row={row} />
      </span>
      <span role="gridcell" className="result-metric-cell ar-cell">
        <strong>{fixed1(row.ar.total)}</strong>
        {objective === "max_ar" ? <MetricDelta movement={movement} objective={objective} /> : null}
      </span>
      {skillColumn ? (
        <span role="gridcell" className="result-metric-cell skill-cell" title={aowAvailable ? undefined : "Skill damage isn't modeled for this loadout."}>
          {aowAvailable
            ? <><strong>{compactNumber(row.aowFullSequenceDamage)}</strong><small>1st hit {compactNumber(row.aowFirstHitDamage)}</small></>
            : <span className="result-unavailable">Unavailable</span>}
        </span>
      ) : null}
      {objective !== "max_ar" ? (
        <span role="gridcell" className="objective-score">
          {metric === null ? "Unavailable" : fixed1(metric)}
          <MetricDelta movement={movement} objective={objective} />
        </span>
      ) : null}
      {/* Row actions sit outside the tab order: the focused row's P and L reach them, so 25
          rows are 25 tab stops, not 75. They stay clickable and readable. */}
      <span role="gridcell">
        <button
          className="inline-lock"
          type="button"
          tabIndex={-1}
          aria-pressed={pinned}
          aria-keyshortcuts="P"
          aria-label={`${pinned ? "Unpin" : "Compare"} ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
          title={pinned ? "Unpin from comparison (P)" : "Pin for comparison (P)"}
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
          tabIndex={-1}
          aria-keyshortcuts="L"
          aria-label={`Lock stats to ${row.weaponName}, ${row.affinity}, rank ${index + 1}`}
          title="Lock stats to this result (L)"
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
      {COMBAT_STAT_LABELS.map((key) => <span key={key}>{key}</span>)}
    </span>
  );
}

// First run: say what to do next, once. Search itself stays in the strip above.
function EmptyRows({ onExample, busy, classBudget }: { onExample: () => void; busy: boolean; classBudget: boolean }) {
  return (
    <div className="empty-state">
      <strong>Find the best weapon for this character</strong>
      <span>
        {classBudget ? "Set your class and stats above, then press Search (Ctrl+Enter)." : "Enter your combat stats above, then press Search (Ctrl+Enter)."}
        {" "}Every legal weapon, affinity and skill is ranked unless you narrow the loadout.
      </span>
      {classBudget ? (
        <button
          className="inline-lock"
          type="button"
          onClick={onExample}
          disabled={busy}
          title="Keeps your stats, minimums, locks and world settings"
        >
          Try an Uchigatana +3 example
        </button>
      ) : null}
    </div>
  );
}
