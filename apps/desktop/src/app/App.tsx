import { Activity, useCallback, useEffect, useEffectEvent, useRef, useState } from "react";
import { Check, CircleAlert, GitCompareArrows, Radar, RotateCcw, Route, Table2, X } from "lucide-react";
import { api } from "../lib/api";
import { setAnalysisCacheVersion } from "../lib/analysis-cache";
import { keyboardOwnedByOverlay } from "../lib/keyboard";
import { useDesktopStore } from "../lib/state";
import { WorkspaceTab } from "../lib/types";
import { AffinityWatchView } from "../features/affinity-watch/AffinityWatchView";
import { QueryStrip } from "../features/query-strip/QueryStrip";
import { Popover } from "../features/shared/Popover";
import { CompareView } from "../features/compare/CompareView";
import { Inspector } from "../features/inspector/Inspector";
import { PathsView } from "../features/paths/PathsView";
import { SkeletonRows } from "../features/shared/SkeletonRows";
import { EASE_OUT, reducedMotion } from "../lib/motion";
import { RankingsBoard } from "../features/rankings/RankingsBoard";
import { ReproductionReport } from "../features/shared/ReproductionReport";

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
  const resultsStale = useDesktopStore((state) => state.resultsStale);
  const error = useDesktopStore((state) => state.error);
  const setError = useDesktopStore((state) => state.setError);
  const notices = useDesktopStore((state) => state.notices);
  const [catalogAttempt, setCatalogAttempt] = useState(0);
  const profileGeneration = useRef(0);

  const loadProfile = useCallback(async (nextProfileId: string, generation = ++profileGeneration.current) => {
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
  }, [beginProfileSwitch, setCatalog, setCatalogFailure]);

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
  }, [catalogAttempt, loadProfile, setCatalogFailure, setCatalogLoading, setProfiles]);

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

  const profileSwitch = (
          <div className="profile-switch" role="radiogroup" aria-label="Game profile" onKeyDown={(event) => {
            const direction = ["ArrowRight", "ArrowDown"].includes(event.key) ? 1
              : ["ArrowLeft", "ArrowUp"].includes(event.key) ? -1 : 0;
            if (!direction || catalogStatus === "loading") return;
            event.preventDefault();
            const index = (profiles.findIndex((profile) => profile.profile.id === profileId) + direction + profiles.length) % profiles.length;
            event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="radio"]')[index]?.focus();
            void loadProfile(profiles[index].profile.id);
          }}>
            {profiles.map((profile) => {
              const active = profile.profile.id === profileId;
              const version = profile.profile.modVersion ?? profile.profile.gameVersion;
              return (
                <button
                  key={profile.profile.id}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  tabIndex={active ? 0 : -1}
                  className={active ? "active" : ""}
                  disabled={catalogStatus === "loading" && !active}
                  onClick={() => {
                    if (!active) void loadProfile(profile.profile.id);
                  }}
                >
                  <span>
                    {active ? <Check size={13} aria-hidden="true" /> : null}
                    {profile.profile.displayName}
                    {profile.profile.id === "convergence" ? <span className="sr-only"> Beta</span> : null}
                  </span>
                  <small>
                    {version}
                    {profile.profile.id === "convergence" ? <span className="profile-beta-mark" aria-hidden="true">Beta</span> : null}
                  </small>
                </button>
              );
            })}
          </div>
  );
  const coverageStatus = profileReady ? (limitedAowModel ? "Experimental fixed-stat model" : "Snapshot loaded") : "Loading profile…";
  const profileCoverage = (
    <div
      className={`profile-coverage ${profileReady ? (limitedAowModel ? "limited" : "complete") : "loading"}`}
      role="status"
    >
      <Popover
        label="Model coverage and assumptions"
        trigger={<strong>{coverageStatus}</strong>}
        triggerLabel={`${coverageStatus}. Model coverage and assumptions`}
        triggerTitle={profileReady ? "Validated snapshot loaded; available calculations follow its declared capabilities." : "Loading and validating the selected data snapshot."}
        triggerClassName="coverage-token"
        panelClassName="editor-panel coverage-panel"
        disabled={!profileReady}
      >
        <div className="editor-body">
          <strong>Model coverage and assumptions</strong>
          <p className="editor-note">
            {limitedAowModel
              ? `${convergenceProfile ? "Convergence " : ""}base weapon fields are reference-checked; final AR and customization remain experimental. Enter exact stats: class budgets, Compare, Paths, and Affinity Watch are unavailable. Ammo weapons and AoW hit/route damage remain unsupported.`
              : "Weapon AR, status buildup, and supported Ash routes are available. Profile capabilities do not guarantee every skill is modeled; check the selected build's model coverage. Buildup is not a prediction of status procs or damage after enemy defenses."}
          </p>
        </div>
      </Popover>
    </div>
  );

  const tabState = (id: WorkspaceTab) => {
    const requiresSelection = id !== "rankings" && (!selected || resultsStale);
    const unsupportedBudget = id !== "rankings" && activeProfile?.capabilities.classBudget === false;
    return { requiresSelection, unsupportedBudget, disabled: catalogStatus !== "ready" || requiresSelection || unsupportedBudget };
  };

  // Ctrl+1 to Ctrl+4 follow the tab order, and only reach tabs that are currently available.
  const onTabShortcut = useEffectEvent((event: KeyboardEvent) => {
    const index = Number(event.key) - 1;
    if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey || !tabs[index]) return;
    if (keyboardOwnedByOverlay()) return;
    event.preventDefault();
    if (!tabState(tabs[index].id).disabled) setWorkspace(tabs[index].id);
  });

  // One pill slides to the active tab, so a workspace switch reads as movement along the bar.
  // Its size is set once per switch and the slide is a transform, so no frame lays out. It is
  // measured in the next frame, reusing that frame's layout: measuring during the commit would
  // lay out the new workspace twice once its effects change it.
  const tabBar = useRef<HTMLElement>(null);
  const tabPill = useRef<HTMLSpanElement>(null);
  const pillPlaced = useRef(false);
  useEffect(() => {
    const bar = tabBar.current;
    const pill = tabPill.current;
    if (!bar || !pill) return;
    const place = (slide: boolean) => {
      const active = bar.querySelector<HTMLElement>('button[aria-current="page"]');
      pill.style.opacity = active ? "1" : "0";
      if (!active) return;
      // Where the pill is drawn now, mid-slide included.
      const from = pill.getBoundingClientRect();
      const left = from.left - bar.getBoundingClientRect().left - bar.clientLeft;
      pill.style.width = `${active.offsetWidth}px`;
      pill.style.transform = `translateX(${active.offsetLeft}px)`;
      if (slide && pillPlaced.current && from.width > 0 && !reducedMotion()) {
        pill.getAnimations().forEach((animation) => animation.cancel());
        pill.animate(
          [{ transform: `translateX(${left}px) scaleX(${from.width / active.offsetWidth})` }, { transform: `translateX(${active.offsetLeft}px)` }],
          { duration: 300, easing: EASE_OUT },
        );
      }
      pillPlaced.current = true;
    };
    const frame = requestAnimationFrame(() => place(true));
    const observer = new ResizeObserver(() => place(false));
    observer.observe(bar);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [activeWorkspace, catalogStatus]);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => onTabShortcut(event);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  return (
    <main className="desktop-shell" aria-busy={catalogStatus === "loading"}>
      <QueryStrip profile={profileSwitch} coverage={profileCoverage} onProfileChange={(id) => void loadProfile(id)} />
      <section className="center-workspace">
        <nav className="workspace-tabs" ref={tabBar}>
          <span className="tab-pill" ref={tabPill} aria-hidden="true" />
          {tabs.map(({ id, label, icon: Icon }, index) => {
            const { requiresSelection, unsupportedBudget, disabled } = tabState(id);
            return (
              <button
                key={id}
                className={`${activeWorkspace === id ? "active" : ""} ${requiresSelection ? "locked" : ""}`}
                type="button"
                aria-label={label}
                aria-current={activeWorkspace === id ? "page" : undefined}
                aria-keyshortcuts={`Control+${index + 1}`}
                onClick={() => setWorkspace(id)}
                title={unsupportedBudget ? "Requires verified profile class budgets" : requiresSelection ? `${label} requires a current selected ranking` : `${label} (Ctrl+${index + 1})`}
                disabled={disabled}
              >
                <Icon size={16} aria-hidden="true" />
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
        {catalogStatus === "error" ? (
          <div className="startup-state error" role="alert">
            <CircleAlert size={24} />
            <strong>Game data could not be loaded</strong>
            <span>{catalogError}</span>
            <ReproductionReport />
            <button type="button" onClick={() => setCatalogAttempt((attempt) => attempt + 1)}>
              <RotateCcw size={15} />Retry loading
            </button>
          </div>
        ) : null}
        <div className="workspace-stage">
          {catalogStatus === "loading" ? (
            <div className="workspace-panel startup-skeleton" role="status">
              <div className="workspace-header">
                <div>
                  <strong>Loading verified game data</strong>
                  <span>Checking the snapshot manifest and preparing weapon filters.</span>
                  <span className="forge-bar indeterminate" aria-hidden="true"><i /></span>
                </div>
              </div>
              <SkeletonRows count={9} />
            </div>
          ) : null}
          {/* Workspaces stay mounted while hidden, so switching tabs shows them instantly; a
              hidden workspace's effects stop, exactly as if it had unmounted. */}
          {catalogStatus === "ready" ? (
            <>
              <Activity mode={activeWorkspace === "rankings" ? "visible" : "hidden"}><RankingsBoard /></Activity>
              <Activity mode={activeWorkspace === "compare" ? "visible" : "hidden"}><CompareView /></Activity>
              <Activity mode={activeWorkspace === "paths" ? "visible" : "hidden"}><PathsView /></Activity>
              <Activity mode={activeWorkspace === "affinity_watch" ? "visible" : "hidden"}><AffinityWatchView /></Activity>
            </>
          ) : null}
        </div>
      </section>
      <Inspector />
    </main>
  );
}
