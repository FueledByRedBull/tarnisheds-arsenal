import { WheelEvent, useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { cachedSolveBuild, cachedUpgradeSeries, cachedWeaponScalingForUpgrade } from "../../lib/analysis-cache";
import { compactNumber, fixed1, metricForObjective, statLine } from "../../lib/format";
import { SearchableSelect, openOption } from "../../lib/SearchableSelect";
import { scalingLetter } from "../../lib/session";
import { useRequestBudget } from "../../lib/hooks";
import { useDesktopStore } from "../../lib/state";
import { ScalingDto, SolvedBuildDto, UpgradePointDto } from "../../lib/types";

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
  const matrixRef = useRef<HTMLDivElement | null>(null);
  const { base: baseRequest } = useRequestBudget(catalog, request, lockedStatMode);
  const [weaponNames, setWeaponNames] = useState<string[]>([]);
  const [affinityNames, setAffinityNames] = useState<string[]>([]);
  const [aowNames, setAowNames] = useState<string[]>([]);
  const [series, setSeries] = useState<CompareLane[]>([]);
  const typeOptions = catalog?.weaponTypeOptions.length
    ? catalog.weaponTypeOptions
    : catalog?.weaponTypeKeys.map((key) => ({ key, label: key })) ?? [];

  useEffect(() => {
    let cancelled = false;
    api.weaponNamesForType(compareControls.weaponTypeKey).then((names) => {
      if (!cancelled) setWeaponNames(names);
    }).catch((error) => setError(error instanceof Error ? error.message : String(error)));
    return () => {
      cancelled = true;
    };
  }, [compareControls.weaponTypeKey, setError]);

  useEffect(() => {
    let cancelled = false;
    async function loadAffinities() {
      const names = compareControls.weaponName
        ? await api.affinitiesForWeapon(compareControls.weaponName)
        : [];
      if (!cancelled) setAffinityNames(names);
    }
    loadAffinities().catch((error) => setError(error instanceof Error ? error.message : String(error)));
    return () => {
      cancelled = true;
    };
  }, [compareControls.weaponName, setError]);

  useEffect(() => {
    let cancelled = false;
    const selectedAow = compareControls.matchSelectedAow ? selected?.aowName ?? null : compareControls.aowName;
    async function loadAows() {
      const names = compareControls.weaponName
        ? await api.compatibleAowNames(compareControls.weaponName, compareControls.affinity)
        : compareControls.affinity
          ? await api.compatibleAowNamesForAffinity(compareControls.affinity)
          : catalog?.aowNames ?? [];
      if (!cancelled) setAowNames(selectedAow && !names.includes(selectedAow) ? [selectedAow, ...names] : names);
    }
    loadAows().catch((error) => setError(error instanceof Error ? error.message : String(error)));
    return () => {
      cancelled = true;
    };
  }, [catalog?.aowNames, compareControls.affinity, compareControls.matchSelectedAow, compareControls.aowName, compareControls.weaponName, selected?.aowName, setError]);

  useEffect(() => {
    let cancelled = false;
    async function resolveRows() {
      if (!selected) {
        setSeries([]);
        setCompareTarget(null);
        return;
      }
      const resolvedSelected =
        await cachedSolveBuild(baseRequest, selected.weaponName, selected.affinity, selected.aowName) ?? selected;
      const lanes: Array<{ label: string; row: SolvedBuildDto | null }> = [
        { label: "Selected", row: resolvedSelected },
      ];
      let summaryTarget: SolvedBuildDto | null = null;

      if (compareControls.weaponName) {
        const compareAow = compareControls.matchSelectedAow ? resolvedSelected.aowName : compareControls.aowName;
        const compareRow = await cachedSolveBuild(baseRequest, compareControls.weaponName, compareControls.affinity, compareAow);
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
            row: await cachedSolveBuild(baseRequest, row.weaponName, row.affinity, row.aowName) ?? row,
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
          const [points, scaling] = await Promise.all([
            cachedUpgradeSeries(baseRequest, lane.row, request.maxUpgrade),
            cachedWeaponScalingForUpgrade(lane.row.weaponName, lane.row.affinity, lane.row.upgrade),
          ]);
          return { ...lane, points, scaling };
        }),
      );
      if (!cancelled) {
        setCompareTarget(summaryTarget);
        setSeries(nextSeries);
      }
    }
    resolveRows().catch((error) => {
      if (!cancelled) {
        setSeries([]);
        setError(error instanceof Error ? error.message : String(error));
      }
    });
    return () => {
      cancelled = true;
    };
  }, [baseRequest, compareControls, request.maxUpgrade, rows, selected, setCompareTarget, setError]);

  return (
    <section className="workspace-panel compare-panel">
      <div className="workspace-header">
        <div>
          <h1>Compare</h1>
          <span>Selected line, explicit target, or top ranked rivals</span>
        </div>
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
      <div className="compare-lanes">
        <Lane title="Selected" row={series[0]?.row ?? selected} objective={request.objective} scaling={series[0]?.scaling ?? null} />
        <Lane title="Target" row={target} objective={request.objective} scaling={series[1]?.scaling ?? null} />
      </div>
      <div className="matrix-toolbar">
        <span>{compareControls.weaponName ? "Explicit target" : "Top ranked rivals"}</span>
        <div>
          <button type="button" onClick={() => scrollMatrix(matrixRef.current, -1)}>+0</button>
          <button type="button" onClick={() => scrollMatrix(matrixRef.current, 1)}>+{request.maxUpgrade}</button>
        </div>
      </div>
      <div className="matrix-wrap" ref={matrixRef} onWheel={scrollMatrixWithWheel}>
        <div className="metric-matrix" role="grid" aria-label="Compare upgrade metrics">
          <span>Line</span>
          {Array.from({ length: request.maxUpgrade + 1 }, (_, upgrade) => <span key={upgrade}>+{upgrade}</span>)}
          {series.map((lane) => (
            <MatrixRow key={lane.label} lane={lane} maxUpgrade={request.maxUpgrade} />
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
    <>
      <strong>{lane.label}</strong>
      {Array.from({ length: maxUpgrade + 1 }, (_, upgrade) => (
        <span key={`${lane.label}-${upgrade}`}>{fixed1(byUpgrade.get(upgrade))}</span>
      ))}
    </>
  );
}

function Lane({
  title,
  row,
  objective,
  scaling,
}: {
  title: string;
  row: SolvedBuildDto | null;
  objective: ReturnType<typeof useDesktopStore.getState>["request"]["objective"];
  scaling: ScalingDto | null;
}) {
  return (
    <div className="compare-lane">
      <span>{title}</span>
      {row ? (
        <>
          <strong>{row.weaponName}</strong>
          <small>{row.affinity} / {row.aowName ?? "Native"} / +{row.upgrade}</small>
          <div className="scaling-strip">
            {formatScaling(scaling).map(([label, value]) => (
              <span key={label}>{label} <b>{value}</b></span>
            ))}
          </div>
          <div className="lane-metrics">
            <span>Metric <b>{fixed1(metricForObjective(row, objective))}</b></span>
            <span>AR <b>{compactNumber(row.ar.total)}</b></span>
            <span>Bleed <b>{compactNumber(row.bleedBuildup)}</b></span>
            <span>AoW <b>{compactNumber(row.aowFullSequenceDamage)}</b></span>
          </div>
          <small>{statLine(row)}</small>
        </>
      ) : (
        <em>Unset</em>
      )}
    </div>
  );
}

function formatScaling(scaling: ScalingDto | null): Array<[string, string]> {
  if (!scaling) {
    return [["STR", "-"], ["DEX", "-"], ["INT", "-"], ["FAI", "-"], ["ARC", "-"]];
  }
  return [
    ["STR", formatScalingValue(scaling.str)],
    ["DEX", formatScalingValue(scaling.dex)],
    ["INT", formatScalingValue(scaling.int)],
    ["FAI", formatScalingValue(scaling.fai)],
    ["ARC", formatScalingValue(scaling.arc)],
  ];
}

function formatScalingValue(value: number): string {
  if (value <= 0) {
    return "-";
  }
  const rounded = Math.round((value + Number.EPSILON) * 100) / 100;
  return `${scalingLetter(rounded)} (${rounded.toFixed(2)})`;
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
