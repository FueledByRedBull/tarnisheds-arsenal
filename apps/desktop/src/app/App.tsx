import { useEffect } from "react";
import { CircleAlert, GitCompareArrows, Radar, Route, Table2 } from "lucide-react";
import { api } from "../lib/api";
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
  const error = useDesktopStore((state) => state.error);
  const setError = useDesktopStore((state) => state.setError);
  const notices = useDesktopStore((state) => state.notices);

  useEffect(() => {
    api
      .catalog()
      .then(setCatalog)
      .catch((err) => setError(err instanceof Error ? err.message : String(err)));
  }, [setCatalog, setError]);

  return (
    <main className="desktop-shell">
      <CommandRail />
      <section className="center-workspace">
        <nav className="workspace-tabs">
          {tabs.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              className={activeWorkspace === id ? "active" : ""}
              type="button"
              onClick={() => setWorkspace(id)}
              title={label}
            >
              <Icon size={16} />
              <span>{label}</span>
            </button>
          ))}
        </nav>
        {error ? (
          <div className="error-strip">
            <CircleAlert size={16} />
            <span>{error}</span>
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
        {activeWorkspace === "rankings" ? <RankingsBoard /> : null}
        {activeWorkspace === "compare" ? <CompareView /> : null}
        {activeWorkspace === "paths" ? <PathsView /> : null}
        {activeWorkspace === "affinity_watch" ? <AffinityWatchView /> : null}
      </section>
      <Inspector />
    </main>
  );
}
