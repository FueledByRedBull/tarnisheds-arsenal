import { useEffect, useState } from "react";
import { CircleAlert, GitCompareArrows, LoaderCircle, Radar, RotateCcw, Route, Table2, X } from "lucide-react";
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

export function App() {
  const activeWorkspace = useDesktopStore((state) => state.activeWorkspace);
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
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

  useEffect(() => {
    let active = true;
    setCatalogLoading();
    api
      .catalog()
      .then((catalog) => {
        if (!active) return;
        setAnalysisCacheVersion(
          `${catalog.dataManifest.schemaVersion}:${catalog.dataManifest.datasetVersion}:${catalog.dataManifest.modelVersion}`,
        );
        setCatalog(catalog);
      })
      .catch((err) => {
        if (!active) return;
        setCatalogFailure(err instanceof Error ? err.message : String(err));
      });
    return () => {
      active = false;
    };
  }, [catalogAttempt, setCatalog, setCatalogFailure, setCatalogLoading]);

  return (
    <main className="desktop-shell" aria-busy={catalogStatus === "loading"}>
      <div className="ambient-field" aria-hidden="true">
        <i />
        <i />
        <i />
      </div>
      <CommandRail />
      <section className="center-workspace">
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
