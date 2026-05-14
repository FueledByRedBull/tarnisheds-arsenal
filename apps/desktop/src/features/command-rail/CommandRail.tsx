import { Crosshair, Filter, Play, RotateCcw, SlidersHorizontal, Swords } from "lucide-react";
import { KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { api, hasTauriRuntime } from "../../lib/api";
import { fixed1, objectiveLabel } from "../../lib/format";
import { SearchableSelect, openOption } from "../../lib/SearchableSelect";
import { buildOptimizeRequest, budgetSnapshot, classMeta, classOptions, derivedLevel } from "../../lib/session";
import { objectiveOptions, useDesktopStore } from "../../lib/state";
import { OptimizeRequestDto, SearchFinishedDto, SearchProgressDto, WeaponProfileDto } from "../../lib/types";

const AFFINITY_OPTIONS = [
  "Standard",
  "Heavy",
  "Keen",
  "Quality",
  "Fire",
  "Flame Art",
  "Lightning",
  "Sacred",
  "Magic",
  "Cold",
  "Poison",
  "Blood",
  "Occult",
  "Unique",
];

export function CommandRail() {
  const catalog = useDesktopStore((state) => state.catalog);
  const request = useDesktopStore((state) => state.request);
  const patchRequest = useDesktopStore((state) => state.patchRequest);
  const applyClass = useDesktopStore((state) => state.applyClass);
  const setEstimate = useDesktopStore((state) => state.setEstimate);
  const setRows = useDesktopStore((state) => state.setRows);
  const clearResults = useDesktopStore((state) => state.clearResults);
  const setError = useDesktopStore((state) => state.setError);
  const setSearching = useDesktopStore((state) => state.setSearching);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const activeJobId = useDesktopStore((state) => state.activeJobId);
  const progress = useDesktopStore((state) => state.progress);
  const setActiveJobId = useDesktopStore((state) => state.setActiveJobId);
  const setProgress = useDesktopStore((state) => state.setProgress);
  const estimate = useDesktopStore((state) => state.estimate);
  const lockedStatMode = useDesktopStore((state) => state.lockedStatMode);
  const setLockedStatMode = useDesktopStore((state) => state.setLockedStatMode);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [weaponProfile, setWeaponProfile] = useState<WeaponProfileDto | null>(null);
  const [weaponNames, setWeaponNames] = useState<string[]>([]);
  const [aowNames, setAowNames] = useState<string[]>([]);
  const [searchStartedAt, setSearchStartedAt] = useState<number | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [searchCancellationRequested, setSearchCancellationRequested] = useState(false);
  const searchCancellationRequestedRef = useRef(false);
  const meta = classMeta(catalog, request.className);
  const budget = budgetSnapshot(catalog, request);
  const apiRequest = useMemo(
    () => buildOptimizeRequest(catalog, request, lockedStatMode),
    [catalog, lockedStatMode, request],
  );
  const effectiveStr =
    request.twoHanding && !weaponProfile?.disablesTwoHandBonus
      ? Math.min(99, Math.floor(request.strStat * 1.5))
      : request.strStat;
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
  const affinityOptions = weaponProfile?.affinities.length ? weaponProfile.affinities : AFFINITY_OPTIONS;
  const typeOptions = catalog?.weaponTypeOptions.length
    ? catalog.weaponTypeOptions
    : catalog?.weaponTypeKeys.map((key) => ({ key, label: key })) ?? [];
  const startingClasses = classOptions(catalog);

  useEffect(() => {
    let cancelled = false;
    api.weaponNamesForType(request.weaponTypeKey).then((names) => {
      if (!cancelled) setWeaponNames(names);
    }).catch((error) => setError(error instanceof Error ? error.message : String(error)));
    return () => {
      cancelled = true;
    };
  }, [request.weaponTypeKey, setError]);

  useEffect(() => {
    let cancelled = false;
    async function loadAows() {
      const names = request.weaponName
        ? await api.compatibleAowNames(request.weaponName, request.affinity)
        : request.affinity
          ? await api.compatibleAowNamesForAffinity(request.affinity)
          : catalog?.aowNames ?? [];
      if (!cancelled) setAowNames(names);
    }
    loadAows().catch((error) => setError(error instanceof Error ? error.message : String(error)));
    return () => {
      cancelled = true;
    };
  }, [catalog?.aowNames, request.affinity, request.weaponName, setError]);

  useEffect(() => {
    let cancelled = false;
    async function loadWeaponProfile() {
      if (!request.weaponName) {
        setWeaponProfile(null);
        return;
      }
      const profile = await api.weaponProfile(request.weaponName, request.affinity);
      if (cancelled) return;
      setWeaponProfile(profile);

      const patch: Partial<OptimizeRequestDto> = {};
      if (request.maxUpgrade > profile.maxUpgrade) patch.maxUpgrade = profile.maxUpgrade;
      if (request.fixedUpgrade !== null && request.fixedUpgrade > profile.maxUpgrade) {
        patch.fixedUpgrade = profile.maxUpgrade;
      }
      if (request.affinity && !profile.affinities.includes(request.affinity)) {
        patch.affinity = profile.affinities[0] ?? null;
      }
      if (request.aowName && !profile.compatibleAows.includes(request.aowName)) {
        patch.aowName = null;
      }
      if (Object.keys(patch).length > 0) patchRequest(patch);
    }

    loadWeaponProfile().catch((error) => {
      if (!cancelled) {
        setWeaponProfile(null);
        setError(error instanceof Error ? error.message : String(error));
      }
    });

    return () => {
      cancelled = true;
    };
  }, [patchRequest, request.affinity, request.aowName, request.fixedUpgrade, request.maxUpgrade, request.weaponName, setError]);

  useEffect(() => {
    if (!activeJobId || !isSearching) return undefined;
    let disposed = false;

    async function pollSearchStatus() {
      try {
        const currentJobId = useDesktopStore.getState().activeJobId;
        if (!currentJobId) return;
        const status = await api.searchStatus(currentJobId);
        if (disposed) return;
        if (!status) {
          finishSearch({
            jobId: currentJobId,
            cancelled: true,
            rows: [],
            error: "Search job disappeared before returning a result.",
          });
          return;
        }
        if (status.progress) setProgress(status.progress);
        if (status.finished) finishSearch(status.finished);
      } catch (error) {
        if (!disposed) {
          finishSearch({
            jobId: useDesktopStore.getState().activeJobId ?? "search",
            cancelled: false,
            rows: [],
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }

    void pollSearchStatus();
    const interval = window.setInterval(pollSearchStatus, 200);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [activeJobId, isSearching, setProgress]);

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
    setSearching(true);
    setSearchCancellationRequested(false);
    setSearchStartedAt(Date.now());
    setError(null);
    setProgress(null);
    try {
      const nextEstimate = await api.estimateSearchSpace(apiRequest);
      if (searchCancellationRequestedRef.current) {
        clearResults("Search stopped.");
        setSearching(false);
        setSearchStartedAt(null);
        setSearchCancellationRequested(false);
        searchCancellationRequestedRef.current = false;
        return;
      }
      setEstimate(nextEstimate);
      if (nextEstimate.combinations <= 0) {
        clearResults("No valid search space for current constraints.");
        setSearching(false);
        setSearchStartedAt(null);
        setSearchCancellationRequested(false);
        return;
      }
      if (hasTauriRuntime()) {
        const { jobId } = await api.startSearch(apiRequest);
        setActiveJobId(jobId);
        if (searchCancellationRequestedRef.current) {
          await api.cancelSearch(jobId);
          return;
        }
      } else {
        const rows = await api.runSearch(apiRequest);
        setRows(rows);
        setSearching(false);
        setSearchStartedAt(null);
        setSearchCancellationRequested(false);
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
      setSearching(false);
      setSearchStartedAt(null);
      setSearchCancellationRequested(false);
    }
  }

  function finishSearch(payload: SearchFinishedDto) {
    const current = useDesktopStore.getState();
    if (current.activeJobId && payload.jobId !== current.activeJobId) return;
    if (payload.error) current.setError(payload.error);
    if (payload.cancelled) current.clearResults("Search stopped.");
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
      </div>

      <div className="rail-scroll">
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
          <div className="level-strip">
            <label>
              Level
              <input readOnly value={derivedLevel(catalog, request)} />
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
                  onDraftChange={clearResults}
                  onCommit={(value) => patchRequest({ [key]: value } as Partial<OptimizeRequestDto>)}
                />
              </label>
            ))}
          </div>
          <div className="hero-chip-row">
            <span>{request.twoHanding ? "2H" : "1H"}</span>
            <span>{lockedStatMode ? "Exact Stats" : "Open Stats"}</span>
            <span>{request.fixedUpgrade !== null ? `Exact +${request.maxUpgrade}` : `+0..+${request.maxUpgrade}`}</span>
          </div>
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
            <span>Scope</span>
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
          <div className="rail-pair">
            <label>
              Max Upgrade
              <DraftNumberInput
                min={0}
                max={weaponProfile?.maxUpgrade ?? 25}
                value={request.maxUpgrade}
                onDraftChange={clearResults}
                onCommit={(maxUpgrade) => {
                  patchRequest({
                    maxUpgrade,
                    fixedUpgrade: request.fixedUpgrade === null ? null : maxUpgrade,
                  });
                }}
              />
            </label>
            <label className="toggle-line" title="Force exactly the selected upgrade level">
              <input
                type="checkbox"
                checked={request.fixedUpgrade !== null}
                onChange={(event) =>
                  patchRequest({ fixedUpgrade: event.target.checked ? request.maxUpgrade : null })
                }
              />
              Exact
            </label>
          </div>
          <div className="cap-readout">
            <span>{weaponProfile?.isSomber ? "Somber" : "Standard"} cap</span>
            <strong>+{weaponProfile?.maxUpgrade ?? 25}</strong>
          </div>
          <label>
            Top Results
            <DraftNumberInput
              min={1}
              max={50}
              value={request.topK}
              onDraftChange={clearResults}
              onCommit={(topK) => patchRequest({ topK })}
            />
          </label>
        </section>

        <section className="rail-section">
          <div className="section-title">
            <Crosshair size={15} />
            <span>Objective</span>
          </div>
          <div className="segmented">
            {objectiveOptions.map((objective) => (
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
        </section>

        <section className="rail-section advanced-section">
          <button className="advanced-toggle" type="button" onClick={() => setAdvancedOpen((value) => !value)}>
            <SlidersHorizontal size={15} />
            <span>Advanced</span>
            <strong>{advancedOpen ? "Hide" : "Show"}</strong>
          </button>
          {advancedOpen ? (
            <div className="advanced-body">
              <SearchableSelect
                label="Somber"
                value={request.somberFilter}
                options={[
                  { value: "all", label: "All" },
                  { value: "standard_only", label: "Standard Only" },
                  { value: "somber_only", label: "Somber Only" },
                ]}
                onChange={(somberFilter) => somberFilter && patchRequest({ somberFilter })}
              />
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
                      onDraftChange={clearResults}
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
          ) : null}
        </section>
      </div>

      <div className="rail-actions">
        <button
          className={`search-button ${isSearching ? "busy" : ""}`}
          type="button"
          onClick={isSearching ? cancelSearch : runSearch}
          disabled={searchCancellationRequested}
        >
          {isSearching ? <RotateCcw size={17} /> : <Play size={17} />}
          {isSearching ? (searchCancellationRequested ? "Cancelling..." : "Cancel Search") : "Search"}
        </button>

        {isSearching ? (
          <SearchProgressPanel progress={progress} elapsedMs={progress?.elapsedMs ?? elapsedMs} />
        ) : estimate ? (
          <div className="estimate-strip">
            <span>{estimate.weaponCandidates} weapons</span>
            <strong>{fixed1(estimate.combinations / 1000)}k</strong>
            <span>combos</span>
          </div>
        ) : null}
      </div>
    </aside>
  );
}

function SearchProgressPanel({
  progress,
  elapsedMs,
}: {
  progress: SearchProgressDto | null;
  elapsedMs: number;
}) {
  const pct = progress ? Math.min(100, (progress.checked / Math.max(progress.total, 1)) * 100) : null;
  return (
    <div className="estimate-strip progress-strip">
      <div className="progress-meta">
        <span>{pct === null ? "Starting" : `${fixed1(pct)}%`}</span>
        <strong>{progress ? fixed1(progress.bestScore) : formatDuration(elapsedMs)}</strong>
        <span>{progress ? `${progress.eligible} eligible` : "checking"}</span>
      </div>
      <div className={`search-progress-bar ${pct === null ? "indeterminate" : ""}`}>
        <i style={pct === null ? undefined : { width: `${pct}%` }} />
      </div>
      <small>{formatDuration(elapsedMs)}</small>
    </div>
  );
}

function formatDuration(ms: number): string {
  const seconds = Math.max(0, ms / 1000);
  return `${seconds.toFixed(seconds < 10 ? 1 : 0)}s`;
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

  useEffect(() => {
    setDraft(String(value));
  }, [value]);

  function commit(raw: string) {
    const next = clamp(parseInteger(raw), min, max);
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
        setDraft(next);
        if (next !== String(value)) {
          onDraftChange?.();
        }
        const parsed = parseInteger(next);
        if (Number.isInteger(parsed) && parsed >= min && parsed <= max && parsed !== value) {
          onCommit(parsed);
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
