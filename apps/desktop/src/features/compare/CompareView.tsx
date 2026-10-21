import { SlidersHorizontal, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { AowSelect } from "../../lib/AowSelect";
import { cachedComparisonSearch, cachedSolveBuild, cachedUpgradeSeries, cachedWeaponProfile } from "../../lib/analysis-cache";
import { aowModelSupported, compactNumber, errorMessage, fixed1, hasAowDamage, metricForObjective, objectiveLabel, statLine } from "../../lib/format";
import { CheckboxMultiSelect, SearchableSelect, openOption } from "../../lib/SearchableSelect";
import { clearTypeAndAffinityFilters, compareUpgradeHorizon, replaceFilterEntries, rowFingerprint, stableSignature, upgradeCapForRow, usesFixedStats } from "../../lib/session";
import { selectedFilterIds } from "../query-strip/QueryEditors";
import { LatestRequest } from "../../lib/request-generation";
import { useBuiltRequest } from "../../lib/hooks";
import { reducedMotion } from "../../lib/motion";
import { useDesktopStore } from "../../lib/state";
import { CompareControls, SolvedBuildDto, UpgradePointDto } from "../../lib/types";
import { Popover } from "../shared/Popover";
import { WorkspaceHeading } from "../shared/WorkspaceHeading";
import { WorkspaceGate } from "../shared/WorkspaceGate";
import { ScalingTokens, StatusTokens } from "../shared/BuildMetricTokens";
import { LoadoutTradeoffs } from "./LoadoutTradeoffs";

type CompareLane = {
  label: string;
  row: SolvedBuildDto | null;
  points: UpgradePointDto[];
  chartStatus: "idle" | "loading" | "ready" | "error";
  chartError?: string;
  emptyLabel?: string;
};

type CompareChip = { key: string; kind: string; label: string; patch: Partial<CompareControls> };

// The id keys the column; the label is display text only.
type CompareMetric = {
  id: string;
  label: string;
  value: (row: SolvedBuildDto) => number | null;
  direction?: 1 | -1;
};

const PRIMARY_METRICS = new Set(["objective", "ar", "bleed", "aowFull"]);

export function CompareView() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const rows = useDesktopStore((state) => state.rows);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const target = useDesktopStore((state) => state.compareTarget);
  const restoredTarget = useDesktopStore((state) => state.restoredCompareTarget);
  const compareBench = useDesktopStore((state) => state.compareBench);
  const clearCompareBench = useDesktopStore((state) => state.clearCompareBench);
  const setCompareTarget = useDesktopStore((state) => state.setCompareTarget);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const isExporting = useDesktopStore((state) => state.isExporting);
  const compareControls = useDesktopStore((state) => state.compareControls);
  const patchCompareControls = useDesktopStore((state) => state.patchCompareControls);
  const setError = useDesktopStore((state) => state.setError);
  const matrixRef = useRef<HTMLDivElement | null>(null);
  const seriesRequest = useRef(new LatestRequest());
  // The inputs of the last comparison that finished. Showing the tab again re-runs the effect
  // below (Activity); with unchanged inputs its lanes and charts are still on screen.
  const finishedComparison = useRef<string | null>(null);
  const baseRequest = useBuiltRequest(catalog, request, lockedStatMode);
  const [series, setSeries] = useState<CompareLane[]>([]);
  const [seriesStatus, setSeriesStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [seriesError, setSeriesError] = useState<string | null>(null);
  const typeDimension = catalog?.filterDimensions.find((dimension) => dimension.id === "weapon_type");
  const affinityDimension = catalog?.filterDimensions.find((dimension) => dimension.id === "affinity");
  const filterEntries = compareControls.filters.entries;
  const selectedTypeIds = selectedFilterIds(typeDimension, filterEntries, null);
  const selectedAffinityIds = selectedFilterIds(affinityDimension, filterEntries, null);
  const excludedTypeIds = selectedFilterIds(typeDimension, filterEntries, null, "exclude");
  const excludedAffinityIds = selectedFilterIds(affinityDimension, filterEntries, null, "exclude");
  const selectedTypeLabels = typeDimension?.options
    .filter((option) => selectedTypeIds.includes(option.id))
    .map((option) => option.label) ?? [];
  const selectedAffinityLabels = affinityDimension?.options
    .filter((option) => selectedAffinityIds.includes(option.id))
    .map((option) => option.label) ?? [];
  const aowAffinity = selectedAffinityLabels.length === 1 ? selectedAffinityLabels[0] : null;
  const customCompare = Boolean(
    compareControls.weaponName
    || compareControls.filters.entries.length
    || compareControls.aowName
    || !compareControls.matchSelectedAow
    || !compareControls.includeSmithing
    || !compareControls.includeSomber,
  );
  const compareTargetLabel = compareControls.weaponName
    ?? (selectedTypeLabels.length ? `Best ${selectedTypeLabels.join(" + ")}` : "Best matching weapon");
  const reinforcementLabel = compareControls.includeSmithing && compareControls.includeSomber
    ? null
    : compareControls.includeSomber
      ? "Somber"
      : compareControls.includeSmithing ? "Smithing" : "No reinforcement";
  // Custom comparisons show their filters as chips instead.
  const compareSource = compareBench.length
    ? `${restoredTarget ? "Saved target and " : ""}${compareBench.length} pinned target${compareBench.length === 1 ? "" : "s"}`
    : restoredTarget ? "Saved target" : "Current ranked rivals";
  const pinnedMatchesSelected = Boolean(selected)
    && compareBench.length > 0
    && compareBench.every((row) => rowFingerprint(row) === rowFingerprint(selected));
  const emptyTargetLabel = pinnedMatchesSelected
    ? "The pinned target is already the selected baseline. Pin a different build or choose comparison filters."
    : "No other current ranked result. Run a broader Rankings search or choose comparison filters.";
  const extendedScalingGrades = catalog?.dataManifest.rules.extendedScalingGrades ?? false;
  // Every non-default control is one removable chip, so customCompare is true exactly when chips exist.
  const chips: CompareChip[] = [
    ...(compareControls.weaponName
      ? [{ key: "weapon", kind: "Weapon", label: compareControls.weaponName, patch: { weaponName: null, aowName: null } }]
      : []),
    ...compareControls.filters.entries.map((entry) => {
      const dimension = entry.dimension === "affinity" ? affinityDimension : typeDimension;
      const name = dimension?.options.find((option) => option.id === entry.id)?.label ?? entry.id;
      return {
        key: `${entry.dimension}:${entry.mode}:${entry.id}`,
        kind: entry.dimension === "affinity" ? "Affinity" : "Type",
        label: entry.mode === "exclude" ? `Not ${name}` : name,
        patch: {
          aowName: null,
          matchSelectedAow: false,
          filters: { version: 1 as const, entries: compareControls.filters.entries.filter((other) => other !== entry) },
        },
      };
    }),
    ...(!compareControls.matchSelectedAow || compareControls.aowName
      ? [{ key: "aow", kind: "Skill", label: compareControls.aowName ?? "Automatic", patch: { matchSelectedAow: true, aowName: null } }]
      : []),
    ...(reinforcementLabel
      ? [{ key: "reinforcement", kind: "Reinforcement", label: reinforcementLabel === "No reinforcement" ? "None" : `${reinforcementLabel} only`,
        patch: { includeSmithing: true, includeSomber: true } }]
      : []),
  ];

  useEffect(() => {
    const signature = stableSignature({
      baseRequest,
      compareControls,
      resultsStale,
      request,
      rows,
      selected,
      compareBench,
      restoredTarget,
      isExporting,
    });
    if (finishedComparison.current === signature) return;
    finishedComparison.current = null;
    const controller = new AbortController();
    const currentRequest = seriesRequest.current;
    const token = currentRequest.begin(signature);
    const finish = () => {
      if (currentRequest.isCurrent(token)) finishedComparison.current = signature;
    };
    async function resolveRows() {
      // Stale results and a cleared selection already clear the target in the store. Showing the
      // tab can re-run this effect with the hidden tab's older inputs, so it must not clear it here.
      if (resultsStale) {
        setSeries([]);
        setSeriesError(null);
        setSeriesStatus("idle");
        finish();
        return;
      }
      if (isExporting) {
        setSeries([]);
        setSeriesStatus("loading");
        finish();
        return;
      }
      if (!selected) {
        setSeries([]);
        setSeriesStatus("idle");
        finish();
        return;
      }
      const restored = !customCompare && restoredTarget && rowFingerprint(restoredTarget) !== rowFingerprint(selected)
        ? restoredTarget : null;
      setSeries([]);
      setSeriesStatus("loading");
      setSeriesError(null);
      const resolvedSelected = selected;
      const lanes: Array<{ label: string; row: SolvedBuildDto | null; emptyLabel?: string }> = [
        { label: "Selected", row: resolvedSelected },
      ];
      let summaryTarget: SolvedBuildDto | null = null;

      if (customCompare) {
        const compareAow = compareControls.matchSelectedAow ? resolvedSelected.aowName : compareControls.aowName;
        const reinforcementSelected = compareControls.includeSmithing || compareControls.includeSomber;
        const candidates = reinforcementSelected
          ? await cachedComparisonSearch({
            ...baseRequest,
            weaponName: compareControls.weaponName,
            weaponTypeKey: null,
            affinity: null,
            aowName: compareAow,
            somberFilter: compareControls.includeSmithing === compareControls.includeSomber
              ? "all"
              : compareControls.includeSomber ? "somber_only" : "standard_only",
            filters: compareControls.filters,
            lockStr: null,
            lockDex: null,
            lockInt: null,
            lockFai: null,
            lockArc: null,
            resultGrouping: compareControls.weaponName ? "loadout" : "weapon",
            topK: 6,
          }, controller.signal)
          : [];
        const compareRow = candidates.find((row) => rowFingerprint(row) !== rowFingerprint(resolvedSelected)) ?? null;
        lanes.push({
          label: compareTargetLabel,
          row: compareRow,
          emptyLabel: reinforcementSelected
            ? "No other weapon matches these comparison filters"
            : "Select Smithing, Somber, or both",
        });
        summaryTarget = compareRow;
      } else {
        const sources = compareBench.length ? compareBench : rows;
        const rivalInputs = sources
          .map((row, index) => ({ row, index }))
          .filter(({ row }) => rowFingerprint(row) !== rowFingerprint(selected))
          .filter(({ row }) => !restored || rowFingerprint(row) !== rowFingerprint(restored))
          .slice(0, compareBench.length || 3);
        const rivals = await Promise.all(rivalInputs.map(async ({ row, index }) => {
          const label = `${compareBench.length ? "Pinned" : "Top"} #${index + 1}`;
          if (!compareBench.length) return { label, row };
          const profile = baseRequest.exactUpgrade
            ? await cachedWeaponProfile(baseRequest.profileId, row.weaponName, row.affinity, controller.signal)
            : null;
          const fixedAtZero = profile?.maxUpgrade === 0;
          const solveRequest = { ...baseRequest, lockStr: null, lockDex: null, lockInt: null, lockFai: null, lockArc: null };
          if (fixedAtZero) {
            if (profile.isSomber) solveRequest.somberMaxUpgrade = 0;
            else solveRequest.standardMaxUpgrade = 0;
          }
          return {
            label: fixedAtZero ? `${label} (+0 only)` : label,
            row: await cachedSolveBuild(solveRequest, row.weaponName, row.affinity, row.aowName, controller.signal),
          };
        }));
        if (restored) lanes.push({ label: "Saved target", row: restored });
        lanes.push(...rivals);
        summaryTarget = restored ?? rivals.find(({ row }) => row !== null)?.row ?? null;
      }

      if (!currentRequest.isCurrent(token)) return;
      setCompareTarget(summaryTarget);
      setSeries(lanes.map(lane => ({ ...lane, points: [], chartStatus: lane.row ? "loading" : "idle" })));
      setSeriesStatus("ready");
      // Verified rows are usable independently of the optional, queued charts.
      let chartFailed = false;
      await Promise.all(lanes.map(async (lane, index) => {
        if (!lane.row) return;
        const row = lane.row;
        try {
          const points = await cachedUpgradeSeries(baseRequest, row, upgradeCapForRow(row, request), controller.signal);
          if (!currentRequest.isCurrent(token)) return;
          setSeries(current => current.map((entry, i) => i === index ? { ...entry, points, chartStatus: "ready" } : entry));
        } catch (error) {
          if (!currentRequest.isCurrent(token)) return;
          chartFailed = true;
          const message = errorMessage(error);
          setSeries(current => current.map((entry, i) => i === index ? { ...entry, chartStatus: "error", chartError: message } : entry));
          setError(`${lane.label} upgrade series at level ${baseRequest.characterLevel} (${statLine(row)}): ${message}`);
        }
      }));
      // A failed chart retries the next time the tab is shown.
      if (!chartFailed) finish();
    }
    resolveRows().catch((error) => {
      controller.abort();
      if (seriesRequest.current.isCurrent(token)) {
        setSeries([]);
        const message = errorMessage(error);
        setSeriesError(message);
        setSeriesStatus("error");
        setError(message);
      }
    });
    return () => {
      controller.abort();
      currentRequest.invalidate(token);
    };
  }, [baseRequest, compareBench, compareControls, compareTargetLabel, customCompare, isExporting, request, restoredTarget, resultsStale, rows, selected, setCompareTarget, setError]);

  const matrixHorizon = compareUpgradeHorizon(request);
  const chartsLoading = series.some(lane => lane.chartStatus === "loading");

  if (!selected) {
    return <WorkspaceGate panelClass="compare-panel" title="Compare" requirement="Requires a current ranked build"
      heading="Select a ranking first" message="Run or update Rankings, then select any row to use as the baseline." />;
  }

  if (resultsStale) {
    return <WorkspaceGate panelClass="compare-panel" title="Compare" requirement="Requires current ranked results"
      heading="Update Rankings before comparing" message="The selected build and comparison targets belong to the previous query." />;
  }

  const aowSupported = aowModelSupported(catalog?.dataManifest);
  const baselineMetric = metricForObjective(selected, request.objective, aowSupported);
  const laneAow = [selected, target, ...series.map((lane) => lane.row)].some((row) => row && hasAowDamage(row, aowSupported));
  return (
    <section className="workspace-panel compare-panel">
      <div className="workspace-header analysis-workspace-header compare-workspace-header">
        <WorkspaceHeading title="Compare" aboutLabel="About Compare" about={<>
          Every comparison uses this query&apos;s level, stat budget, locks and world settings. Pinned loadouts keep their
          weapon, affinity and skill; their stats and upgrades are optimized again for the current budget, and pins that
          cannot be upgraded use +0. The selected build stays the baseline, and deltas are the compared build minus it.
        </>}>
          <small className="selected-summary">Baseline: {selected.weaponName} / {selected.affinity} / +{selected.upgrade} · {objectiveLabel(request.objective)} {baselineMetric === null ? "Unavailable" : fixed1(baselineMetric)}</small>
        </WorkspaceHeading>
      </div>
      <div className="compare-toolbar">
        <span className="compare-chip-lead">Against</span>
        {chips.length ? (
          <ul className="scope-chips" aria-label="Active comparison filters">
            {chips.map((chip) => (
              <li className="scope-chip" key={chip.key}>
                <span className="chip-kind">{chip.kind}</span>
                <span>{chip.label}</span>
                <button type="button" aria-label={`Remove ${chip.kind} ${chip.label}`} onClick={() => patchCompareControls(chip.patch)}>
                  <X size={12} aria-hidden="true" />
                </button>
              </li>
            ))}
          </ul>
        ) : <span className="compare-default">{compareSource}</span>}
        <Popover
          label="Comparison filters"
          trigger={<><SlidersHorizontal size={14} aria-hidden="true" />Filters</>}
          triggerLabel={`Comparison filters: ${chips.length ? `${chips.length} active` : "none"}`}
          triggerClassName="chip-action filters-trigger"
          panelClassName="editor-panel compare-filters-panel"
        >
          <div className="editor-body editor-grid">
            <CheckboxMultiSelect
              label="Compare Type"
              values={selectedTypeIds}
              excludedValues={excludedTypeIds}
              options={typeDimension?.options.map((option) => ({ value: option.id, label: option.label, count: option.count })) ?? []}
              onChange={(values, excludedValues) => patchCompareControls({
                weaponName: null,
                aowName: null,
                matchSelectedAow: false,
                filters: { version: 1, entries: replaceFilterEntries(compareControls.filters.entries, "weapon_type", values, excludedValues) },
              })}
            />
            <CheckboxMultiSelect
              label="Compare Affinity"
              values={selectedAffinityIds}
              excludedValues={excludedAffinityIds}
              options={affinityDimension?.options.map((option) => ({ value: option.id, label: option.label, count: option.count })) ?? []}
              onChange={(values, excludedValues) => patchCompareControls({
                aowName: null,
                matchSelectedAow: false,
                filters: { version: 1, entries: replaceFilterEntries(compareControls.filters.entries, "affinity", values, excludedValues) },
              })}
            />
            <SearchableSelect
              label="Compare Weapon"
              value={compareControls.weaponName}
              options={[
                openOption(selectedTypeLabels.length ? `Best ${selectedTypeLabels.join(" + ")}` : compareBench.length ? "Pinned targets" : "Current ranked rivals"),
                ...(catalog?.weaponNames ?? []).map((name) => ({ value: name, label: name })),
              ]}
              onChange={(weaponName) => patchCompareControls({
                weaponName,
                aowName: null,
                filters: { version: 1, entries: clearTypeAndAffinityFilters(compareControls.filters.entries) },
              })}
            />
            <AowSelect
              label="Compare AoW"
              profileId={request.profileId}
              weaponName={compareControls.weaponName}
              affinity={aowAffinity}
              catalogNames={catalog?.aowNames}
              allowMatchSelected
              value={compareControls.matchSelectedAow ? "__match_selected__" : compareControls.aowName}
              onChange={(value) =>
                patchCompareControls(
                  value === "__match_selected__"
                    ? { matchSelectedAow: true, aowName: null }
                    : { matchSelectedAow: false, aowName: value },
                )
              }
            />
            <div className="compare-reinforcement" role="group" aria-label="Compare Reinforcement">
              <span>Reinforcement</span>
              <label>
                <input
                  type="checkbox"
                  checked={compareControls.includeSmithing}
                  onChange={(event) => patchCompareControls({ includeSmithing: event.target.checked })}
                />
                Smithing
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={compareControls.includeSomber}
                  onChange={(event) => patchCompareControls({ includeSomber: event.target.checked })}
                />
                Somber
              </label>
            </div>
          </div>
        </Popover>
        {customCompare ? <button type="button" className="chip-action" onClick={() => patchCompareControls({
          weaponName: null, aowName: null, matchSelectedAow: true, includeSmithing: true, includeSomber: true,
          filters: { version: 1, entries: [] },
        })}>{compareBench.length ? "Use pinned targets" : "Use ranked rivals"}</button> : null}
        {compareBench.length ? (
          <button type="button" className="chip-action" onClick={clearCompareBench}>Clear {compareBench.length} pinned target{compareBench.length === 1 ? "" : "s"}</button>
        ) : null}
        {/* The comparison's state sits at the end of its scope line rather than on a row of its own. */}
        <div className="analysis-state compare-status" role="status" aria-live="polite">
          {seriesStatus === "loading" ? "Resolving comparison builds…" : null}
          {seriesStatus === "error" ? `Compare failed: ${seriesError}` : null}
          {seriesStatus === "ready" ? chartsLoading ? "Comparison current · Upgrade charts loading…" : "Comparison current" : null}
        </div>
      </div>
      <DeltaTable baseline={series[0]?.row ?? selected} candidates={series.slice(1)} objective={request.objective} />
      {/* The delta table answers the comparison; the full build cards are there on request. */}
      <details className="compare-build-details">
        <summary>Build details</summary>
        <div className="compare-lanes" aria-busy={seriesStatus === "loading"}>
          <Lane title="Selected baseline" row={series[0]?.row ?? selected} objective={request.objective} extendedScalingGrades={extendedScalingGrades} showAow={laneAow} emptyLabel="Selected build unavailable" />
          {series.length > 1 ? series.slice(1).map((lane) => (
            <Lane key={lane.label} title={lane.label} row={lane.row} objective={request.objective} extendedScalingGrades={extendedScalingGrades} showAow={laneAow} emptyLabel={lane.emptyLabel ?? "No compatible target"} />
          )) : <Lane title="Target" row={target} objective={request.objective} extendedScalingGrades={extendedScalingGrades} showAow={laneAow} emptyLabel={seriesStatus === "loading" ? "Loading target…" : emptyTargetLabel} />}
        </div>
      </details>
      {/* Folded: the delta table above is the answer; the per-upgrade lines are reference. */}
      <details className="compare-matrix">
        <summary>Upgrade matrix</summary>
        <div className="matrix-toolbar">
          <span>{objectiveLabel(request.objective)} at each upgrade</span>
          <div>
            <button type="button" onClick={() => scrollMatrix(matrixRef.current, -1)}>+0</button>
            <button type="button" onClick={() => scrollMatrix(matrixRef.current, 1)}>+{matrixHorizon}</button>
          </div>
        </div>
        <div className="matrix-wrap" ref={matrixRef} aria-busy={chartsLoading} role="region" aria-label="Upgrade matrix" tabIndex={0}>
          <div className="metric-matrix" role="table" aria-label="Compare upgrade metrics">
            <div className="matrix-row matrix-header" role="row">
              <span role="columnheader">Line</span>
              {Array.from({ length: matrixHorizon + 1 }, (_, upgrade) => <span role="columnheader" key={upgrade}>+{upgrade}</span>)}
            </div>
            {series.map((lane) => (
              <MatrixRow key={lane.label} lane={lane} maxUpgrade={matrixHorizon} />
            ))}
          </div>
        </div>
      </details>
      {catalog && !usesFixedStats(catalog, request.className) && catalog.dataManifest.capabilities.statusBuildup
        ? <LoadoutTradeoffs base={baseRequest} current={request} selected={selected} />
        : <p className="analysis-state">AR / bleed tradeoffs require class budgets and status modeling.</p>}
    </section>
  );
}

function DeltaTable({ baseline, candidates, objective }: { baseline: SolvedBuildDto; candidates: CompareLane[]; objective: ReturnType<typeof useDesktopStore.getState>["request"]["objective"] }) {
  const aowSupported = useDesktopStore((state) => aowModelSupported(state.catalog?.dataManifest));
  const objectiveMetric: CompareMetric[] = objective === "max_ar"
    ? []
    : [{ id: "objective", label: "Objective", value: (row) => metricForObjective(row, objective, aowSupported) }];
  const compared = [baseline, ...candidates.flatMap((lane) => lane.row ? [lane.row] : [])];
  const allMetrics: CompareMetric[] = [
    ...objectiveMetric,
    { id: "ar", label: "AR", value: (row) => row.ar.total },
    { id: "physical", label: "Physical", value: (row) => row.ar.physical },
    { id: "magic", label: "Magic", value: (row) => row.ar.magic },
    { id: "fire", label: "Fire", value: (row) => row.ar.fire },
    { id: "lightning", label: "Lightning", value: (row) => row.ar.lightning },
    { id: "holy", label: "Holy", value: (row) => row.ar.holy },
    { id: "bleed", label: "Bleed", value: (row) => row.bleedBuildup },
    { id: "aowFirst", label: "AoW first", value: (row) => hasAowDamage(row, aowSupported) ? row.aowFirstHitDamage : null },
    { id: "aowFull", label: "AoW full", value: (row) => hasAowDamage(row, aowSupported) ? row.aowFullSequenceDamage : null },
    { id: "stamina", label: "Stamina", value: (row) => hasAowDamage(row, aowSupported) ? row.aowRoute?.totalStaminaCost ?? null : null, direction: -1 },
  ];
  // A metric no compared build models is left out rather than shown as a column of "Unavailable".
  const metrics = allMetrics.filter(({ value }) => compared.some((row) => value(row) !== null));
  const primaryMetrics = metrics.filter(({ id }) => PRIMARY_METRICS.has(id));
  const renderTable = (tableMetrics: CompareMetric[], caption: string) => (
    <table>
      <caption>{caption}</caption>
      <thead><tr><th scope="col">Compared build</th>{tableMetrics.map(({ id, label }) => <th scope="col" key={id}>{label}</th>)}</tr></thead>
      <tbody>{candidates.map((lane) => lane.row ? (
        <tr key={lane.label}>
          <th scope="row">{lane.row.weaponName}<small>{lane.row.affinity}</small></th>
          {tableMetrics.map(({ id, value, direction = 1 }) => {
            const candidateValue = value(lane.row!);
            const baselineValue = value(baseline);
            if (candidateValue === null || baselineValue === null) return <td key={id}>Unavailable</td>;
            // Rounded to the shown precision first, so "0.0" is never coloured as a change.
            const delta = Number(fixed1(candidateValue - baselineValue));
            const improvement = delta * direction;
            return <td className={improvement > 0 ? "positive" : improvement < 0 ? "negative" : ""} key={id}>{delta > 0 ? "+" : ""}{fixed1(delta)}</td>;
          })}
        </tr>
      ) : null)}</tbody>
    </table>
  );
  return (
    <div className="compare-deltas">
      {!candidates.length ? <small>No comparison target is currently available.</small> : null}
      {candidates.length ? renderTable(primaryMetrics, "Primary deltas versus baseline") : null}
      {candidates.length ? (
        <details>
          <summary>Full metric breakdown</summary>
          {renderTable(metrics, "All candidate deltas versus baseline")}
        </details>
      ) : null}
    </div>
  );
}

function MatrixRow({
  lane,
  maxUpgrade,
}: {
  lane: CompareLane;
  maxUpgrade: number;
}) {
  const byUpgrade = new Map(lane.points.map((point) => [point.upgrade, point.metric]));
  return (
    <div className="matrix-row" role="row" aria-busy={lane.chartStatus === "loading"}>
      <strong role="rowheader">{lane.label}
        {lane.chartStatus === "loading" ? <small>Upgrade chart loading…</small> : null}
        {lane.chartStatus === "error" ? <small role="status">Upgrade chart unavailable: {lane.chartError}</small> : null}
      </strong>
      {Array.from({ length: maxUpgrade + 1 }, (_, upgrade) => (
        <span
          role="cell"
          className={lane.row?.upgrade === upgrade ? "current-upgrade" : undefined}
          key={`${lane.label}-${upgrade}`}
        >
          {fixed1(byUpgrade.get(upgrade))}
        </span>
      ))}
    </div>
  );
}

function Lane({
  title,
  row,
  objective,
  extendedScalingGrades,
  showAow,
  emptyLabel,
}: {
  title: string;
  row: SolvedBuildDto | null;
  objective: ReturnType<typeof useDesktopStore.getState>["request"]["objective"];
  extendedScalingGrades: boolean;
  showAow: boolean;
  emptyLabel: string;
}) {
  const aowSupported = useDesktopStore((state) => aowModelSupported(state.catalog?.dataManifest));
  const metric = row ? metricForObjective(row, objective, aowSupported) : null;
  return (
    <div className="compare-lane" role="group" aria-label={title}>
      <span>{title}</span>
      {row ? (
        <>
          <strong>{row.weaponName}</strong>
          <small>{row.affinity} / {row.aowName ?? "Unspecified skill"} / +{row.upgrade}</small>
          <ScalingTokens scaling={row.effectiveScaling} extended={extendedScalingGrades} />
          <div className="lane-metrics">
            {objective !== "max_ar" ? <span>Metric <b>{metric === null ? "Unavailable" : fixed1(metric)}</b></span> : null}
            <span>AR <b>{fixed1(row.ar.total)}</b></span>
            {showAow ? <span>AoW <b>{hasAowDamage(row, aowSupported) ? compactNumber(row.aowFullSequenceDamage) : "Unavailable"}</b></span> : null}
          </div>
          <StatusTokens row={row} />
          {hasAowDamage(row, aowSupported) && row.aowRoute ? (
            <small>{row.aowRoute.routeLabel} / {fixed1(row.aowRoute.totalStaminaCost)} stamina / {row.aowRoute.actions.length} actions</small>
          ) : null}
          <small>{statLine(row)}</small>
        </>
      ) : (
        <em>{emptyLabel}</em>
      )}
    </div>
  );
}

function scrollMatrix(element: HTMLDivElement | null, direction: -1 | 1) {
  if (!element) {
    return;
  }
  element.scrollTo({
    left: direction < 0 ? 0 : element.scrollWidth,
    behavior: reducedMotion() ? "auto" : "smooth",
  });
}
