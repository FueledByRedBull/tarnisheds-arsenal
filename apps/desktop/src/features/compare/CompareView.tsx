import { useEffect, useRef, useState } from "react";
import { AowSelect } from "../../lib/AowSelect";
import { cachedSolveBuild, cachedUpgradeSeries } from "../../lib/analysis-cache";
import { compactNumber, fixed1, metricForObjective, objectiveLabel, statLine } from "../../lib/format";
import { CheckboxMultiSelect, SearchableSelect, openOption } from "../../lib/SearchableSelect";
import { compareUpgradeHorizon, rowFingerprint, upgradeCapForRow } from "../../lib/session";
import { stableSignature } from "../../lib/session";
import { LatestRequest } from "../../lib/request-generation";
import { useRequestBudget } from "../../lib/hooks";
import { useDesktopStore } from "../../lib/state";
import { ScalingDto, SolvedBuildDto, StableFilterEntryDto, UpgradePointDto } from "../../lib/types";
import { runSearchRequestForRows } from "../../lib/workflows";
import { ScalingTokens, StatusTokens } from "../shared/BuildMetricTokens";

type CompareLane = {
  label: string;
  row: SolvedBuildDto | null;
  points: UpgradePointDto[];
  scaling: ScalingDto | null;
  emptyLabel?: string;
};

export function CompareView() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const rows = useDesktopStore((state) => state.rows);
  const target = useDesktopStore((state) => state.compareTarget);
  const compareBench = useDesktopStore((state) => state.compareBench);
  const clearCompareBench = useDesktopStore((state) => state.clearCompareBench);
  const setCompareTarget = useDesktopStore((state) => state.setCompareTarget);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const compareControls = useDesktopStore((state) => state.compareControls);
  const patchCompareControls = useDesktopStore((state) => state.patchCompareControls);
  const setError = useDesktopStore((state) => state.setError);
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
  const matrixRef = useRef<HTMLDivElement | null>(null);
  const seriesRequest = useRef(new LatestRequest());
  const { base: baseRequest } = useRequestBudget(catalog, request, lockedStatMode);
  const [series, setSeries] = useState<CompareLane[]>([]);
  const [seriesStatus, setSeriesStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [seriesError, setSeriesError] = useState<string | null>(null);
  const typeDimension = catalog?.filterDimensions.find((dimension) => dimension.id === "weapon_type");
  const affinityDimension = catalog?.filterDimensions.find((dimension) => dimension.id === "affinity");
  const selectedTypeIds = compareControls.filters.entries
    .filter((entry) => entry.dimension === "weapon_type" && entry.mode === "include")
    .map((entry) => entry.id);
  const selectedAffinityIds = compareControls.filters.entries
    .filter((entry) => entry.dimension === "affinity" && entry.mode === "include")
    .map((entry) => entry.id);
  const excludedTypeIds = compareControls.filters.entries
    .filter((entry) => entry.dimension === "weapon_type" && entry.mode === "exclude")
    .map((entry) => entry.id);
  const excludedAffinityIds = compareControls.filters.entries
    .filter((entry) => entry.dimension === "affinity" && entry.mode === "exclude")
    .map((entry) => entry.id);
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
  const compareSource = customCompare
    ? [compareTargetLabel, selectedAffinityLabels.join(" + "), reinforcementLabel].filter(Boolean).join(" · ")
    : compareBench.length
      ? `${compareBench.length} pinned target${compareBench.length === 1 ? "" : "s"}`
      : "current ranked rivals";
  const pinnedMatchesSelected = Boolean(selected)
    && compareBench.length > 0
    && compareBench.every((row) => rowFingerprint(row) === rowFingerprint(selected));
  const emptyTargetLabel = pinnedMatchesSelected
    ? "The pinned target is already the selected baseline. Pin a different build or choose comparison filters."
    : "No other current ranked result. Run a broader Rankings search or choose comparison filters.";
  const extendedScalingGrades = catalog?.dataManifest.rules.extendedScalingGrades ?? false;

  useEffect(() => {
    const controller = new AbortController();
    const token = seriesRequest.current.begin(stableSignature({
      baseRequest,
      compareControls,
      request,
      rows,
      selected,
      compareBench,
    }));
    async function resolveRows() {
      if (!selected) {
        setSeries([]);
        setCompareTarget(null);
        setSeriesStatus("idle");
        return;
      }
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
          ? await runSearchRequestForRows({
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
          .slice(0, 5)
          .map((row, index) => ({ row, index }))
          .filter(({ row }) => rowFingerprint(row) !== rowFingerprint(selected))
          .slice(0, compareBench.length ? 5 : 3);
        const rivals = await Promise.all(rivalInputs.map(async ({ row, index }) => ({
          label: `${compareBench.length ? "Pinned" : "Top"} #${index + 1}`,
          row: compareBench.length
            ? await cachedSolveBuild(
              baseRequest,
              row.weaponName,
              row.affinity,
              row.aowName,
              controller.signal,
            )
            : row,
        })));
        lanes.push(...rivals);
        summaryTarget = rivals[0]?.row ?? null;
      }

      const nextSeries = await Promise.all(
        lanes.map(async (lane) => {
          if (!lane.row) {
            return { ...lane, points: [], scaling: null };
          }
          const row = lane.row;
          const points = await cachedUpgradeSeries(
            baseRequest,
            row,
            upgradeCapForRow(row, request),
            controller.signal,
          ).catch((error) => {
            throw new Error(
              `${lane.label} upgrade series at level ${baseRequest.characterLevel} (${statLine(row)}): ${error instanceof Error ? error.message : String(error)}`,
            );
          });
          return { ...lane, points, scaling: row.effectiveScaling ?? null };
        }),
      );
      if (seriesRequest.current.isCurrent(token)) {
        setCompareTarget(summaryTarget);
        setSeries(nextSeries);
        setSeriesStatus("ready");
      }
    }
    resolveRows().catch((error) => {
      if (seriesRequest.current.isCurrent(token)) {
        setSeries([]);
        const message = error instanceof Error ? error.message : String(error);
        setSeriesError(message);
        setSeriesStatus("error");
        setError(message);
      }
    });
    return () => {
      controller.abort();
      seriesRequest.current.invalidate(token);
    };
  }, [baseRequest, compareBench, compareControls, request, rows, selected, setCompareTarget, setError]);

  const matrixHorizon = compareUpgradeHorizon(request);
  const dataVersion = catalog
    ? `${catalog.dataManifest.datasetVersion} · model ${catalog.dataManifest.modelVersion}`
    : "data unavailable";

  if (!selected) {
    return (
      <section className="workspace-panel compare-panel">
        <div className="workspace-header"><div><h1>Compare</h1><span>Requires a current ranked build</span></div></div>
        <div className="empty-state workspace-prerequisite">
          <strong>Select a ranking first</strong>
          <span>Run or update Rankings, then select any row to use as the baseline.</span>
          <button type="button" onClick={() => setWorkspace("rankings")}>Go to Rankings</button>
        </div>
      </section>
    );
  }

  return (
    <section className="workspace-panel compare-panel">
      <div className="workspace-header analysis-workspace-header compare-workspace-header">
        <div className="workspace-heading-copy">
          <h1>Compare</h1>
          <span>Selected baseline versus {compareSource}</span>
          <small>Pinned loadouts keep their weapon, affinity, and skill; stats and upgrades are reoptimized for the current budget.</small>
          <small className="selected-summary">{selected.weaponName} / {selected.affinity} / +{selected.upgrade} · {objectiveLabel(request.objective)} · {dataVersion}</small>
        </div>
      </div>
      <div className="analysis-state" role="status" aria-live="polite">
        {seriesStatus === "loading" ? "Resolving builds, upgrade series, and scaling…" : null}
        {seriesStatus === "error" ? `Compare failed: ${seriesError}` : null}
        {seriesStatus === "ready" ? "Comparison current" : null}
      </div>
      <div className="compare-toolbar">
        {compareBench.length ? (
          <div className="compare-pins">
            <button type="button" className="clear-locks" onClick={clearCompareBench}>Clear {compareBench.length} pinned target{compareBench.length === 1 ? "" : "s"}</button>
            <small>Selected stays as the baseline.</small>
          </div>
        ) : null}
        <CheckboxMultiSelect
          label="Compare Type"
          values={selectedTypeIds}
          excludedValues={excludedTypeIds}
          options={typeDimension?.options.map((option) => ({ value: option.id, label: option.label, count: option.count })) ?? []}
          onChange={(values, excludedValues) => patchCompareControls({
            weaponName: null,
            aowName: null,
            matchSelectedAow: false,
            filters: { version: 1, entries: replaceCompareFilters(compareControls.filters.entries, "weapon_type", values, excludedValues) },
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
            filters: {
              version: 1,
              entries: replaceCompareFilters(
                replaceCompareFilters(compareControls.filters.entries, "weapon_type", [], []),
                "affinity",
                [],
                [],
              ),
            },
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
            filters: { version: 1, entries: replaceCompareFilters(compareControls.filters.entries, "affinity", values, excludedValues) },
          })}
        />
        <AowSelect
          label="Compare AoW"
          profileId={request.profileId}
          weaponName={compareControls.weaponName}
          affinity={aowAffinity}
          catalogNames={catalog?.aowNames}
          allowMatchSelected
          setError={setError}
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
      <div className="compare-lanes" aria-busy={seriesStatus === "loading"}>
        <Lane title="Selected" row={series[0]?.row ?? selected} objective={request.objective} scaling={series[0]?.scaling ?? null} extendedScalingGrades={extendedScalingGrades} emptyLabel="Selected build unavailable" />
        {series.length > 1 ? series.slice(1).map((lane) => (
          <Lane key={lane.label} title={lane.label} row={lane.row} objective={request.objective} scaling={lane.scaling} extendedScalingGrades={extendedScalingGrades} emptyLabel={lane.emptyLabel ?? "No compatible target"} />
        )) : <Lane title="Target" row={target} objective={request.objective} scaling={null} extendedScalingGrades={extendedScalingGrades} emptyLabel={seriesStatus === "loading" ? "Loading target…" : emptyTargetLabel} />}
      </div>
      <DeltaTable baseline={series[0]?.row ?? selected} candidates={series.slice(1)} objective={request.objective} />
      <div className="matrix-toolbar">
        <span>{compareSource}</span>
        <div>
          <button type="button" onClick={() => scrollMatrix(matrixRef.current, -1)}>+0</button>
          <button type="button" onClick={() => scrollMatrix(matrixRef.current, 1)}>+{matrixHorizon}</button>
        </div>
      </div>
      <div className="matrix-wrap" ref={matrixRef} aria-busy={seriesStatus === "loading"}>
        <div className="metric-matrix" role="grid" aria-label="Compare upgrade metrics">
          <div className="matrix-row matrix-header" role="row">
            <span role="columnheader">Line</span>
            {Array.from({ length: matrixHorizon + 1 }, (_, upgrade) => <span role="columnheader" key={upgrade}>+{upgrade}</span>)}
          </div>
          {series.map((lane) => (
            <MatrixRow key={lane.label} lane={lane} maxUpgrade={matrixHorizon} />
          ))}
        </div>
      </div>
    </section>
  );
}

function DeltaTable({ baseline, candidates, objective }: { baseline: SolvedBuildDto; candidates: CompareLane[]; objective: ReturnType<typeof useDesktopStore.getState>["request"]["objective"] }) {
  if (!candidates.length) return null;
  const metrics = [
    ["Objective", (row: SolvedBuildDto) => metricForObjective(row, objective)],
    ["AR", (row: SolvedBuildDto) => row.ar.total],
    ["Physical", (row: SolvedBuildDto) => row.ar.physical],
    ["Magic", (row: SolvedBuildDto) => row.ar.magic],
    ["Fire", (row: SolvedBuildDto) => row.ar.fire],
    ["Lightning", (row: SolvedBuildDto) => row.ar.lightning],
    ["Holy", (row: SolvedBuildDto) => row.ar.holy],
    ["Bleed", (row: SolvedBuildDto) => row.bleedBuildup],
    ["AoW first", (row: SolvedBuildDto) => row.aowFirstHitDamage],
    ["AoW full", (row: SolvedBuildDto) => row.aowFullSequenceDamage],
    ["Stamina", (row: SolvedBuildDto) => row.aowRoute?.totalStaminaCost ?? 0],
  ] as const;
  return (
    <div className="compare-deltas">
      <table>
        <caption>Deltas from {baseline.weaponName} / {baseline.affinity}</caption>
        <thead><tr><th>Build</th>{metrics.map(([label]) => <th key={label}>{label}</th>)}</tr></thead>
        <tbody>{candidates.map((lane) => lane.row ? (
          <tr key={lane.label}>
            <th>{lane.row.weaponName}<small>{lane.row.affinity}</small></th>
            {metrics.map(([label, value]) => {
              const delta = value(lane.row!) - value(baseline);
              return <td className={delta > 0 ? "positive" : delta < 0 ? "negative" : ""} key={label}>{delta > 0 ? "+" : ""}{fixed1(delta)}</td>;
            })}
          </tr>
        ) : null)}</tbody>
      </table>
      {candidates.map((lane) => lane.row ? <small key={`${lane.label}-explanation`}>{explainDelta(baseline, lane.row, objective)}</small> : null)}
    </div>
  );
}

function explainDelta(baseline: SolvedBuildDto, candidate: SolvedBuildDto, objective: ReturnType<typeof useDesktopStore.getState>["request"]["objective"]): string {
  const objectiveDelta = metricForObjective(candidate, objective) - metricForObjective(baseline, objective);
  const arDelta = candidate.ar.total - baseline.ar.total;
  const statDelta = ["strStat", "dex", "intStat", "fai", "arc"].reduce((sum, key) => sum + candidate.stats[key as keyof typeof candidate.stats] - baseline.stats[key as keyof typeof baseline.stats], 0);
  return `${candidate.weaponName}: ${objectiveDelta >= 0 ? "gains" : "loses"} ${fixed1(Math.abs(objectiveDelta))} objective value, ${arDelta >= 0 ? "gains" : "loses"} ${fixed1(Math.abs(arDelta))} AR, and uses ${statDelta >= 0 ? "+" : ""}${statDelta} combat-stat points versus the baseline.`;
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
    <div className="matrix-row" role="row">
      <strong role="rowheader">{lane.label}</strong>
      {Array.from({ length: maxUpgrade + 1 }, (_, upgrade) => (
        <span
          role="gridcell"
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
  scaling,
  extendedScalingGrades,
  emptyLabel,
}: {
  title: string;
  row: SolvedBuildDto | null;
  objective: ReturnType<typeof useDesktopStore.getState>["request"]["objective"];
  scaling: ScalingDto | null;
  extendedScalingGrades: boolean;
  emptyLabel: string;
}) {
  return (
    <div className="compare-lane">
      <span>{title}</span>
      {row ? (
        <>
          <strong>{row.weaponName}</strong>
          <small>{row.affinity} / {row.aowName ?? "Unspecified skill"} / +{row.upgrade}</small>
          <ScalingTokens scaling={scaling} extended={extendedScalingGrades} />
          <div className="lane-metrics">
            <span>Metric <b>{fixed1(metricForObjective(row, objective))}</b></span>
            <span>AR <b>{compactNumber(row.ar.total)}</b></span>
            <span>AoW <b>{compactNumber(row.aowFullSequenceDamage)}</b></span>
          </div>
          <StatusTokens row={row} />
          {row.aowRoute ? (
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
    behavior: "smooth",
  });
}

function replaceCompareFilters(
  entries: StableFilterEntryDto[],
  dimension: "weapon_type" | "affinity",
  ids: string[],
  excludedIds: string[],
): StableFilterEntryDto[] {
  return [
    ...entries.filter((entry) => entry.dimension !== dimension),
    ...ids.map((id) => ({ dimension, id, mode: "include" as const })),
    ...excludedIds.map((id) => ({ dimension, id, mode: "exclude" as const })),
  ];
}
