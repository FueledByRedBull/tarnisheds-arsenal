import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./app/App";
import { AppErrorBoundary } from "./app/AppErrorBoundary";
import { blockBrowserReload } from "./lib/keyboard";
import "./styles.css";
import "./features/shared/build-tools.css";

// Development keeps reload for hot-fix iteration; release builds keep the session (XC-001).
if (import.meta.env.PROD) window.addEventListener("keydown", blockBrowserReload, { capture: true });

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary><App /></AppErrorBoundary>
  </React.StrictMode>,
);
