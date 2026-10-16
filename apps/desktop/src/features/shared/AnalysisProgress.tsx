import { analysisStatus, analysisStatusLabel } from "../../lib/analysis-status";

// The live region names the state only; the changing count beside it is not announced on
// every poll.
export function AnalysisProgress({ checked, total, status, running, completed }: {
  checked: number;
  total: number;
  status: ReturnType<typeof analysisStatus>;
  running: string;
  completed: string;
}) {
  const displayChecked = status === "completed" ? total : checked;
  const pct = Math.min(100, Math.max(0, (displayChecked / Math.max(total, 1)) * 100));
  return (
    <div className={`workspace-progress analysis-progress status-${status}`} data-analysis-status={status}>
      <span>
        <span role="status">{status === "running" ? running : status === "completed" ? completed : analysisStatusLabel(status)}</span>
        {status === "running" ? <span aria-hidden="true"> {checked}/{total}</span> : null}
      </span>
      <div className="forge-bar"><i style={{ transform: `translateX(${pct - 100}%)` }} /></div>
    </div>
  );
}
