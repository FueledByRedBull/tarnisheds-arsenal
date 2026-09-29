import { Component, type ReactNode } from "react";

export class AppErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render() {
    if (this.state.failed) {
      return <main role="alert">
        <h1>The workspace could not be displayed</h1>
        <p>Reload to return to a fresh workspace. Saved builds remain on this device.</p>
        <button type="button" onClick={() => window.location.reload()}>Reload workspace</button>
      </main>;
    }
    return this.props.children;
  }
}
