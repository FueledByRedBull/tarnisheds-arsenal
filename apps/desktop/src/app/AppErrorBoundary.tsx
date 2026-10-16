import { Component, type ReactNode } from "react";
import { errorMessage } from "../lib/format";
import { clearSavedQueries } from "../lib/state";

// The page boundary offers a fresh workspace; a panel boundary (Build Detail, a workspace) keeps
// the rest of the app usable and lets that panel try again. Both show the message, selectable,
// so a report can quote it.
export class AppErrorBoundary extends Component<{ children: ReactNode; panel?: string }, { error: unknown }> {
  state = { error: null as unknown };

  static getDerivedStateFromError(error: unknown) {
    return { error: error ?? new Error("Unknown error") };
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
