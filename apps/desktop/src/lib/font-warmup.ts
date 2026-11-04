// Bahnschrift is a variable system font. Each weight, size and numeric-variant combination the
// page first lays out loads its own instance, which landed inside the first Search, the first
// Compare and the first Ctrl+K (6 to 8 ms of one task each). Measuring the combinations the
// stylesheet uses in an off-screen element while the app is idle moves that cost out of them.

// Weights and sizes come from styles.css: font-weight 400 to 800, and the --text-* sizes that
// text in Bahnschrift uses (label 12, note 13, body 14, emphasis 16, title 20, heading 22).
const WEIGHTS = [400, 500, 600, 700, 800] as const;
const SIZES = [12, 13, 14, 16, 20, 22] as const;
const SAMPLE = "0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz +-./:%,()";

/** Longest an idle callback spends measuring before it yields. */
export const SLICE_BUDGET_MS = 3;

export interface WarmupStep {
  weight: number;
  size: number;
  /** Numeric columns use tabular figures, which select another set of glyphs. */
  tabular: boolean;
}

export function warmupSteps(): WarmupStep[] {
  return WEIGHTS.flatMap((weight) => SIZES.flatMap((size) => [false, true].map((tabular) => ({ weight, size, tabular }))));
}

/**
 * Runs steps from `from` until the budget is spent and returns the index of the next one. At
 * least one step runs, so a callback that is handed no idle time still makes progress.
 */
export function runSlice(
  steps: readonly WarmupStep[],
  from: number,
  run: (step: WarmupStep) => void,
  budgetMs: number,
  now: () => number = () => performance.now(),
): number {
  const started = now();
  let index = from;
  while (index < steps.length) {
    run(steps[index]);
    index += 1;
    if (now() - started >= budgetMs) break;
  }
  return index;
}

interface IdleDeadline {
  timeRemaining(): number;
}

// Idle callbacks are missing in some webviews (WebKitGTK): a timer stands in, as in QueryStrip.
function whenIdle(callback: (deadline: IdleDeadline) => void): () => void {
  if ("requestIdleCallback" in window) {
    const id = window.requestIdleCallback(callback, { timeout: 2_000 });
    return () => window.cancelIdleCallback(id);
  }
  const id = globalThis.setTimeout(() => callback({ timeRemaining: () => SLICE_BUDGET_MS }), 100);
  return () => globalThis.clearTimeout(id);
}

let finished = false;
let nextStep = 0;

/** Starts measuring in idle time, continuing where an earlier call stopped; the result cancels. */
export function warmFonts(): () => void {
  if (finished) return () => {};
  const steps = warmupSteps();
  const host = document.createElement("div");
  host.setAttribute("aria-hidden", "true");
  host.style.cssText = "position:absolute;left:-9999px;top:0;width:2000px;height:10px;overflow:hidden;visibility:hidden;contain:strict";
  let cancelIdle = () => {};
  let stopped = false;

  const measure = ({ weight, size, tabular }: WarmupStep) => {
    const probe = document.createElement("span");
    probe.textContent = SAMPLE;
    probe.style.cssText = `display:block;font-weight:${weight};font-size:${size}px;${tabular ? "font-variant-numeric:tabular-nums" : ""}`;
    host.append(probe);
    // Reading the height lays the probe out, which is what loads the instance.
    void probe.offsetHeight;
    probe.remove();
  };

  const step = (deadline: IdleDeadline) => {
    if (stopped) return;
    const budget = Math.min(SLICE_BUDGET_MS, deadline.timeRemaining());
    if (!host.isConnected) document.body.append(host);
    nextStep = runSlice(steps, nextStep, measure, budget);
    if (nextStep < steps.length) {
      cancelIdle = whenIdle(step);
    } else {
      finished = true;
      host.remove();
    }
  };

  cancelIdle = whenIdle(step);
  return () => {
    stopped = true;
    cancelIdle();
    host.remove();
  };
}
