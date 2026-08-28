import { Crosshair, Filter, Play, RotateCcw, SlidersHorizontal, Swords } from "lucide-react";
import { KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../lib/api";
import { useRequestBudget, useSearchJob, useWeaponProfile } from "../../lib/hooks";
import { fixed1, objectiveLabel } from "../../lib/format";
import { SearchableSelect, openOption } from "../../lib/SearchableSelect";
import {
  SCADUTREE_MAX_LEVEL,
  scadutreeAttackMultiplier,
  scadutreeDamageNegation,
  scadutreeReceivedDamageMultiplier,
} from "../../lib/scadutree";
import { classMeta, classOptions, derivedLevel } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { FilterDimensionDto, OptimizeRequestDto, SearchFinishedDto, SearchProgressDto } from "../../lib/types";
import { runSearchFromStore } from "../../lib/workflows";

export function CommandRail() {
  const catalog = useDesktopStore((state) => state.catalog);
  const request = useDesktopStore((state) => state.request);
  const patchRequest = useDesktopStore((state) => state.patchRequest);
  const applyClass = useDesktopStore((state) => state.applyClass);
  const markResultsStale = useDesktopStore((state) => state.markResultsStale);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const setError = useDesktopStore((state) => state.setError);
  const setSearching = useDesktopStore((state) => state.setSearching);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const searchGeneration = useDesktopStore((state) => state.searchGeneration);
  const activeJobId = useDesktopStore((state) => state.activeJobId);
  const progress = useDesktopStore((state) => state.progress);
  const setActiveJobId = useDesktopStore((state) => state.setActiveJobId);
  const setProgress = useDesktopStore((state) => state.setProgress);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const setLockedStatMode = useDesktopStore((state) => state.setLockedStatMode);
  const [weaponNames, setWeaponNames] = useState<string[]>([]);
  const [aowNames, setAowNames] = useState<string[]>([]);
  const [searchStartedAt, setSearchStartedAt] = useState<number | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [searchCancellationRequested, setSearchCancellationRequested] = useState(false);
  const searchCancellationRequestedRef = useRef(false);
  const meta = classMeta(catalog, request.className);
  const { base: apiRequest, budget } = useRequestBudget(catalog, request, lockedStatMode);
  const weaponProfile = useWeaponProfile(request, patchRequest, setError);
  const effectiveStr =
    request.twoHanding && !weaponProfile?.disablesTwoHandBonus
      ? Math.min(99, Math.floor(request.strStat * 1.5))
      : request.strStat;
  const scadutreeDamageMultiplier = scadutreeAttackMultiplier(request.dlcScaling, request.scadutreeLevel);
  const scadutreeTakenMultiplier = scadutreeReceivedDamageMultiplier(request.dlcScaling, request.scadutreeLevel);
  const scadutreeNegation = scadutreeDamageNegation(request.dlcScaling, request.scadutreeLevel);
  const requirementGaps = useMemo(() => {
    const requirements = weaponProfile?.requirements;
    if (!requirements) {
      return null;
    }
    return {
      strStat: Math.max(requirements.strStat - effectiveStr, 0),
      dex: Math.max(requirements.dex - request.dex, 0),
      intStat: Math.max(requirements.intStat - request.intStat, 0),
      fai: Math.max(requirements.fai - request.fai, 0),
      arc: Math.max(requirements.arc - request.arc, 0),
    };
  }, [effectiveStr, request.arc, request.dex, request.fai, request.intStat, weaponProfile]);
  const missingRequirements = requirementGaps
    ? Object.values(requirementGaps).some((gap) => gap > 0)
    : false;
  const affinityOptions = weaponProfile?.affinities.length ? weaponProfile.affinities : catalog?.affinityNames ?? [];
  const typeOptions = catalog?.weaponTypeOptions.length
    ? catalog.weaponTypeOptions
    : catalog?.weaponTypeKeys.map((key) => ({ key, label: key })) ?? [];
  const startingClasses = classOptions(catalog);
  const activeMinimums = [request.minStr, request.minDex, request.minInt, request.minFai, request.minArc]
    .filter((value) => value > 0).length;
  const exactLocksActive = lockedStatMode && request.lockStr !== null;
  const profileRules = catalog?.dataManifest.rules;
  const separateUpgradeCaps = profileRules?.separateUpgradeCaps ?? true;
  const scadutreeAvailable = profileRules?.scadutreeScaling ?? true;
  const standardUpgradeLimit = profileRules?.standardMaxUpgrade ?? 25;
  const somberUpgradeLimit = profileRules?.somberMaxUpgrade ?? 10;
  const upgradeSummary = separateUpgradeCaps
    ? `${request.exactUpgrade ? "Exact" : "Up to"} +${request.standardMaxUpgrade} / +${request.somberMaxUpgrade}`
    : `${request.exactUpgrade ? "Exact" : "Up to"} +${request.standardMaxUpgrade}`;

  function setBudgetMode(mode: OptimizeRequestDto["budgetMode"]) {
    if (mode === request.budgetMode) return;
    if (mode === "target_level") {
      patchRequest({ budgetMode: mode, characterLevel: derivedLevel(catalog, request) });
      return;
    }
    const fixedUps = Math.max(0, request.vig - meta.baseStats.vig)
      + Math.max(0, request.mnd - meta.baseStats.mnd)
      + Math.max(0, request.end - meta.baseStats.end);
    patchRequest({
      budgetMode: mode,
      offensivePointBudget: Math.max(0, derivedLevel(catalog, request) - meta.baseLevel - fixedUps),
    });
  }

  useEffect(() => {
    let cancelled = false;
    api.weaponNamesForType(request.profileId, request.weaponTypeKey).then((names) => {
      if (!cancelled) setWeaponNames(names);
    }).catch((error) => setError(error instanceof Error ? error.message : String(error)));
    return () => {
      cancelled = true;
    };
  }, [request.profileId, request.weaponTypeKey, setError]);

  useEffect(() => {
    let cancelled = false;
    async function loadAows() {
      const names = request.weaponName
        ? await api.compatibleAowNames(request.profileId, request.weaponName, request.affinity)
        : request.affinity
          ? await api.compatibleAowNamesForAffinity(request.profileId, request.affinity)
          : catalog?.aowNames ?? [];
      if (!cancelled) setAowNames(names);
    }
    loadAows().catch((error) => setError(error instanceof Error ? error.message : String(error)));
    return () => {
      cancelled = true;
    };
  }, [catalog?.aowNames, request.affinity, request.profileId, request.weaponName, setError]);

  useSearchJob({
    activeJobId,
    isSearching,
    generation: searchGeneration,
    setProgress,
    finish: finishSearch,
  });

  useEffect(() => {
    if (!searchStartedAt || !isSearching) {
      setElapsedMs(0);
      return undefined;
    }
    const tick = window.setInterval(() => {
      setElapsedMs(Date.now() - searchStartedAt);
    }, 200);
    return () => window.clearInterval(tick);
  }, [isSearching, searchStartedAt]);

  async function runSearch() {
    searchCancellationRequestedRef.current = false;
    setSearchCancellationRequested(false);
    setSearchStartedAt(Date.now());
    setError(null);
    setProgress(null);
    const started = await runSearchFromStore(apiRequest, () => searchCancellationRequestedRef.current);
    if (!started) {
      setSearchStartedAt(null);
      setSearchCancellationRequested(false);
      searchCancellationRequestedRef.current = false;
    }
  }

  function finishSearch(payload: SearchFinishedDto, generation: number) {
    const current = useDesktopStore.getState();
    if (
      generation !== current.searchGeneration ||
      !current.activeSearchSignature ||
      payload.jobId !== current.activeJobId
    ) return;
    if (payload.error) current.setError(payload.error);
    if (payload.cancelled) {
      current.pushNotice({
        scope: "rankings",
        tone: "warning",
        message: "Search stopped. Previous results were retained.",
      });
    }
    else current.setRows(payload.rows);
    current.setSearching(false);
    current.setActiveJobId(null);
    current.setProgress(null);
    setSearchStartedAt(null);
    setSearchCancellationRequested(false);
    searchCancellationRequestedRef.current = false;
  }

  async function cancelSearch() {
    if (searchCancellationRequested) return;
    searchCancellationRequestedRef.current = true;
    setSearchCancellationRequested(true);
    const currentJobId = useDesktopStore.getState().activeJobId;
    if (!currentJobId) return;
    try {
      const cancelled = await api.cancelSearch(currentJobId);
      if (!cancelled) {
        setSearching(false);
        setActiveJobId(null);
        setProgress(null);
        setSearchStartedAt(null);
        setSearchCancellationRequested(false);
        searchCancellationRequestedRef.current = false;
      }
    } catch (error) {
      setSearchCancellationRequested(false);
      searchCancellationRequestedRef.current = false;
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <aside className="command-rail">
      <div className="brand-block">
        <span className="brand-kicker">Tarnished's</span>
        <strong>Arsenal</strong>
        <small className="data-version">{catalog?.dataManifest.label ?? "Loading data version"}</small>
      </div>

      <fieldset className="rail-scroll rail-fieldset" disabled={!catalog} aria-busy={!catalog}>
        <section className="rail-section">
          <div className="section-title">
            <Swords size={15} />
            <span>Character</span>
          </div>
          <SearchableSelect
            label="Class"
            value={request.className}
            options={startingClasses.map((entry) => ({ value: entry.name, label: entry.name }))}
            onChange={(value) => value && applyClass(value)}
          />
          <div className="segmented" aria-label="Level budget mode">
            <button type="button" className={request.budgetMode === "target_level" ? "active" : ""} onClick={() => setBudgetMode("target_level")}>Target level</button>
            <button type="button" className={request.budgetMode === "offensive_points" ? "active" : ""} onClick={() => setBudgetMode("offensive_points")}>Offensive points</button>
          </div>
          <div className="level-strip">
            <label>
              {request.budgetMode === "target_level" ? "Level" : "Offensive Points"}
              <DraftNumberInput
                min={0}
                max={request.budgetMode === "target_level" ? 713 : 712}
                value={request.budgetMode === "target_level" ? request.characterLevel : request.offensivePointBudget}
                onDraftChange={markResultsStale}
                onCommit={(value) => patchRequest(request.budgetMode === "target_level"
                  ? { characterLevel: Math.max(meta.baseLevel, value) }
                  : { offensivePointBudget: value })}
              />
            </label>
            <div className="budget-readout" title={`Levels above ${request.className}'s base level ${budget.baseLevel}`}>
              <span>Lv Ups</span>
              <strong>{budget.levelUps}</strong>
            </div>
            <div className="budget-readout" title="Movable STR/DEX/INT/FAI/ARC points after class minimums, fixed VIG/MND/END, and advanced minimum floors">
              <span>Redistrib</span>
              <strong>{budget.redistributable}</strong>
            </div>
          </div>
          <div className="stat-grid">
            {[
              ["VIG", "vig", meta.baseStats.vig],
              ["MND", "mnd", meta.baseStats.mnd],
              ["END", "end", meta.baseStats.end],
              ["STR", "strStat", meta.baseStats.strStat],
              ["DEX", "dex", meta.baseStats.dex],
              ["INT", "intStat", meta.baseStats.intStat],
              ["FAI", "fai", meta.baseStats.fai],
              ["ARC", "arc", meta.baseStats.arc],
            ].map(([label, key, min]) => (
              <label
                key={key}
                className={statIsShort(String(key), requirementGaps) ? "stat-short" : undefined}
              >
                {label}
                <DraftNumberInput
                  min={Number(min)}
                  max={99}
                  value={Number(request[key as keyof typeof request])}
                  onDraftChange={markResultsStale}
                  onCommit={(value) => patchRequest({ [key]: value } as Partial<OptimizeRequestDto>)}
                />
              </label>
            ))}
          </div>
          <div className="hero-chip-row">
            <span>{request.twoHanding ? "Two-handed strength" : "One-handed strength"}</span>
            <span>{lockedStatMode ? "Exact combat stats" : "Stats optimized"}</span>
            <span>{upgradeSummary}</span>
            <span>
              {scadutreeAvailable
                ? request.dlcScaling
                  ? `DLC blessing x${scadutreeDamageMultiplier.toFixed(2)}`
                  : "Base-game scaling"
                : "No Scadutree scaling"}
            </span>
            {activeMinimums > 0 ? <span>{activeMinimums} minimum stat floor{activeMinimums === 1 ? "" : "s"}</span> : null}
          </div>
          {exactLocksActive ? (
            <div className="active-lock-warning" role="status">
              <strong>Exact locks active:</strong>
              <span>STR {request.lockStr} / DEX {request.lockDex} / INT {request.lockInt} / FAI {request.lockFai} / ARC {request.lockArc}</span>
              <small>Changing class or loadout keeps these locks and may make the query incompatible. Clear locks in Fine tuning when you want automatic stats.</small>
            </div>
          ) : null}
          {weaponProfile ? (
            <div className={`requirements-strip ${missingRequirements ? "missing" : ""}`}>
              <span>{missingRequirements ? "Requirements unmet" : "Requirements clear"}</span>
              <strong>
                STR {weaponProfile.requirements.strStat} / DEX {weaponProfile.requirements.dex} /
                INT {weaponProfile.requirements.intStat} / FAI {weaponProfile.requirements.fai} /
                ARC {weaponProfile.requirements.arc}
              </strong>
            </div>
          ) : null}
        </section>

        <section className="rail-section">
          <div className="section-title">
            <Filter size={15} />
            <span>Loadout</span>
          </div>
          <SearchableSelect
            label="Weapon Type"
            value={request.weaponTypeKey}
            options={[openOption(), ...typeOptions.map((entry) => ({ value: entry.key, label: entry.label }))]}
            onChange={(weaponTypeKey) => patchRequest({ weaponTypeKey, weaponName: null, affinity: null, aowName: null })}
          />
          <SearchableSelect
            label="Weapon"
            value={request.weaponName}
            options={[openOption(), ...weaponNames.map((name) => ({ value: name, label: name }))]}
            onChange={(weaponName) => patchRequest({ weaponName, affinity: null, aowName: null })}
          />
          <SearchableSelect
            label="Affinity"
            value={request.affinity}
            options={[openOption(), ...affinityOptions.map((name) => ({ value: name, label: name }))]}
            onChange={(affinity) => patchRequest({ affinity, aowName: null })}
          />
          <SearchableSelect
            label="AoW"
            value={request.aowName}
            options={[openOption(), ...(aowNames ?? []).map((name) => ({ value: name, label: name }))]}
            onChange={(aowName) => patchRequest({ aowName })}
          />
          {separateUpgradeCaps ? (
            <div className="rail-pair">
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
            <label className="profile-upgrade-input">
              Weapon Upgrade
              <DraftNumberInput
                min={0}
                max={standardUpgradeLimit}
                value={request.standardMaxUpgrade}
                onDraftChange={markResultsStale}
                onCommit={(upgrade) => patchRequest({
                  standardMaxUpgrade: upgrade,
                  somberMaxUpgrade: upgrade,
                })}
              />
              <small>Convergence uses one +0–+15 reinforcement path for every weapon.</small>
            </label>
          )}
          <div className="upgrade-mode-row" aria-label="Upgrade search policy">
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
          <div className="rail-pair upgrade-cap-row">
            <div className="cap-readout">
              <span>
                {separateUpgradeCaps
                  ? weaponProfile?.isSomber ? "Selected Somber cap" : "Selected Standard cap"
                  : "Selected Convergence cap"}
              </span>
              <strong>+{weaponProfile?.maxUpgrade ?? standardUpgradeLimit}</strong>
            </div>
            <small className="rail-helper">
              {request.exactUpgrade
                ? "Only the entered reinforcement levels are ranked."
                : "Every reinforcement level from zero to each cap is eligible."}
            </small>
          </div>
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
          <div className="segmented" aria-label="Result grouping">
            {(["automatic", "weapon", "loadout"] as const).map((grouping) => (
              <button
                type="button"
                key={grouping}
                className={request.resultGrouping === grouping ? "active" : ""}
                onClick={() => patchRequest({ resultGrouping: grouping })}
              >
                {grouping === "automatic" ? "Auto" : grouping === "weapon" ? "Per weapon" : "Per loadout"}
              </button>
            ))}
          </div>
          <GenericFilters dimensions={catalog?.filterDimensions ?? []} request={request} patchRequest={patchRequest} />
        </section>

        <section className="rail-section">
          <div className="section-title">
            <Crosshair size={15} />
            <span>Objective</span>
          </div>
          <div className="segmented">
            {(catalog?.objectiveIds ?? []).map((objective) => (
              <button
                key={objective}
                className={request.objective === objective ? "active" : ""}
                type="button"
                onClick={() => patchRequest({ objective })}
              >
                {objectiveLabel(objective)}
              </button>
            ))}
          </div>
          <label className="toggle-line" title="Apply the 1.5x effective STR rule when legal">
            <input
              type="checkbox"
              checked={request.twoHanding}
              onChange={(event) => patchRequest({ twoHanding: event.target.checked })}
            />
            Two-handing
          </label>
          {scadutreeAvailable ? (
            <>
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
              <div className="cap-readout" title="Outgoing damage multiplier and equivalent incoming damage reduction from the selected blessing level">
                <span>{request.dlcScaling ? "Shadow Realm" : "DLC off"}</span>
                <strong>
                  x{scadutreeDamageMultiplier.toFixed(2)} dmg / x{scadutreeTakenMultiplier.toFixed(3)} taken
                </strong>
                <small>{(scadutreeNegation * 100).toFixed(1)}% negation</small>
              </div>
            </>
          ) : (
            <div className="profile-rule-note" role="note">
              <span>Convergence rule</span>
              <strong>Scadutree Blessing is removed</strong>
              <small>Weapon AR is calculated without Shadow Realm blessing controls.</small>
            </div>
          )}
        </section>

        <section className="rail-section advanced-section">
          <div className="section-title">
            <SlidersHorizontal size={15} />
            <span>Fine tuning</span>
          </div>
          <p className="section-intro">Optional minimums and exact result locks. Leave these open for automatic optimization.</p>
          <div className="advanced-body">
              {separateUpgradeCaps ? (
                <SearchableSelect
                  label="Somber"
                  value={request.somberFilter}
                  options={(catalog?.somberFilters ?? []).map((value) => ({ value, label: somberFilterLabel(value) }))}
                  onChange={(somberFilter) => somberFilter && patchRequest({ somberFilter })}
                />
              ) : null}
              <div className="stat-grid floors">
                {[
                  ["Min STR", "minStr"],
                  ["Min DEX", "minDex"],
                  ["Min INT", "minInt"],
                  ["Min FAI", "minFai"],
                  ["Min ARC", "minArc"],
                ].map(([label, key]) => (
                  <label key={key}>
                    {label}
                    <DraftNumberInput
                      min={0}
                      max={99}
                      value={Number(request[key as keyof typeof request])}
                      onDraftChange={markResultsStale}
                      onCommit={(value) => patchRequest({ [key]: value } as Partial<OptimizeRequestDto>)}
                    />
                  </label>
                ))}
              </div>
              <label className="toggle-line" title="Use combat stats captured by Use As Locks">
                <input
                  type="checkbox"
                  checked={lockedStatMode}
                  onChange={(event) => setLockedStatMode(event.target.checked)}
                />
                Use Locked Result Stats
              </label>
              <div className="lock-readout">
                <span>Locks</span>
                <strong>
                  {!lockedStatMode || request.lockStr === null
                    ? "Open"
                    : `STR ${request.lockStr} DEX ${request.lockDex} INT ${request.lockInt} FAI ${request.lockFai} ARC ${request.lockArc}`}
                </strong>
              </div>
              <button
                className="clear-locks"
                type="button"
                onClick={() => {
                  setLockedStatMode(false);
                  patchRequest({ lockStr: null, lockDex: null, lockInt: null, lockFai: null, lockArc: null });
                }}
              >
                Clear Locks
              </button>
          </div>
        </section>
      </fieldset>

      <div className="rail-actions">
        <button
          className={`search-button ${isSearching ? "busy" : ""}`}
          type="button"
          onClick={isSearching ? cancelSearch : runSearch}
          disabled={searchCancellationRequested || (!isSearching && !catalog)}
        >
          {isSearching ? <RotateCcw size={17} /> : <Play size={17} />}
          {isSearching
            ? (searchCancellationRequested ? "Cancelling..." : "Cancel Search")
            : resultsStale
              ? "Update Results"
              : "Search"}
        </button>

        {isSearching ? (
          <SearchProgressPanel
            progress={progress}
            elapsedMs={progress?.elapsedMs ?? elapsedMs}
            objective={objectiveLabel(request.objective)}
          />
        ) : catalog ? (
          <div className="estimate-strip quick-estimate">
            <span>Search scope</span>
            <strong>{request.weaponName || (request.weaponTypeKey ? "Filtered" : "Open")}</strong>
            <span>{budget.redistributable} free points</span>
            <small>
              Exact combinations are prepared only after Search, keeping stat editing responsive.
            </small>
          </div>
        ) : null}
      </div>
    </aside>
  );
}

function GenericFilters({
  dimensions,
  request,
  patchRequest,
}: {
  dimensions: FilterDimensionDto[];
  request: OptimizeRequestDto;
  patchRequest: (patch: Partial<OptimizeRequestDto>) => void;
}) {
  if (!dimensions.length) return null;
  const active = request.filters.entries.length;
  return (
    <details className="generic-filters">
      <summary>Filters{active ? ` (${active})` : ""}</summary>
      {active ? <button type="button" className="clear-locks" onClick={() => patchRequest({ filters: { version: 1, entries: [] } })}>Clear filters</button> : null}
      {dimensions.map((dimension) => (
        <FilterDimensionControl key={dimension.id} dimension={dimension} request={request} patchRequest={patchRequest} />
      ))}
      <small>Click once to include, twice to exclude, three times to clear.</small>
    </details>
  );
}

function FilterDimensionControl({
  dimension,
  request,
  patchRequest,
}: {
  dimension: FilterDimensionDto;
  request: OptimizeRequestDto;
  patchRequest: (patch: Partial<OptimizeRequestDto>) => void;
}) {
  const [query, setQuery] = useState("");
  const options = dimension.options.filter((option) => option.label.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
  function cycle(id: string) {
    const current = request.filters.entries.find((entry) => entry.dimension === dimension.id && entry.id === id);
    const entries = request.filters.entries.filter((entry) => !(entry.dimension === dimension.id && entry.id === id));
    if (!current) entries.push({ dimension: dimension.id, id, mode: "include" });
    else if (current.mode === "include") entries.push({ dimension: dimension.id, id, mode: "exclude" });
    patchRequest({ filters: { version: 1, entries } });
  }
  return (
    <details className="filter-dimension" open={dimension.id === "weapon_family"}>
      <summary>{dimension.label}</summary>
      {dimension.options.length > 16 ? <input type="search" value={query} placeholder={`Find ${dimension.label.toLocaleLowerCase()}`} onChange={(event) => setQuery(event.target.value)} /> : null}
      <div className="filter-option-list">
        {options.map((option) => {
          const mode = request.filters.entries.find((entry) => entry.dimension === dimension.id && entry.id === option.id)?.mode;
          return (
            <button type="button" className={mode ?? "neutral"} aria-pressed={Boolean(mode)} key={option.id} onClick={() => cycle(option.id)}>
              <span>{option.label}</span><small>{option.count.toLocaleString()}</small>
            </button>
          );
        })}
      </div>
    </details>
  );
}

function SearchProgressPanel({
  progress,
  elapsedMs,
  objective,
}: {
  progress: SearchProgressDto | null;
  elapsedMs: number;
  objective: string;
}) {
  const pct = progress ? Math.min(100, (progress.checked / Math.max(progress.total, 1)) * 100) : null;
  return (
    <div className="estimate-strip progress-strip">
      <div className="progress-meta">
        <span>{pct === null ? "Starting" : `${fixed1(pct)}%`}</span>
        <strong aria-label={progress ? `${objective} best score` : "Elapsed time"}>
          {progress ? fixed1(progress.bestScore) : formatDuration(elapsedMs)}
        </strong>
        <span>{progress ? `${progress.eligible} eligible` : "checking"}</span>
      </div>
      <div className={`search-progress-bar ${pct === null ? "indeterminate" : ""}`}>
        <i style={pct === null ? undefined : { width: `${pct}%` }} />
      </div>
      <small>{objective} · {formatDuration(elapsedMs)}</small>
    </div>
  );
}

function formatDuration(ms: number): string {
  const seconds = Math.max(0, ms / 1000);
  return `${seconds.toFixed(seconds < 10 ? 1 : 0)}s`;
}

function somberFilterLabel(value: string): string {
  return value
    .split("_")
    .map((word) => `${word.slice(0, 1).toUpperCase()}${word.slice(1)}`)
    .join(" ");
}

function DraftNumberInput({
  value,
  min,
  max,
  onCommit,
  onDraftChange,
  readOnly = false,
}: {
  value: number;
  min: number;
  max: number;
  onCommit: (value: number) => void;
  onDraftChange?: () => void;
  readOnly?: boolean;
}) {
  const [draft, setDraft] = useState(String(value));
  const idleCommit = useRef<number | null>(null);

  function clearIdleCommit() {
    if (idleCommit.current !== null) {
      window.clearTimeout(idleCommit.current);
      idleCommit.current = null;
    }
  }

  useEffect(() => {
    clearIdleCommit();
    setDraft(String(value));
  }, [value]);

  useEffect(() => () => clearIdleCommit(), []);

  function commit(raw: string) {
    clearIdleCommit();
    const parsed = parseInteger(raw);
    if (!Number.isInteger(parsed)) {
      setDraft(String(value));
      return;
    }
    const next = clamp(parsed, min, max);
    setDraft(String(next));
    if (next !== value) {
      onCommit(next);
    }
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      commit(event.currentTarget.value);
      event.currentTarget.blur();
    }
  }

  return (
    <input
      type="number"
      min={min}
      max={max}
      readOnly={readOnly}
      value={draft}
      onBlur={(event) => commit(event.target.value)}
      onChange={(event) => {
        const next = event.target.value;
        clearIdleCommit();
        setDraft(next);
        if (next !== String(value)) {
          onDraftChange?.();
        }
        const parsed = parseInteger(next);
        if (Number.isInteger(parsed) && parsed >= min && parsed <= max && parsed !== value) {
          idleCommit.current = window.setTimeout(() => commit(next), 700);
        }
      }}
      onKeyDown={handleKeyDown}
    />
  );
}

function parseInteger(value: string): number {
  if (!/^\d+$/.test(value.trim())) {
    return Number.NaN;
  }
  return Number(value);
}

function statIsShort(key: string, gaps: Record<string, number> | null): boolean {
  return Boolean(gaps && (gaps[key] ?? 0) > 0);
}

function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
