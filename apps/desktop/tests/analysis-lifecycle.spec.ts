import { expect, test } from "@playwright/test";
import { openSection } from "./editors";

type AnalysisKind = "path" | "affinity";

for (const kind of ["path", "affinity"] as const) {
  test(`${kind} blocks stale selections in navigation and an already-open workspace`, async ({ page }) => {
    await prepareAnalysis(page, kind);
    await commitStatEdit(page);
    await expect(page.getByRole("navigation").getByRole("button", {
      name: kind === "path" ? "Paths" : "Affinity Watch", exact: true,
    })).toBeDisabled();
    await expect(page.getByText("Update Rankings before continuing").filter({ visible: true })).toBeVisible();
    await expect(page.getByRole("button", { name: kind === "path" ? "Trace paths" : "Watch affinities", exact: true })).toHaveCount(0);
  });

  test(`${kind} cancels a late native start when its selected ranking becomes stale`, async ({ page }) => {
    await prepareAnalysis(page, kind);
    await installLateStartProbe(page, kind);
    await page.getByRole("button", { name: kind === "path" ? "Trace paths" : "Watch affinities", exact: true }).click();
    await expect.poll(() => page.evaluate(() => (window as any).lateAnalysisProbe.started)).toBe(true);
    await commitStatEdit(page);
    await page.evaluate(() => (window as any).lateAnalysisProbe.resolveStart({ jobId: "stale-job" }));
    await expect.poll(() => page.evaluate(() => (window as any).lateAnalysisProbe.cancellations)).toContain("stale-job");
    await expect(page.getByText("Update Rankings before continuing").filter({ visible: true })).toBeVisible();
    expect(await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const state = useDesktopStore.getState();
      return { paths: state.paths, affinity: state.affinityPayload };
    })).toEqual({ paths: [], affinity: null });
  });
}

for (const kind of ["path", "affinity"] as const) {
  test(`${kind} run keeps going while its tab is hidden and lands on return`, async ({ page }) => {
    await prepareAnalysis(page, kind);
    await installHeldRunProbe(page, kind);
    await page.getByRole("button", { name: kind === "path" ? "Trace paths" : "Watch affinities", exact: true }).click();
    await expect(page.getByRole("button", { name: "Stop", exact: true })).toBeVisible();

    await page.keyboard.press("Control+1");
    await expect(page.getByRole("navigation").getByRole("button", { name: "Rankings", exact: true })).toHaveAttribute("aria-current", "page");
    await page.evaluate(() => { (window as any).hiddenRunProbe.release = true; });
    await expect.poll(() => page.evaluate(async (kind) => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const state = useDesktopStore.getState();
      return kind === "path" ? state.paths.length > 0 : state.affinityPayload !== null;
    }, kind)).toBe(true);

    await page.keyboard.press(kind === "path" ? "Control+3" : "Control+4");
    await expect(page.locator(".analysis-progress:visible")).toHaveAttribute("data-analysis-status", "completed");
    await expect(page.getByText(kind === "path" ? "Path preview stopped." : "Affinity watch stopped.")).toHaveCount(0);
    expect(await page.evaluate(() => (window as any).hiddenRunProbe.cancellations)).toEqual([]);
  });
}

for (const source of ["saved target", "ranked rival"] as const) {
  test(`path run survives a Compare visit that keeps the ${source}`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.keyboard.press("Control+2");
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    if (source === "saved target") {
      // A loaded saved build restores its comparison target before Compare is first shown.
      await openSection(page, "Saved Builds");
      await page.getByRole("button", { name: "Save new", exact: true }).click();
      await page.reload();
      await page.getByText("Snapshot loaded", { exact: true }).waitFor();
      await openSection(page, "Saved Builds");
      await page.getByRole("button", { name: "Load", exact: true }).click();
      await expect(page.getByText(/saved results verified on current data/)).toBeVisible();
    }
    await page.keyboard.press("Control+3");
    await expect(page.locator(".path-lane").nth(1)).not.toContainText("No compare lane selected.");
    await installHeldRunProbe(page, "path");
    await page.getByRole("button", { name: "Trace paths", exact: true }).click();
    await expect(page.getByRole("button", { name: "Stop", exact: true })).toBeVisible();

    await page.keyboard.press("Control+2");
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await page.keyboard.press("Control+3");
    await page.evaluate(() => { (window as any).hiddenRunProbe.release = true; });
    await expect(page.locator(".analysis-progress:visible")).toHaveAttribute("data-analysis-status", "completed");
    await expect(page.getByText("Path preview stopped.")).toHaveCount(0);
    expect(await page.evaluate(() => (window as any).hiddenRunProbe.cancellations)).toEqual([]);
  });
}

test("an abandoned numeric draft leaves results and analysis tabs current", async ({ page }) => {
  await prepareAnalysis(page, "path");
  const input = page.locator(".ribbon-stat input").first();
  const committed = await input.inputValue();
  await input.fill("");
  await input.blur();
  await expect(input).toHaveValue(committed);
  await expect(page.getByText("Inputs changed", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Trace paths", exact: true })).toBeEnabled();
  await expect(page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true })).toBeEnabled();
});

for (const kind of ["path", "affinity"] as const) {
  test(`${kind} adapter keeps native ownership after status and cancellation failures`, async ({ page }) => {
    await prepareAnalysis(page, kind);
    await installFailureProbe(page, kind);
    await page.getByRole("button", { name: kind === "path" ? "Trace paths" : "Watch affinities", exact: true }).click();

    await expect(page.locator('.error-strip[role="alert"]')).toContainText(`${kind} status IPC failed`);
    await expect.poll(() => page.evaluate(() => (window as any).analysisFailureProbe.cancellations)).toContain(`probe-${kind}-1`);
    await expect(page.getByRole("button", { name: kind === "path" ? "Trace paths" : "Watch affinities", exact: true })).toBeEnabled();

    await page.getByRole("button", { name: kind === "path" ? "Trace paths" : "Watch affinities", exact: true }).click();
    await expect.poll(() => page.evaluate(() => (window as any).analysisFailureProbe.starts.length)).toBe(1);

    await page.evaluate(() => { (window as any).analysisFailureProbe.oldTerminal = true; });
    await expect.poll(() => page.evaluate(() => (window as any).analysisFailureProbe.starts.length)).toBe(2);
    await expect.poll(() => page.locator(".analysis-progress:visible").getAttribute("data-analysis-status")).toBe("completed");
    expect(await page.evaluate(() => (window as any).analysisFailureProbe.cancellations)).toEqual([`probe-${kind}-1`]);
  });
}

for (const kind of ["path", "affinity"] as const) {
  test(`${kind} adapter cancels a late native start after profile invalidation`, async ({ page }) => {
    await prepareAnalysis(page, kind);
    await installLateStartProbe(page, kind);
    await page.getByRole("button", { name: kind === "path" ? "Trace paths" : "Watch affinities", exact: true }).click();
    await expect.poll(() => page.evaluate(() => (window as any).lateAnalysisProbe.started)).toBe(true);

    await page.getByRole("radio", { name: /Convergence/ }).click();
    await expect(page.getByRole("radio", { name: /Convergence/ })).toHaveAttribute("aria-checked", "true");
    await page.evaluate(() => (window as any).lateAnalysisProbe.resolveStart({ jobId: "late-job" }));

    await expect.poll(() => page.evaluate(() => (window as any).lateAnalysisProbe.cancellations)).toContain("late-job");
    expect(await page.evaluate(() => (window as any).lateAnalysisProbe.cancellations)).toEqual(["late-job"]);
  });
}

// Only a committed value changes the query; an abandoned draft leaves results current.
async function commitStatEdit(page: import("@playwright/test").Page) {
  const input = page.locator(".ribbon-stat input").first();
  await input.fill("20");
  await input.press("Enter");
}

async function prepareAnalysis(page: import("@playwright/test").Page, kind: AnalysisKind) {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.getByRole("navigation").getByRole("button", {
    name: kind === "path" ? "Paths" : "Affinity Watch",
    exact: true,
  }).click();
}

// Holds a run open until the test releases it, so a tab switch happens mid-run.
async function installHeldRunProbe(page: import("@playwright/test").Page, kind: AnalysisKind) {
  await page.evaluate(async (kind) => {
    const { api } = await import("/src/lib/api.ts");
    const names = kind === "path"
      ? { status: "pathPreviewStatus", cancel: "cancelPathPreview" } as const
      : { status: "affinityWatchStatus", cancel: "cancelAffinityWatch" } as const;
    const probe = { release: false, cancellations: [] as string[] };
    const status = api[names.status];
    const cancel = api[names.cancel];
    (api as any)[names.status] = (jobId: string) => probe.release ? status(jobId) : Promise.resolve({ progress: null, finished: null });
    (api as any)[names.cancel] = (jobId: string) => { probe.cancellations.push(jobId); return cancel(jobId); };
    Object.assign(window, { hiddenRunProbe: probe });
  }, kind);
}

async function installFailureProbe(page: import("@playwright/test").Page, kind: AnalysisKind) {
  await page.evaluate(async (kind) => {
    const { api } = await import("/src/lib/api.ts");
    const names = kind === "path"
      ? { start: "startPathPreview", status: "pathPreviewStatus", cancel: "cancelPathPreview" }
      : { start: "startAffinityWatch", status: "affinityWatchStatus", cancel: "cancelAffinityWatch" };
    const probe = {
      starts: [] as string[],
      statuses: [] as string[],
      cancellations: [] as string[],
      failStatus: true,
      rejectCancel: true,
      oldTerminal: false,
    };
    const finished = (jobId: string, cancelled: boolean) => kind === "path"
      ? { jobId, cancelled, paths: [], error: null }
      : { jobId, cancelled, payload: { lines: [], breakpoints: [] }, error: null };

    (api as any)[names.start] = async () => {
      const jobId = `probe-${kind}-${probe.starts.length + 1}`;
      probe.starts.push(jobId);
      return { jobId };
    };
    (api as any)[names.cancel] = async (jobId: string) => {
      probe.cancellations.push(jobId);
      if (probe.rejectCancel) {
        probe.rejectCancel = false;
        throw new Error(`${kind} cancel IPC failed`);
      }
      return true;
    };
    (api as any)[names.status] = async (jobId: string) => {
      probe.statuses.push(jobId);
      if (jobId === probe.starts[0] && probe.failStatus) {
        probe.failStatus = false;
        throw new Error(`${kind} status IPC failed`);
      }
      if (jobId === probe.starts[0] && !probe.oldTerminal) return { progress: null, finished: null };
      return { progress: null, finished: finished(jobId, jobId === probe.starts[0]) };
    };
    Object.assign(window, { analysisFailureProbe: probe });
  }, kind);
}

async function installLateStartProbe(page: import("@playwright/test").Page, kind: AnalysisKind) {
  await page.evaluate(async (kind) => {
    const { api } = await import("/src/lib/api.ts");
    const names = kind === "path"
      ? { start: "startPathPreview", status: "pathPreviewStatus", cancel: "cancelPathPreview" }
      : { start: "startAffinityWatch", status: "affinityWatchStatus", cancel: "cancelAffinityWatch" };
    const probe: {
      started: boolean;
      cancellations: string[];
      resolveStart: (response: { jobId: string }) => void;
    } = { started: false, cancellations: [], resolveStart: () => undefined };
    const finished = (jobId: string) => kind === "path"
      ? { jobId, cancelled: true, paths: [], error: null }
      : { jobId, cancelled: true, payload: null, error: null };

    (api as any)[names.start] = () => {
      probe.started = true;
      return new Promise<{ jobId: string }>((resolve) => { probe.resolveStart = resolve; });
    };
    (api as any)[names.cancel] = async (jobId: string) => {
      probe.cancellations.push(jobId);
      return true;
    };
    (api as any)[names.status] = async (jobId: string) => ({
      progress: null,
      finished: finished(jobId),
    });
    Object.assign(window, { lateAnalysisProbe: probe });
  }, kind);
}
