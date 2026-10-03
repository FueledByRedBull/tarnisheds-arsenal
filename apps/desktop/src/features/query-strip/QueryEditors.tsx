import { RotateCcw, Sparkles } from "lucide-react";
import { AowSelect } from "../../lib/AowSelect";
import { objectiveLabel, statLockLine } from "../../lib/format";
import { CheckboxMultiSelect, SearchableSelect, openOption } from "../../lib/SearchableSelect";
import {
  SCADUTREE_MAX_LEVEL,
  scadutreeAttackMultiplier,
  scadutreeDamageNegation,
  scadutreeReceivedDamageMultiplier,
} from "../../lib/scadutree";
import { classOptions, replaceFilterEntries } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { CatalogDto, FilterDimensionDto, ObjectiveId, OptimizeRequestDto, WeaponProfileDto } from "../../lib/types";
import { DraftNumberInput } from "./DraftNumberInput";
import { SearchRunner } from "./useSearchRunner";

type Patch = (patch: Partial<OptimizeRequestDto>) => void;

export function selectedFilterIds(
  dimension: FilterDimensionDto | undefined,
  entries: OptimizeRequestDto["filters"]["entries"],
  legacyLabel: string | null,
  mode: "include" | "exclude" = "include",
): string[] {
  const selected = entries
    .filter((entry) => entry.dimension === dimension?.id && entry.mode === mode)
    .map((entry) => entry.id);
  if (selected.length || !legacyLabel || mode === "exclude") return selected;
  const legacy = dimension?.options.find((option) => option.label === legacyLabel);
  return legacy ? [legacy.id] : [];
}

export function somberFilterLabel(value: string): string {
  return value
    .split("_")
    .map((word) => `${word.slice(0, 1).toUpperCase()}${word.slice(1)}`)
    .join(" ");
}

export function ClassEditor({ catalog, request, fixedStats, applyClass, onOptimize, onReset }: {
  catalog: CatalogDto | null;
  request: OptimizeRequestDto;
  fixedStats: boolean;
  applyClass: (className: string) => void;
  onOptimize: () => void;
  onReset: () => void;
}) {
  return (
    <div className="editor-body">
      <SearchableSelect
        label="Class"
        disabled={fixedStats}
        value={request.className}
        options={classOptions(catalog).map((entry) => ({ value: entry.name, label: entry.name }))}
        onChange={(value) => value && applyClass(value)}
      />
      <div className="editor-actions">
        <button type="button" onClick={onOptimize} disabled={fixedStats} title="Choose the lowest required level for all eight entered stats">
          <Sparkles size={14} aria-hidden="true" />
          Optimize class
        </button>
        <button
          type="button"
          onClick={onReset}
          title={fixedStats ? "Reset all eight stats to 1" : "Reset all eight stats to this class's base values"}
        >
          <RotateCcw size={14} aria-hidden="true" />
          Reset stats
        </button>
      </div>
      <p className="editor-note">
        {fixedStats
          ? "Convergence uses the entered stats exactly, so there is no starting class."
          : "Changing class resets all eight stats to its base values."}
      </p>
    </div>
  );
}

// One line on what each objective ranks by, so the choice does not depend on knowing the label.
const OBJECTIVE_HINTS: Record<ObjectiveId, string> = {
  max_ar: "Highest total attack rating",
  max_physical_ar: "Highest physical attack rating",
  max_ar_plus_bleed: "Most bleed buildup; AR breaks ties",
  aow_first_hit: "Hardest first damaging hit of the skill",
  aow_full_sequence: "Most damage over one full skill route",
};

export function ObjectiveEditor({ objectives, objective, patchRequest, close }: {
  objectives: ObjectiveId[];
  objective: ObjectiveId;
  patchRequest: Patch;
  close: () => void;
}) {
  return (
    <div className="editor-body">
      <div className="segmented editor-choices" role="group" aria-label="Ranking objective">
        {objectives.map((option) => (
          <button
            key={option}
            className={objective === option ? "active" : ""}
            type="button"
            aria-pressed={objective === option}
            aria-description={OBJECTIVE_HINTS[option]}
            onClick={() => {
              patchRequest({ objective: option });
              close();
            }}
          >
            {objectiveLabel(option)}
            <small aria-hidden="true">{OBJECTIVE_HINTS[option]}</small>
          </button>
        ))}
      </div>
    </div>
  );
}

export function LoadoutEditor({ catalog, request, patchRequest, runner, loadoutSelectionRevision, separateUpgradeCaps }: {
  catalog: CatalogDto | null;
  request: OptimizeRequestDto;
  patchRequest: Patch;
  runner: SearchRunner;
  loadoutSelectionRevision: number;
  separateUpgradeCaps: boolean;
}) {
  const typeDimension = catalog?.filterDimensions.find((dimension) => dimension.id === "weapon_type");
  const affinityDimension = catalog?.filterDimensions.find((dimension) => dimension.id === "affinity");
  const legacyTypeLabel = catalog?.weaponTypeOptions.find((entry) => entry.key === request.weaponTypeKey)?.label
    ?? request.weaponTypeKey;
  const selectedTypeIds = selectedFilterIds(typeDimension, request.filters.entries, legacyTypeLabel);
  const selectedAffinityIds = selectedFilterIds(affinityDimension, request.filters.entries, request.affinity);
  const excludedTypeIds = selectedFilterIds(typeDimension, request.filters.entries, null, "exclude");
  const excludedAffinityIds = selectedFilterIds(affinityDimension, request.filters.entries, null, "exclude");
  const selectedAffinityNames = affinityDimension?.options
    .filter((option) => selectedAffinityIds.includes(option.id))
    .map((option) => option.label) ?? [];
  const aowAffinity = selectedAffinityNames.length === 1 ? selectedAffinityNames[0] : request.affinity;
  const filtersActive = weaponFiltersActive(request);
  return (
    <div className="editor-body editor-grid">
      <CheckboxMultiSelect
        label="Weapon Type"
        values={selectedTypeIds}
        excludedValues={excludedTypeIds}
        options={typeDimension?.options.map((option) => ({ value: option.id, label: option.label, count: option.count })) ?? []}
        onChange={(values, excludedValues) => patchRequest({
          weaponTypeKey: null,
          weaponName: null,
          aowName: null,
          filters: { version: 1, entries: replaceFilterEntries(request.filters.entries, "weapon_type", values, excludedValues) },
        })}
      />
      <SearchableSelect
        label="Weapon"
        value={request.weaponName}
        options={[openOption(), ...(catalog?.weaponNames ?? []).map((name) => ({ value: name, label: name }))]}
        onChange={(weaponName) => {
          patchRequest(weaponPatch(request, weaponName));
          runner.markManualWeapon();
        }}
      />
      <CheckboxMultiSelect
        label="Affinity"
        values={selectedAffinityIds}
        excludedValues={excludedAffinityIds}
        options={affinityDimension?.options.map((option) => ({ value: option.id, label: option.label, count: option.count })) ?? []}
        onChange={(values, excludedValues) => patchRequest({
          affinity: null,
          aowName: null,
          filters: { version: 1, entries: replaceFilterEntries(request.filters.entries, "affinity", values, excludedValues) },
        })}
      />
      <AowSelect
        label="AoW"
        profileId={request.profileId}
        weaponName={request.weaponName}
        affinity={aowAffinity}
        catalogNames={catalog?.aowNames}
        value={request.aowName}
        onChange={(aowName) => patchRequest({ aowName })}
        defaultNativeSkill={loadoutSelectionRevision === runner.manualWeaponRevision.current}
        onWeaponResolved={() => { runner.manualWeaponRevision.current = null; }}
      />
      {separateUpgradeCaps ? (
        <SearchableSelect
          label="Somber"
          value={request.somberFilter}
          options={(catalog?.somberFilters ?? []).map((value) => ({ value, label: somberFilterLabel(value) }))}
          onChange={(somberFilter) => somberFilter && patchRequest({ somberFilter })}
        />
      ) : null}
      <button
        type="button"
        className="editor-reset"
        disabled={!filtersActive}
        onClick={() => patchRequest(resetWeaponFiltersPatch(request))}
      >
        <RotateCcw size={14} aria-hidden="true" />
        Reset weapon filters
      </button>
    </div>
  );
}

export function UpgradeEditor({ request, patchRequest, markResultsStale, separateUpgradeCaps, standardUpgradeLimit, somberUpgradeLimit, weaponProfile }: {
  request: OptimizeRequestDto;
  patchRequest: Patch;
  markResultsStale: () => void;
  separateUpgradeCaps: boolean;
  standardUpgradeLimit: number;
  somberUpgradeLimit: number;
  weaponProfile: WeaponProfileDto | null;
}) {
  return (
    <div className="editor-body">
      {separateUpgradeCaps ? (
        <div className="editor-pair">
          <label>
            Standard Upgrade
            <DraftNumberInput
              min={0}
              max={standardUpgradeLimit}
              value={request.standardMaxUpgrade}
              onDraftChange={markResultsStale}
              onCommit={(standardMaxUpgrade) => patchRequest({ standardMaxUpgrade })}
            />
          </label>
          <label>
            Somber Upgrade
            <DraftNumberInput
              min={0}
              max={somberUpgradeLimit}
              value={request.somberMaxUpgrade}
              onDraftChange={markResultsStale}
              onCommit={(somberMaxUpgrade) => patchRequest({ somberMaxUpgrade })}
            />
          </label>
        </div>
      ) : (
        <label>
          Weapon Upgrade
          <DraftNumberInput
            min={0}
            max={standardUpgradeLimit}
            value={request.standardMaxUpgrade}
            onDraftChange={markResultsStale}
            onCommit={(upgrade) => patchRequest({ standardMaxUpgrade: upgrade, somberMaxUpgrade: upgrade })}
          />
          <small>Convergence uses one +0 to +15 reinforcement path for every weapon.</small>
        </label>
      )}
      <div className="segmented" role="group" aria-label="Upgrade search policy">
        <button
          type="button"
          className={request.exactUpgrade ? "active" : ""}
          aria-pressed={request.exactUpgrade}
          onClick={() => patchRequest({ exactUpgrade: true })}
        >
          Use exact levels
        </button>
        <button
          type="button"
          className={!request.exactUpgrade ? "active" : ""}
          aria-pressed={!request.exactUpgrade}
          onClick={() => patchRequest({ exactUpgrade: false })}
        >
          Explore up to caps
        </button>
      </div>
      <div className="editor-readout">
        <span>
          {separateUpgradeCaps
            ? weaponProfile ? weaponProfile.isSomber ? "Selected Somber cap" : "Selected Standard cap" : "Profile upgrade limit"
            : "Selected Convergence cap"}
        </span>
        <strong>{request.weaponName && !weaponProfile ? "Unavailable" : `+${weaponProfile?.maxUpgrade ?? standardUpgradeLimit}`}</strong>
      </div>
      <p className="editor-note">
        {request.exactUpgrade
          ? "Only the entered reinforcement levels are ranked."
          : "Every reinforcement level from zero to each cap is eligible."}
      </p>
    </div>
  );
}

export function ScalingEditor({ request, patchRequest, markResultsStale }: {
  request: OptimizeRequestDto;
  patchRequest: Patch;
  markResultsStale: () => void;
}) {
  const attack = scadutreeAttackMultiplier(request.dlcScaling, request.scadutreeLevel);
  const taken = scadutreeReceivedDamageMultiplier(request.dlcScaling, request.scadutreeLevel);
  const negation = scadutreeDamageNegation(request.dlcScaling, request.scadutreeLevel);
  return (
    <div className="editor-body">
      <label className="toggle-line" title="Apply Shadow of the Erdtree Scadutree Blessing attack scaling">
        <input
          type="checkbox"
          checked={request.dlcScaling}
          onChange={(event) => patchRequest({ dlcScaling: event.target.checked })}
        />
        DLC Scaling
      </label>
      <label>
        Scadutree Level
        <DraftNumberInput
          min={0}
          max={SCADUTREE_MAX_LEVEL}
          value={request.scadutreeLevel}
          onDraftChange={markResultsStale}
          onCommit={(scadutreeLevel) => patchRequest({ scadutreeLevel })}
        />
      </label>
      <div className="editor-readout" title="Outgoing damage multiplier and equivalent incoming damage reduction from the selected blessing level">
        <span>{request.dlcScaling ? "Shadow Realm" : "DLC off"}</span>
        <strong>x{attack.toFixed(2)} dmg / x{taken.toFixed(3)} taken</strong>
        <small>{(negation * 100).toFixed(1)}% negation</small>
      </div>
    </div>
  );
}

export function ResultsEditor({ request, patchRequest, markResultsStale }: {
  request: OptimizeRequestDto;
  patchRequest: Patch;
  markResultsStale: () => void;
}) {
  return (
    <div className="editor-body">
      <label>
        Top Results
        <DraftNumberInput
          min={1}
          max={50}
          value={request.topK}
          onDraftChange={markResultsStale}
          onCommit={(topK) => patchRequest({ topK })}
        />
      </label>
      <div className="segmented" role="group" aria-label="Result grouping">
        {(["automatic", "weapon", "loadout"] as const).map((grouping) => (
          <button
            type="button"
            key={grouping}
            className={request.resultGrouping === grouping ? "active" : ""}
            aria-pressed={request.resultGrouping === grouping}
            onClick={() => patchRequest({ resultGrouping: grouping })}
          >
            {groupingLabel(grouping)}
          </button>
        ))}
      </div>
      <p className="editor-note">Per weapon keeps the best setup of each weapon; per loadout ranks every setup.</p>
    </div>
  );
}

export function LimitsEditor({ request, patchRequest, markResultsStale, fixedStats, lockedStatMode, exactLocksActive, onClearLocks }: {
  request: OptimizeRequestDto;
  patchRequest: Patch;
  markResultsStale: () => void;
  fixedStats: boolean;
  lockedStatMode: boolean;
  exactLocksActive: boolean;
  onClearLocks: () => void;
}) {
  const setLockedStatMode = useDesktopStore((state) => state.setLockedStatMode);
  const savedCoverageFilters = request.filters.entries.filter((entry) => entry.dimension === "coverage");
  return (
    <div className="editor-body">
      <p className="editor-note">
        {fixedStats
          ? "Convergence uses the entered combat stats exactly. Minimum floors do not redistribute stats."
          : "Optional minimums and exact result locks. Leave these open for automatic optimization."}
      </p>
      <div className="editor-floors">
        {([
          ["Min STR", "minStr"],
          ["Min DEX", "minDex"],
          ["Min INT", "minInt"],
          ["Min FAI", "minFai"],
          ["Min ARC", "minArc"],
        ] as const).map(([label, key]) => (
          <label key={key}>
            {label}
            <DraftNumberInput
              min={0}
              max={99}
              value={request[key]}
              onDraftChange={markResultsStale}
              onCommit={(value) => patchRequest({ [key]: value })}
            />
          </label>
        ))}
      </div>
      <label className="toggle-line" title="Use combat stats captured by Use As Locks">
        <input
          type="checkbox"
          checked={fixedStats || lockedStatMode}
          disabled={fixedStats}
          onChange={(event) => setLockedStatMode(event.target.checked)}
        />
        {fixedStats ? "Use entered combat stats exactly" : "Use stat locks"}
      </label>
      <div className="editor-readout">
        <span>Stat locks</span>
        <strong>
          {fixedStats
            ? `STR ${request.strStat} DEX ${request.dex} INT ${request.intStat} FAI ${request.fai} ARC ${request.arc}`
            : exactLocksActive ? statLockLine(request) : "Open"}
        </strong>
        {exactLocksActive ? <small>Changing class or loadout keeps these locks and may make the query incompatible.</small> : null}
      </div>
      <button
        className="clear-locks"
        type="button"
        disabled={fixedStats}
        onClick={onClearLocks}
      >
        Clear stat locks
      </button>
      {savedCoverageFilters.length > 0 ? (
        <section className="editor-section" aria-label="Saved profile filters">
          <strong>Saved profile filters</strong>
          <p className="editor-note">
            These saved filters apply to the whole profile, not individual weapons or skills.
            Excluding a supported capability excludes every result.
          </p>
          <ul>
            {savedCoverageFilters.map((entry) => (
              <li key={`${entry.mode}:${entry.id}`}>
                {entry.mode === "exclude" ? "Exclude" : "Include"} {entry.id.replace("coverage:", "").replaceAll("-", " ")}
              </li>
            ))}
          </ul>
          <button type="button" onClick={() => patchRequest({
            filters: { version: 1, entries: request.filters.entries.filter((entry) => entry.dimension !== "coverage") },
          })}>
            Remove saved profile filters
          </button>
        </section>
      ) : null}
    </div>
  );
}

// One readable line for the Loadout token: what is searched, then any narrowing.
export function loadoutSummary(catalog: CatalogDto | null, request: OptimizeRequestDto): string {
  const labels = (dimensionId: string, mode: "include" | "exclude") => {
    const dimension = catalog?.filterDimensions.find((entry) => entry.id === dimensionId);
    const ids = new Set(request.filters.entries
      .filter((entry) => entry.dimension === dimensionId && entry.mode === mode)
      .map((entry) => entry.id));
    return dimension?.options.filter((option) => ids.has(option.id)).map((option) => option.label) ?? [];
  };
  const types = labels("weapon_type", "include");
  const affinities = labels("affinity", "include");
  const excluded = labels("weapon_type", "exclude").length + labels("affinity", "exclude").length;
  const legacyType = catalog?.weaponTypeOptions.find((entry) => entry.key === request.weaponTypeKey)?.label ?? request.weaponTypeKey;
  const parts = [
    request.weaponName ?? (types.length ? types.join(" + ") : legacyType ?? "Any weapon"),
    request.affinity ?? (affinities.length === 1 ? affinities[0] : affinities.length ? `${affinities.length} affinities` : null),
    request.aowName,
    request.somberFilter !== "all" ? somberFilterLabel(request.somberFilter) : null,
    excluded ? `${excluded} excluded` : null,
  ];
  return parts.filter(Boolean).join(", ");
}

export function groupingLabel(grouping: OptimizeRequestDto["resultGrouping"]): string {
  return grouping === "automatic" ? "Auto" : grouping === "weapon" ? "Per weapon" : "Per loadout";
}

export function weaponFiltersActive(request: OptimizeRequestDto): boolean {
  return Boolean(
    request.weaponTypeKey
    || request.weaponName
    || request.affinity
    || request.aowName
    || request.somberFilter !== "all"
    || request.filters.entries.some((entry) => entry.dimension !== "coverage"),
  );
}

// Choosing a weapon clears type and affinity filters that could exclude it.
export function weaponPatch(request: OptimizeRequestDto, weaponName: string | null): Partial<OptimizeRequestDto> {
  return {
    weaponName,
    weaponTypeKey: null,
    affinity: null,
    aowName: null,
    filters: {
      version: 1,
      entries: replaceFilterEntries(
        replaceFilterEntries(request.filters.entries, "weapon_type", [], []),
        "affinity",
        [],
        [],
      ),
    },
  };
}

export function resetWeaponFiltersPatch(request: OptimizeRequestDto): Partial<OptimizeRequestDto> {
  return {
    weaponTypeKey: null,
    weaponName: null,
    affinity: null,
    aowName: null,
    somberFilter: "all",
    filters: {
      version: 1,
      entries: request.filters.entries.filter((entry) => entry.dimension === "coverage"),
    },
  };
}
