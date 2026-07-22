import { WheelEvent, useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { cachedSolveBuild, cachedUpgradeSeries } from "../../lib/analysis-cache";
import { compactNumber, fixed1, metricForObjective, objectiveLabel, statLine } from "../../lib/format";
import { SearchableSelect, openOption } from "../../lib/SearchableSelect";
import { compareUpgradeHorizon, upgradeCapForRow } from "../../lib/session";
import { stableSignature } from "../../lib/session";
import { LatestRequest } from "../../lib/request-generation";
import { useRequestBudget } from "../../lib/hooks";
import { useDesktopStore } from "../../lib/state";
import { ScalingDto, SolvedBuildDto, UpgradePointDto } from "../../lib/types";
import { ScalingTokens, StatusTokens } from "../shared/BuildMetricTokens";

type CompareLane = {
  label: string;
  row: SolvedBuildDto | null;
  points: UpgradePointDto[];
  scaling: ScalingDto | null;
};

export function CompareView() {
  const catalog = useDesktopStore((state) => state.catalog);
  const selected = useDesktopStore((state) => state.selected);
  const rows = useDesktopStore((state) => state.rows);
  const target = useDesktopStore((state) => state.compareTarget);
  const setCompareTarget = useDesktopStore((state) => state.setCompareTarget);
  const request = useDesktopStore((state) => state.request);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const compareControls = useDesktopStore((state) => state.compareControls);
  const patchCompareControls = useDesktopStore((state) => state.patchCompareControls);
  const setError = useDesktopStore((state) => state.setError);
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
  const matrixRef = useRef<HTMLDivElement | null>(null);
  const weaponNamesRequest = useRef(new LatestRequest());
  const affinitiesRequest = useRef(new LatestRequest());
  const aowsRequest = useRef(new LatestRequest());
  const seriesRequest = useRef(new LatestRequest());
  const { base: baseRequest } = useRequestBudget(catalog, request, lockedStatMode);
  const [weaponNames, setWeaponNames] = useState<string[]>([]);
  const [affinityNames, setAffinityNames] = useState<string[]>([]);
  const [aowNames, setAowNames] = useState<string[]>([]);
  const [series, setSeries] = useState<CompareLane[]>([]);
  const [seriesStatus, setSeriesStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [seriesError, setSeriesError] = useState<string | null>(null);
  const typeOptions = catalog?.weaponTypeOptions.length
    ? catalog.weaponTypeOptions
    : catalog?.weaponTypeKeys.map((key) => ({ key, label: key })) ?? [];
  const extendedScalingGrades = catalog?.dataManifest.rules.extendedScalingGrades ?? false;

  useEffect(() => {
    const token = weaponNamesRequest.current.begin(stableSignature({
      weaponTypeKey: compareControls.weaponTypeKey,
    }));
    api.weaponNamesForType(request.profileId, compareControls.weaponTypeKey).then((names) => {
      if (weaponNamesRequest.current.isCurrent(token)) setWeaponNames(names);
    }).catch((error) => {
      if (weaponNamesRequest.current.isCurrent(token)) {
        setError(error instanceof Error ? error.message : String(error));
      }
    });
    return () => {
      weaponNamesRequest.current.invalidate(token);
    };
  }, [compareControls.weaponTypeKey, request.profileId, setError]);

  useEffect(() => {
    const token = affinitiesRequest.current.begin(stableSignature({
      weaponName: compareControls.weaponName,
    }));
    async function loadAffinities() {
      const names = compareControls.weaponName
        ? await api.affinitiesForWeapon(request.profileId, compareControls.weaponName)
        : [];
      if (affinitiesRequest.current.isCurrent(token)) setAffinityNames(names);
    }
    loadAffinities().catch((error) => {
      if (affinitiesRequest.current.isCurrent(token)) {
        setError(error instanceof Error ? error.message : String(error));
      }
    });
    return () => {
      affinitiesRequest.current.invalidate(token);
    };
  }, [compareControls.weaponName, request.profileId, setError]);

  useEffect(() => {
    const selectedAow = compareControls.matchSelectedAow ? selected?.aowName ?? null : compareControls.aowName;
    const token = aowsRequest.current.begin(stableSignature({
      weaponName: compareControls.weaponName,
      affinity: compareControls.affinity,
      selectedAow,
    }));
    async function loadAows() {
      const names = compareControls.weaponName
        ? await api.compatibleAowNames(request.profileId, compareControls.weaponName, compareControls.affinity)
        : compareControls.affinity
          ? await api.compatibleAowNamesForAffinity(request.profileId, compareControls.affinity)
          : catalog?.aowNames ?? [];
      if (aowsRequest.current.isCurrent(token)) {
        setAowNames(selectedAow && !names.includes(selectedAow) ? [selectedAow, ...names] : names);
      }
    }
    loadAows().catch((error) => {
      if (aowsRequest.current.isCurrent(token)) {
        setError(error instanceof Error ? error.message : String(error));
      }
    });
    return () => {
      aowsRequest.current.invalidate(token);
    };
  }, [catalog?.aowNames, compareControls.affinity, compareControls.matchSelectedAow, compareControls.aowName, compareControls.weaponName, request.profileId, selected?.aowName, setError]);

  useEffect(() => {
    const controller = new AbortController();
    const token = seriesRequest.current.begin(stableSignature({
      baseRequest,
      compareControls,
      request,
      rows,
      selected,
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
      const resolvedSelected =
        await cachedSolveBuild(
          baseRequest,
          selected.weaponName,
          selected.affinity,
          selected.aowName,
          controller.signal,
        ) ?? selected;
      const lanes: Array<{ label: string; row: SolvedBuildDto | null }> = [
        { label: "Selected", row: resolvedSelected },
      ];
      let summaryTarget: SolvedBuildDto | null = null;

      if (compareControls.weaponName) {
        const compareAow = compareControls.matchSelectedAow ? resolvedSelected.aowName : compareControls.aowName;
        const compareRow = await cachedSolveBuild(
          baseRequest,
          compareControls.weaponName,
          compareControls.affinity,
          compareAow,
          controller.signal,
        );
        lanes.push({ label: "Compare", row: compareRow });
        summaryTarget = compareRow;
      } else {
        const rivalInputs = rows
          .slice(0, 5)
          .map((row, index) => ({ row, index }))
          .filter(({ row }) => row !== selected)
          .slice(0, 3);
        const rivals = await Promise.all(
          rivalInputs.map(async ({ row, index }) => ({
            label: `Top #${index + 1}`,
            row: await cachedSolveBuild(
              baseRequest,
              row.weaponName,
              row.affinity,
              row.aowName,
              controller.signal,
            ) ?? row,
          })),
        );
        lanes.push(...rivals);
        summaryTarget = rivals[0]?.row ?? null;
      }

      const nextSeries = await Promise.all(
        lanes.map(async (lane) => {
          if (!lane.row) {
            return { ...lane, points: [], scaling: null };
          }
          const points = await cachedUpgradeSeries(
            baseRequest,
            lane.row,
            upgradeCapForRow(lane.row, request),
            controller.signal,
          );
          return { ...lane, points, scaling: lane.row.effectiveScaling ?? null };
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
  }, [baseRequest, compareControls, request, rows, selected, setCompareTarget, setError]);

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
      <div className="workspace-header">
        <div>
          <h1>Compare</h1>
          <span>Selected line, explicit target, or top ranked rivals</span>
          <small className="selected-summary">{selected.weaponName} / {selected.affinity} / +{selected.upgrade} · {objectiveLabel(request.objective)} · {dataVersion}</small>
        </div>
      </div>
      <div className="analysis-state" role="status" aria-live="polite">
        {seriesStatus === "loading" ? "Resolving builds, upgrade series, and scaling…" : null}
        {seriesStatus === "error" ? `Compare failed: ${seriesError}` : null}
        {seriesStatus === "ready" ? "Comparison current" : null}
      </div>
      <div className="compare-toolbar">
        <SearchableSelect
          label="Compare Type"
          value={compareControls.weaponTypeKey}
          options={[openOption("All"), ...typeOptions.map((entry) => ({ value: entry.key, label: entry.label }))]}
          onChange={(weaponTypeKey) => patchCompareControls({ weaponTypeKey, weaponName: null, affinity: null, aowName: null })}
        />
        <SearchableSelect
          label="Compare Weapon"
          value={compareControls.weaponName}
          options={[openOption("Top ranked rivals"), ...weaponNames.map((name) => ({ value: name, label: name }))]}
          onChange={(weaponName) => patchCompareControls({ weaponName, affinity: null, aowName: null })}
        />
        <SearchableSelect
          label="Compare Affinity"
          value={compareControls.affinity}
          options={[openOption(), ...affinitiesForTarget(affinityNames, target, selected).map((name) => ({ value: name, label: name }))]}
          onChange={(affinity) => patchCompareControls({ affinity, aowName: null })}
        />
        <SearchableSelect
          label="Compare AoW"
          value={compareControls.matchSelectedAow ? "__match_selected__" : compareControls.aowName}
          options={[
            { value: "__match_selected__", label: "<Match Selected>" },
            openOption(),
            ...(aowNames ?? []).map((name) => ({ value: name, label: name })),
          ]}
          onChange={(value) =>
            patchCompareControls(
              value === "__match_selected__"
                ? { matchSelectedAow: true, aowName: null }
                : { matchSelectedAow: false, aowName: value },
            )
          }
        />
      </div>
      <div className="compare-lanes" aria-busy={seriesStatus === "loading"}>
        <Lane title="Selected" row={series[0]?.row ?? selected} objective={request.objective} scaling={series[0]?.scaling ?? null} extendedScalingGrades={extendedScalingGrades} emptyLabel="Selected build unavailable" />
        <Lane
          title="Target"
          row={target}
          objective={request.objective}
          scaling={series[1]?.scaling ?? null}
          extendedScalingGrades={extendedScalingGrades}
          emptyLabel={seriesStatus === "loading" ? "Loading target…" : compareControls.weaponName ? "No compatible target" : "No ranked rival available"}
        />
      </div>
      <div className="matrix-toolbar">
        <span>{compareControls.weaponName ? "Explicit target" : "Top ranked rivals"}</span>
        <div>
          <button type="button" onClick={() => scrollMatrix(matrixRef.current, -1)}>+0</button>
          <button type="button" onClick={() => scrollMatrix(matrixRef.current, 1)}>+{matrixHorizon}</button>
        </div>
      </div>
      <div className="matrix-wrap" ref={matrixRef} onWheel={scrollMatrixWithWheel} aria-busy={seriesStatus === "loading"}>
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
          <small>{row.affinity} / {row.aowName ?? "Native"} / +{row.upgrade}</small>
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

function scrollMatrixWithWheel(event: WheelEvent<HTMLDivElement>) {
  if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) {
    return;
  }
  event.currentTarget.scrollLeft += event.deltaY;
}

function affinitiesForTarget(backendAffinities: string[], target: SolvedBuildDto | null, selected: SolvedBuildDto | null): string[] {
  const values = new Set(backendAffinities);
  if (target) values.add(target.affinity);
  if (selected) values.add(selected.affinity);
  return Array.from(values).sort((left, right) => left.localeCompare(right));
}
