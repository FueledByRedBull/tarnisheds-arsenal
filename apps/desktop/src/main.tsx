import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./app/App";
import { AppErrorBoundary } from "./app/AppErrorBoundary";
import "./styles.css";
import "./features/shared/build-tools.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary><App /></AppErrorBoundary>
  </React.StrictMode>,
);
