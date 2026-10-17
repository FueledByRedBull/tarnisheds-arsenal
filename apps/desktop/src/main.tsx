import React from "react";
import ReactDOM from "react-dom/client";
// The base stylesheet loads first, so stylesheets imported by components come after it.
import "./styles.css";
import breakpoints from "./breakpoints.css?inline";
import { App } from "./app/App";
import { AppErrorBoundary } from "./app/AppErrorBoundary";
import { blockBrowserReload } from "./lib/keyboard";

// Width breakpoints get a sheet of their own, after every other: crossing one during a window
// resize then restyles only the elements they name instead of the whole page (5-6 ms).
const breakpointSheet = new CSSStyleSheet();
breakpointSheet.replaceSync(breakpoints);
document.adoptedStyleSheets = [...document.adoptedStyleSheets, breakpointSheet];

// Development keeps reload for hot-fix iteration; release builds keep the session (XC-001).
if (import.meta.env.PROD) window.addEventListener("keydown", blockBrowserReload, { capture: true });

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary><App /></AppErrorBoundary>
  </React.StrictMode>,
);
