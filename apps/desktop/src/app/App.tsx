import { useEffect, useRef, useState } from "react";
import { Check, CircleAlert, GitCompareArrows, Layers3, LoaderCircle, Radar, RotateCcw, Route, Table2, X } from "lucide-react";
import { api } from "../lib/api";
import { setAnalysisCacheVersion } from "../lib/analysis-cache";
import { useDesktopStore } from "../lib/state";
import { WorkspaceTab } from "../lib/types";
import { AffinityWatchView } from "../features/affinity-watch/AffinityWatchView";
import { CommandRail } from "../features/command-rail/CommandRail";
import { CompareView } from "../features/compare/CompareView";
import { Inspector } from "../features/inspector/Inspector";
import { PathsView } from "../features/paths/PathsView";
import { RankingsBoard } from "../features/rankings/RankingsBoard";

const tabs: Array<{ id: WorkspaceTab; label: string; icon: typeof Table2 }> = [
  { id: "rankings", label: "Rankings", icon: Table2 },
  { id: "compare", label: "Compare", icon: GitCompareArrows },
  { id: "paths", label: "Paths", icon: Route },
  { id: "affinity_watch", label: "Affinity Watch", icon: Radar },
];

const PROFILE_STORAGE_KEY = "tarnisheds-arsenal.gameProfile.v1";

export function App() {
  const activeWorkspace = useDesktopStore((state) => state.activeWorkspace);
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
  const profiles = useDesktopStore((state) => state.profiles);
  const setProfiles = useDesktopStore((state) => state.setProfiles);
  const profileId = useDesktopStore((state) => state.request.profileId);
  const beginProfileSwitch = useDesktopStore((state) => state.beginProfileSwitch);
  const setCatalog = useDesktopStore((state) => state.setCatalog);
  const catalogStatus = useDesktopStore((state) => state.catalogStatus);
  const catalogError = useDesktopStore((state) => state.catalogError);
  const setCatalogLoading = useDesktopStore((state) => state.setCatalogLoading);
  const setCatalogFailure = useDesktopStore((state) => state.setCatalogFailure);
  const selected = useDesktopStore((state) => state.selected);
  const error = useDesktopStore((state) => state.error);
  const setError = useDesktopStore((state) => state.setError);
  const notices = useDesktopStore((state) => state.notices);
  const [catalogAttempt, setCatalogAttempt] = useState(0);
  const profileGeneration = useRef(0);

  useEffect(() => {
    const generation = ++profileGeneration.current;
    setCatalogLoading();
    api.profiles().then(async (availableProfiles) => {
      if (generation !== profileGeneration.current) return;
      if (availableProfiles.length === 0) throw new Error("No verified game profiles are available.");
      const currentState = useDesktopStore.getState();
      const retryProfile = currentState.profiles.length > 0
        ? currentState.request.profileId
        : null;
      setProfiles(availableProfiles);
      const stored = readStoredProfile();
      const preferred = retryProfile ?? stored;
      const initialProfile = preferred !== null && availableProfiles.some((entry) => entry.profile.id === preferred)
        ? preferred
        : availableProfiles.some((entry) => entry.profile.id === "vanilla")
          ? "vanilla"
          : availableProfiles[0].profile.id;
      await loadProfile(initialProfile, generation);
    }).catch((err) => {
      if (generation !== profileGeneration.current) return;
      setCatalogFailure(err instanceof Error ? err.message : String(err));
    });
    return () => {
      if (profileGeneration.current === generation) profileGeneration.current += 1;
    };
  }, [catalogAttempt, setCatalogFailure, setCatalogLoading, setProfiles]);

  async function loadProfile(nextProfileId: string, generation = ++profileGeneration.current) {
    const before = useDesktopStore.getState();
    const activeJobs = [
      before.activeJobId ? api.cancelSearch(before.activeJobId) : null,
      before.activePathJobId ? api.cancelPathPreview(before.activePathJobId) : null,
      before.activeAffinityJobId ? api.cancelAffinityWatch(before.activeAffinityJobId) : null,
    ].filter((job): job is Promise<boolean> => job !== null);
    beginProfileSwitch(nextProfileId);
    setAnalysisCacheVersion(`profile-switch:${nextProfileId}`);
    void Promise.allSettled(activeJobs);
    try {
      const catalog = await api.catalog(nextProfileId);
      if (
        generation !== profileGeneration.current ||
        useDesktopStore.getState().request.profileId !== nextProfileId
      ) return;
      setAnalysisCacheVersion(
        `${catalog.dataManifest.profile.id}:${catalog.dataManifest.schemaVersion}:${catalog.dataManifest.datasetVersion}:${catalog.dataManifest.modelVersion}`,
      );
      setCatalog(catalog);
      try {
        localStorage.setItem(PROFILE_STORAGE_KEY, nextProfileId);
      } catch {
        // Profile persistence is optional; the selected verified profile remains active.
      }
    } catch (err) {
      if (generation !== profileGeneration.current) return;
      setCatalogFailure(err instanceof Error ? err.message : String(err));
    }
  }

  function readStoredProfile(): string | null {
    try {
      return localStorage.getItem(PROFILE_STORAGE_KEY);
    } catch {
      return null;
    }
  }

  const activeProfile = profiles.find((entry) => entry.profile.id === profileId) ?? null;
  const profileReady = catalogStatus === "ready";
  const limitedAowModel = activeProfile && (!activeProfile.capabilities.aowDamage || !activeProfile.capabilities.aowRoutes);
  const convergenceProfile = activeProfile?.profile.id === "convergence";

  return (
    <main className="desktop-shell" aria-busy={catalogStatus === "loading"}>
      <div className="ambient-field" aria-hidden="true">
        <i />
        <i />
        <i />
      </div>
      <CommandRail />
      <section className="center-workspace">
        <header className="profile-bar">
          <div className="profile-bar-title">
            <Layers3 size={16} aria-hidden="true" />
            <span>Game profile</span>
          </div>
          <div className="profile-switch" role="radiogroup" aria-label="Game profile">
            {profiles.map((profile) => {
              const active = profile.profile.id === profileId;
              const version = profile.profile.modVersion ?? profile.profile.gameVersion;
              return (
                <button
                  key={profile.profile.id}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  className={active ? "active" : ""}
                  disabled={catalogStatus === "loading" && !active}
                  onClick={() => {
                    if (!active) void loadProfile(profile.profile.id);
                  }}
                >
                  <span>
                    {active ? <Check size={13} aria-hidden="true" /> : null}
                    {profile.profile.displayName}
                    {profile.profile.id === "convergence" ? (
                      <sup className="profile-beta-mark" aria-label="Beta">BETA</sup>
                    ) : null}
                  </span>
                  <small>{version}</small>
                </button>
              );
            })}
          </div>
          <div className={`profile-coverage ${profileReady ? (limitedAowModel ? "limited" : "complete") : "loading"}`} role="status">
            <strong>{profileReady ? (limitedAowModel ? "Weapon model ready" : "Full model ready") : "Loading profile…"}</strong>
            <span>
              {!profileReady
                ? "Loading and validating the selected data snapshot."
                : limitedAowModel
                ? `${convergenceProfile ? "Melee " : ""}AR, status, passives, and compatibility are verified. ${convergenceProfile ? "Ammo weapons and " : ""}AoW hit/route damage stay disabled until their data is mapped.`
                : "Weapon and Ash of War calculations are verified for this snapshot."}
            </span>
          </div>
        </header>
        <nav className="workspace-tabs">
          {tabs.map(({ id, label, icon: Icon }) => {
            const requiresSelection = id !== "rankings" && !selected;
            const disabled = catalogStatus !== "ready" || requiresSelection;
            return (
              <button
                key={id}
                className={`${activeWorkspace === id ? "active" : ""} ${requiresSelection ? "locked" : ""}`}
                type="button"
                onClick={() => setWorkspace(id)}
                title={requiresSelection ? `${label} requires a selected ranked build` : label}
                disabled={disabled}
              >
                <Icon size={16} />
                <span>{label}</span>
              </button>
            );
          })}
        </nav>
        {error ? (
          <div className="error-strip" role="alert">
            <CircleAlert size={16} />
            <span>{error}</span>
            <button type="button" onClick={() => setError(null)} aria-label="Dismiss error"><X size={14} /></button>
          </div>
        ) : null}
        {notices
          .filter((notice) => notice.scope === "global" || notice.scope === activeWorkspace)
          .slice(-2)
          .map((notice, index) => (
            <div className={`notice-strip ${notice.tone}`} key={`${notice.scope}-${index}-${notice.message}`}>
              <span>{notice.message}</span>
            </div>
          ))}
        {catalogStatus === "loading" ? (
          <div className="startup-state" role="status">
            <LoaderCircle className="spin" size={24} />
            <strong>Loading verified game data</strong>
            <span>Checking the snapshot manifest and preparing weapon filters.</span>
          </div>
        ) : null}
        {catalogStatus === "error" ? (
          <div className="startup-state error" role="alert">
            <CircleAlert size={24} />
            <strong>Game data could not be loaded</strong>
            <span>{catalogError}</span>
            <button type="button" onClick={() => setCatalogAttempt((attempt) => attempt + 1)}>
              <RotateCcw size={15} />Retry loading
            </button>
          </div>
        ) : null}
        <div className="workspace-stage" key={activeWorkspace}>
          {catalogStatus === "ready" && activeWorkspace === "rankings" ? <RankingsBoard /> : null}
          {catalogStatus === "ready" && activeWorkspace === "compare" ? <CompareView /> : null}
          {catalogStatus === "ready" && activeWorkspace === "paths" ? <PathsView /> : null}
          {catalogStatus === "ready" && activeWorkspace === "affinity_watch" ? <AffinityWatchView /> : null}
        </div>
      </section>
      <Inspector />
    </main>
  );
}
