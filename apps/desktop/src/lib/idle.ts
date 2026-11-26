export interface IdleTime {
  timeRemaining(): number;
}

/** Longest an idle callback waits for idle time before it runs anyway. */
const IDLE_TIMEOUT_MS = 2_000;

/**
 * Runs `callback` when the browser is idle and returns a function that cancels it. Idle
 * callbacks are missing in some webviews (WebKitGTK): there a timer runs it after
 * `fallbackDelayMs`, reporting `fallbackBudgetMs` of idle time.
 */
export function whenIdle(callback: (idle: IdleTime) => void, fallbackDelayMs: number, fallbackBudgetMs = 0): () => void {
  if ("requestIdleCallback" in window) {
    const id = window.requestIdleCallback(callback, { timeout: IDLE_TIMEOUT_MS });
    return () => window.cancelIdleCallback(id);
  }
  const id = globalThis.setTimeout(() => callback({ timeRemaining: () => fallbackBudgetMs }), fallbackDelayMs);
  return () => globalThis.clearTimeout(id);
}
