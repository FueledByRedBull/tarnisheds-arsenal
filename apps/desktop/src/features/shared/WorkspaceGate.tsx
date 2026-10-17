import { useDesktopStore } from "../../lib/state";

// Compare, Paths and Affinity Watch need a current selected ranking; this says so and leads back.
export function WorkspaceGate({ panelClass, title, requirement, heading, message }: {
  panelClass: string;
  title: string;
  requirement: string;
  heading: string;
  message: string;
}) {
  const setWorkspace = useDesktopStore((state) => state.setWorkspace);
  return (
    <section className={`workspace-panel ${panelClass}`}>
      <div className="workspace-header"><div><h1>{title}</h1><span>{requirement}</span></div></div>
      <div className="empty-state workspace-prerequisite">
        <strong>{heading}</strong>
        <span>{message}</span>
        <button type="button" onClick={() => setWorkspace("rankings")}>Go to Rankings</button>
      </div>
    </section>
  );
}
