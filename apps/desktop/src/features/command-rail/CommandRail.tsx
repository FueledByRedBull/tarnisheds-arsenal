import { Crosshair, Filter, Play, RotateCcw, SlidersHorizontal, Sparkles, Swords } from "lucide-react";
import { KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { AowSelect } from "../../lib/AowSelect";
import { api } from "../../lib/api";
import { useRequestBudget, useWeaponProfile } from "../../lib/hooks";
import { fixed1, objectiveLabel } from "../../lib/format";
import { CheckboxMultiSelect, SearchableSelect, openOption } from "../../lib/SearchableSelect";
import {
  SCADUTREE_MAX_LEVEL,
  scadutreeAttackMultiplier,
  scadutreeDamageNegation,
  scadutreeReceivedDamageMultiplier,
} from "../../lib/scadutree";
import { classMeta, classOptions, derivedLevel, EIGHT_STAT_KEYS, optimalStartingClass, startingClassLevel } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { EightStatsDto, FilterDimensionDto, OptimizeRequestDto } from "../../lib/types";
import { runSearchFromStore } from "../../lib/workflows";

export function CommandRail() {
  const catalog = useDesktopStore((state) => state.catalog);
  const activeWorkspace = useDesktopStore((state) => state.activeWorkspace);
  const request = useDesktopStore((state) => state.request);
  const patchRequest = useDesktopStore((state) => state.patchRequest);
  const applyClass = useDesktopStore((state) => state.applyClass);
  const markResultsStale = useDesktopStore((state) => state.markResultsStale);
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const setError = useDesktopStore((state) => state.setError);
  const setSearching = useDesktopStore((state) => state.setSearching);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const isExporting = useDesktopStore((state) => state.isExporting);
  const setActiveJobId = useDesktopStore((state) => state.setActiveJobId);
  const setProgress = useDesktopStore((state) => state.setProgress);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const setLockedStatMode = useDesktopStore((state) => state.setLockedStatMode);
  const [searchStartedAt, setSearchStartedAt] = useState<number | null>(null);
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
  const weaponFiltersActive = Boolean(
    request.weaponTypeKey
    || request.weaponName
    || request.affinity
    || request.aowName
    || request.somberFilter !== "all"
    || request.filters.entries.some((entry) => entry.dimension !== "coverage"),
  );
  const startingClasses = classOptions(catalog);
  const fixedStats = catalog?.dataManifest.capabilities.classBudget === false;
  const [advancedOpen, setAdvancedOpen] = useState(fixedStats);
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

  useEffect(() => {
    setAdvancedOpen(fixedStats);
  }, [fixedStats]);

  useEffect(() => {
    if (!isSearching) {
      setSearchStartedAt(null);
      setSearchCancellationRequested(false);
      searchCancellationRequestedRef.current = false;
    }
  }, [isSearching]);

  async function runSearch() {
    searchCancellationRequestedRef.current = false;
    setSearchCancellationRequested(false);
    setSearchStartedAt(Date.now());
    setError(null);
    setProgress(null);
    await runSearchFromStore(apiRequest, () => searchCancellationRequestedRef.current);
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

  function optimizeClass() {
    const targets: EightStatsDto = request;
    const optimal = optimalStartingClass(catalog, targets, request.className);
    const targetLevel = startingClassLevel(optimal, targets);
    const stats = Object.fromEntries(EIGHT_STAT_KEYS.map((key) => [
      key,
      Math.max(optimal.baseStats[key], targets[key]),
    ])) as unknown as EightStatsDto;
    patchRequest({ className: optimal.name, characterLevel: targetLevel, ...stats });
  }

  function resetWeaponFilters() {
    patchRequest({
      weaponTypeKey: null,
      weaponName: null,
      affinity: null,
      aowName: null,
      somberFilter: "all",
      filters: {
        version: 1,
        entries: request.filters.entries.filter((entry) =>
          entry.dimension === "coverage"
        ),
      },
    });
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
            disabled={fixedStats}
            value={request.className}
            options={startingClasses.map((entry) => ({ value: entry.name, label: entry.name }))}
            onChange={(value) => value && applyClass(value)}
          />
          <div className="character-action-row">
            <button type="button" onClick={optimizeClass} disabled={fixedStats} title="Choose the lowest required level for all eight entered stats">
              <Sparkles size={14} />
              Optimize class
            </button>
            <button type="button" onClick={() => fixedStats ? patchRequest({ vig: 1, mnd: 1, end: 1, strStat: 1, dex: 1, intStat: 1, fai: 1, arc: 1 }) : applyClass(request.className)} title={fixedStats ? "Reset all eight stats to 1" : "Reset all eight stats to this class's base values"}>
              <RotateCcw size={14} />
              Reset stats
            </button>
          </div>
          <div className="level-strip">
            <label>
              {fixedStats ? "Stat total" : "Level"}
              <input readOnly value={derivedLevel(catalog, request)} />
            </label>
            {!fixedStats && <div className="budget-readout" title={`Levels above ${request.className}'s base level ${budget.baseLevel}`}>
              <span>Lv Ups</span>
              <strong>{budget.levelUps}</strong>
            </div>}
            <div
              className="budget-readout"
              title={fixedStats
                ? "Convergence uses the entered combat stats exactly"
                : "Movable STR/DEX/INT/FAI/ARC points after class minimums, fixed VIG/MND/END, and advanced minimum floors"}
            >
              <span>{fixedStats ? "Mode" : "Redistrib"}</span>
              <strong>{fixedStats ? "Fixed stats" : budget.redistributable}</strong>
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
                  min={Math.max(1, Number(min))}
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
            <span>{fixedStats || lockedStatMode ? "Exact combat stats" : "Stats optimized"}</span>
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
              <small>Changing class or loadout keeps these locks and may make the query incompatible. Clear locks in Advanced when you want automatic stats.</small>
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
            <Crosshair size={15} />
            <span>Objective</span>
          </div>
          <div className="segmented" role="group" aria-label="Ranking objective">
            {(catalog?.objectiveIds ?? []).map((objective) => (
              <button
                key={objective}
                className={request.objective === objective ? "active" : ""}
                type="button"
                aria-pressed={request.objective === objective}
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

        <section className="rail-section">
          <div className="section-title">
            <Filter size={15} />
            <span>Loadout</span>
          </div>
          <CheckboxMultiSelect
            label="Weapon Type"
            values={selectedTypeIds}
            excludedValues={excludedTypeIds}
            options={typeDimension?.options.map((option) => ({ value: option.id, label: option.label, count: option.count })) ?? []}
            onChange={(values, excludedValues) => patchRequest({
              weaponTypeKey: null,
              weaponName: null,
              aowName: null,
              filters: { version: 1, entries: replaceDimensionFilters(request.filters.entries, "weapon_type", values, excludedValues) },
            })}
          />
          <SearchableSelect
            label="Weapon"
            value={request.weaponName}
            options={[openOption(), ...(catalog?.weaponNames ?? []).map((name) => ({ value: name, label: name }))]}
            onChange={(weaponName) => patchRequest({
              weaponName,
              weaponTypeKey: null,
              affinity: null,
              aowName: null,
              filters: {
                version: 1,
                entries: replaceDimensionFilters(
                  replaceDimensionFilters(request.filters.entries, "weapon_type", [], []),
                  "affinity",
                  [],
                  [],
                ),
              },
            })}
          />
          <CheckboxMultiSelect
            label="Affinity"
            values={selectedAffinityIds}
            excludedValues={excludedAffinityIds}
            options={affinityDimension?.options.map((option) => ({ value: option.id, label: option.label, count: option.count })) ?? []}
            onChange={(values, excludedValues) => patchRequest({
              affinity: null,
              aowName: null,
              filters: { version: 1, entries: replaceDimensionFilters(request.filters.entries, "affinity", values, excludedValues) },
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
            setError={setError}
          />
          <button
            type="button"
            className="rail-reset-button"
            disabled={!weaponFiltersActive}
            onClick={resetWeaponFilters}
          >
            <RotateCcw size={14} />
            Reset weapon filters
          </button>
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
          <div className="segmented" role="group" aria-label="Result grouping">
            {(["automatic", "weapon", "loadout"] as const).map((grouping) => (
              <button
                type="button"
                key={grouping}
                className={request.resultGrouping === grouping ? "active" : ""}
                aria-pressed={request.resultGrouping === grouping}
                onClick={() => patchRequest({ resultGrouping: grouping })}
              >
                {grouping === "automatic" ? "Auto" : grouping === "weapon" ? "Per weapon" : "Per loadout"}
              </button>
            ))}
          </div>
        </section>

        <details
          className="rail-section advanced-section"
          open={advancedOpen}
          onToggle={(event) => setAdvancedOpen(event.currentTarget.open)}
        >
          <summary className="section-title">
            <SlidersHorizontal size={15} />
            <span>Advanced</span>
          </summary>
          <p className="section-intro">
            {fixedStats
              ? "Convergence uses the entered combat stats exactly. Minimum floors do not redistribute stats."
              : "Optional minimums and exact result locks. Leave these open for automatic optimization."}
          </p>
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
                  checked={fixedStats || lockedStatMode}
                  disabled={fixedStats}
                  onChange={(event) => setLockedStatMode(event.target.checked)}
                />
                {fixedStats ? "Use entered combat stats exactly" : "Use Locked Result Stats"}
              </label>
              <div className="lock-readout">
                <span>Locks</span>
                <strong>
                  {fixedStats ? `STR ${request.strStat} DEX ${request.dex} INT ${request.intStat} FAI ${request.fai} ARC ${request.arc}` : !lockedStatMode || request.lockStr === null
                    ? "Open"
                    : `STR ${request.lockStr} DEX ${request.lockDex} INT ${request.lockInt} FAI ${request.lockFai} ARC ${request.lockArc}`}
                </strong>
              </div>
              <button
                className="clear-locks"
                type="button"
                disabled={fixedStats}
                onClick={() => {
                  setLockedStatMode(false);
                  patchRequest({ lockStr: null, lockDex: null, lockInt: null, lockFai: null, lockArc: null });
                }}
              >
                Clear Locks
              </button>
          </div>
        </details>
      </fieldset>

      {(activeWorkspace === "rankings" || isSearching) ? (
        <div className="rail-actions">
          <button
            className={`search-button ${isSearching ? "busy" : ""}`}
            type="button"
            onClick={isSearching ? cancelSearch : runSearch}
            disabled={isExporting || searchCancellationRequested || (!isSearching && !catalog)}
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
              searchStartedAt={searchStartedAt}
              objective={objectiveLabel(request.objective)}
            />
          ) : catalog ? (
            <div className="estimate-strip quick-estimate" aria-label="Search scope">
              <span>Scope</span>
              <strong>{request.weaponName || (selectedTypeIds.length ? `${selectedTypeIds.length} type${selectedTypeIds.length === 1 ? "" : "s"}` : "Open")}</strong>
              <span>{fixedStats ? "Entered stats fixed" : `${budget.redistributable} free points`}</span>
            </div>
          ) : null}
        </div>
      ) : null}
    </aside>
  );
}

function selectedFilterIds(
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

function replaceDimensionFilters(
  entries: OptimizeRequestDto["filters"]["entries"],
  dimension: "weapon_type" | "affinity",
  ids: string[],
  excludedIds: string[],
) {
  return [
    ...entries.filter((entry) => entry.dimension !== dimension),
    ...ids.map((id) => ({ dimension, id, mode: "include" as const })),
    ...excludedIds.map((id) => ({ dimension, id, mode: "exclude" as const })),
  ];
}

function SearchProgressPanel({
  searchStartedAt,
  objective,
}: {
  searchStartedAt: number | null;
  objective: string;
}) {
  const progress = useDesktopStore((state) => state.progress);
  const [elapsedMs, setElapsedMs] = useState(0);
  const hasProgress = progress !== null;
  useEffect(() => {
    if (hasProgress) return;
    const startedAt = searchStartedAt ?? Date.now();
    const tick = window.setInterval(() => setElapsedMs(Date.now() - startedAt), 200);
    return () => window.clearInterval(tick);
  }, [hasProgress, searchStartedAt]);
  const pct = progress ? Math.min(100, (progress.checked / Math.max(progress.total, 1)) * 100) : null;
  return (
    <div className="estimate-strip progress-strip">
      <div className="progress-meta">
        <span>{pct === null ? "Starting" : `${fixed1(pct)}%`}</span>
        <strong aria-label={progress ? `${objective} best score` : "Elapsed time"}>
          {progress ? fixed1(progress.bestScore) : formatDuration(elapsedMs)}
        </strong>
        <span>{progress ? `${progress.eligible} covered` : "checking"}</span>
      </div>
      <div className={`search-progress-bar ${pct === null ? "indeterminate" : ""}`}>
        <i style={pct === null ? undefined : { width: `${pct}%` }} />
      </div>
      <small>{objective} · {formatDuration(progress?.elapsedMs ?? elapsedMs)}</small>
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
