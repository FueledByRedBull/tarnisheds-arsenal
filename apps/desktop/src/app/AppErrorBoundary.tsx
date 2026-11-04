import { Component, type ErrorInfo, type ReactNode } from "react";
import { errorMessage } from "../lib/format";
import { clearSavedQueries } from "../lib/state";

// The page boundary offers a fresh workspace; a panel boundary (Build Detail, a workspace) keeps
// the rest of the app usable and lets that panel try again. Both show the message, selectable,
// so a report can quote it. A panel boundary also tries again by itself when its resetKey changes,
// so moving to another workspace, profile or build leaves a crashed panel behind; it is not
// remounted otherwise, because that would drop the state of workspaces kept alive while hidden.
export class AppErrorBoundary extends Component<{ children: ReactNode; panel?: string; resetKey?: string }, { error: unknown }> {
  state = { error: null as unknown };

  static getDerivedStateFromError(error: unknown) {
    return { error: error ?? new Error("Unknown error") };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  componentDidUpdate(previous: { resetKey?: string }) {
    if (this.state.error !== null && previous.resetKey !== this.props.resetKey) this.setState({ error: null });
  }

  render() {
    const { error } = this.state;
    if (error === null) return this.props.children;
    const { panel } = this.props;
    return (
      <div className={panel ? "panel-failure" : "page-failure"} role="alert">
        <h1>{panel ? `${panel} could not be displayed` : "The workspace could not be displayed"}</h1>
        <p>{panel ? "The rest of the app still works. Try again, or report the message below." : "Reload to return to a fresh workspace. Saved builds remain on this device."}</p>
        <pre>{errorMessage(error)}</pre>
        {panel
          ? <button type="button" onClick={() => this.setState({ error: null })}>Try again</button>
          : <button type="button" onClick={() => { clearSavedQueries(); window.location.reload(); }}>Reload workspace</button>}
      </div>
    );
  }
}
