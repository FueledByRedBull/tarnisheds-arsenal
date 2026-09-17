import { expect, test } from "@playwright/test";

type AnalysisKind = "path" | "affinity";

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
    await expect.poll(() => page.locator(".analysis-progress").getAttribute("data-analysis-status")).toBe("completed");
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

async function prepareAnalysis(page: import("@playwright/test").Page, kind: AnalysisKind) {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.getByRole("navigation").getByRole("button", {
    name: kind === "path" ? "Paths" : "Affinity Watch",
    exact: true,
  }).click();
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
