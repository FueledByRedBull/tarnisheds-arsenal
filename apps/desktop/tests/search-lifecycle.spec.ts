import { expect, test } from "@playwright/test";

for (const outcome of ["already-finished", "failure"] as const) {
  test(`a late ${outcome} cancellation reply cannot change a replacement search`, async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
    await page.evaluate(async () => {
      const { api } = await import("/src/lib/api.ts");
      const probe = {
        starts: 0, oldFinished: false, cancellationHeld: false,
        resolveCancel: (_value: boolean) => {}, rejectCancel: (_error: Error) => {},
      };
      Object.assign(window, { delayedCancelProbe: probe });
      api.startSearch = async () => ({ jobId: `search-${++probe.starts}` });
      api.cancelSearch = async () => {
        if (probe.cancellationHeld) return true;
        probe.cancellationHeld = true;
        return new Promise<boolean>((resolve, reject) => {
          probe.resolveCancel = resolve;
          probe.rejectCancel = reject;
        });
      };
      api.searchStatus = async (jobId: string) => ({
        progress: null,
        finished: jobId === "search-1" && probe.oldFinished
          ? { jobId, rows: [], cancelled: true, error: null } : null,
      });
    });
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect.poll(() => page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      return useDesktopStore.getState().activeJobId;
    })).toBe("search-1");
    await page.getByRole("button", { name: "Cancel Search", exact: true }).click();
    await expect(page.getByRole("button", { name: "Cancelling...", exact: true })).toBeDisabled();

    await page.getByRole("spinbutton", { name: "VIG", exact: true }).fill("13");
    await page.getByRole("spinbutton", { name: "VIG", exact: true }).press("Enter");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await page.evaluate(() => { (window as any).delayedCancelProbe.oldFinished = true; });
    await expect.poll(() => page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      return useDesktopStore.getState().activeJobId;
    })).toBe("search-2");

    await page.evaluate(outcome => {
      const probe = (window as any).delayedCancelProbe;
      if (outcome === "already-finished") probe.resolveCancel(false);
      else probe.rejectCancel(new Error("obsolete cancellation failed"));
    }, outcome);
    await expect(page.getByRole("button", { name: "Cancel Search", exact: true })).toBeEnabled();
    await expect(page.locator('.error-strip[role="alert"]')).toHaveCount(0);
    expect(await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const state = useDesktopStore.getState();
      return { searching: state.isSearching, jobId: state.activeJobId };
    })).toEqual({ searching: true, jobId: "search-2" });
  });
}

for (const replacement of ["compare", "rankings"]) {
  test(`changing Compare waits for cancellation before starting ${replacement}`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.evaluate(async () => {
      const apiPath = "/src/lib/api.ts";
      const statePath = "/src/lib/state.ts";
      const { api } = await import(apiPath);
      const { useDesktopStore } = await import(statePath);
      const rows = useDesktopStore.getState().rows;
      const probe = { starts: 0, cancelled: false, release: false, active: "" };
      Object.assign(window, { searchProbe: probe });
      api.startSearch = async () => {
        if (probe.active) throw new Error("search job is already running");
        probe.active = `probe-${++probe.starts}`;
        return { jobId: probe.active };
      };
      api.cancelSearch = async () => { probe.cancelled = true; return true; };
      api.searchStatus = async (jobId: string) => {
        if (jobId === "probe-1" && !probe.release) return { progress: null, finished: null };
        probe.active = "";
        return { progress: null, finished: { jobId, rows, cancelled: jobId === "probe-1", error: null } };
      };
    });
    const nav = page.getByRole("navigation");
    await nav.getByRole("button", { name: "Compare", exact: true }).click();
    await page.getByRole("combobox", { name: "Compare Weapon", exact: true }).click();
    await page.getByRole("option", { name: "Zweihander", exact: true }).click();
    await page.waitForFunction(() => (window as any).searchProbe.starts === 1);
    if (replacement === "compare") {
      await page.getByRole("combobox", { name: "Compare Weapon", exact: true }).fill("Uchigatana");
      await page.getByRole("combobox", { name: "Compare Weapon", exact: true }).press("Enter");
    } else {
      await nav.getByRole("button", { name: "Rankings", exact: true }).click();
      await page.getByRole("button", { name: "Search", exact: true }).click();
    }
    await page.waitForFunction(() => (window as any).searchProbe.cancelled);
    expect(await page.evaluate(() => (window as any).searchProbe.starts)).toBe(1);
    await expect(page.locator('.error-strip[role="alert"]')).toHaveCount(0);
    await page.evaluate(() => { (window as any).searchProbe.release = true; });
    if (replacement === "compare") {
      await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
      await expect(page.getByRole("combobox", { name: "Compare Weapon", exact: true })).toHaveValue("Uchigatana");
    } else {
      await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
    }
    expect(await page.evaluate(() => (window as any).searchProbe.starts)).toBe(2);
    await expect(page.locator('.error-strip[role="alert"]')).toHaveCount(0);
  });
}

test("Compare solve failure aborts its sibling lanes and keeps shared work alive", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const { cachedSolveBuild } = await import("/src/lib/analysis-cache.ts");
    const { buildOptimizeRequest } = await import("/src/lib/session.ts");
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const state = useDesktopStore.getState();
    const rows = state.rows;
    const base = buildOptimizeRequest(state.catalog, state.request, state.lockedStatMode);
    const probe: {
      calls: Array<{ signal: AbortSignal; resolve: (row: typeof rows[number] | null) => void; aborted: boolean }>;
      heldResolved: boolean;
      siblingAborted: boolean;
    } = { calls: [], heldResolved: false, siblingAborted: false };
    let failNextRoot = true;
    api.solveBuild = (_base, _weaponName, _affinity, _aowName, signal) => {
      const index = probe.calls.length;
      const isRootFailure = index > 0 && failNextRoot;
      if (isRootFailure) failNextRoot = false;
      let resolveCall!: (row: typeof rows[number] | null) => void;
      const call = { signal, resolve: (row: typeof rows[number] | null) => resolveCall(row), aborted: false };
      probe.calls.push(call);
      return new Promise<typeof rows[number] | null>((resolve, reject) => {
        resolveCall = resolve;
        signal.addEventListener("abort", () => {
          call.aborted = true;
          if (!isRootFailure) {
            probe.siblingAborted = true;
            failNextRoot = true;
          }
          reject(new Error("cancelled"));
        }, { once: true });
        if (isRootFailure) reject(new Error("comparison solve failed"));
      });
    };
    const held = cachedSolveBuild(base, rows[2].weaponName, rows[2].affinity, rows[2].aowName);
    void held.then(() => { probe.heldResolved = true; }, () => undefined);
    state.toggleCompareBench(rows[1]);
    state.toggleCompareBench(rows[2]);
    state.toggleCompareBench(rows[3]);
    Object.assign(window, { compareSolveProbe: probe, resolveHeldCompareSolve: () => probe.calls[0]?.resolve(rows[2]) });
  });
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.locator('.error-strip[role="alert"]')).toContainText("comparison solve failed");
  await expect.poll(() => page.evaluate(() => (window as any).compareSolveProbe.calls.length)).toBeGreaterThanOrEqual(3);
  await expect.poll(() => page.evaluate(() => (window as any).compareSolveProbe.siblingAborted)).toBe(true);
  expect(await page.evaluate(() => (window as any).compareSolveProbe.calls[0].aborted)).toBe(false);
  await page.evaluate(() => (window as any).resolveHeldCompareSolve());
  await expect.poll(() => page.evaluate(() => (window as any).compareSolveProbe.heldResolved)).toBe(true);
});

test("Compare upgrade failure aborts the remaining upgrade lanes and keeps the original error", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const probe: { calls: Array<{ signal: AbortSignal; aborted: boolean }>; siblingAborted: boolean } = { calls: [], siblingAborted: false };
    let failNextRoot = true;
    api.buildUpgradeSeries = (_base, _solved, _maxUpgrade, signal) => {
      const isRootFailure = failNextRoot;
      if (isRootFailure) failNextRoot = false;
      const call = { signal, aborted: false };
      probe.calls.push(call);
      return new Promise((resolve, reject) => {
        signal.addEventListener("abort", () => {
          call.aborted = true;
          if (!isRootFailure) {
            probe.siblingAborted = true;
            failNextRoot = true;
          }
          reject(new Error("cancelled"));
        }, { once: true });
        if (isRootFailure) reject(new Error("comparison upgrade failed"));
      });
    };
    Object.assign(window, { compareUpgradeProbe: probe });
  });
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.locator('.error-strip[role="alert"]')).toContainText("comparison upgrade failed");
  await expect.poll(() => page.evaluate(() => (window as any).compareUpgradeProbe.calls.length)).toBeGreaterThanOrEqual(4);
  await expect.poll(() => page.evaluate(() => (window as any).compareUpgradeProbe.siblingAborted)).toBe(true);
});

test("Compare displays all eight explicit pins while excluding the selected baseline", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const state = useDesktopStore.getState();
    const pins = [state.selected!, ...Array.from({ length: 7 }, (_, index) => ({
      ...state.rows[1], weaponId: 1_000 + index, weaponName: `Pinned weapon ${index + 1}`,
    }))];
    api.solveBuild = async (_base, weaponName) => pins.find(row => row.weaponName === weaponName) ?? null;
    for (const row of pins) state.toggleCompareBench(row);
  });
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  await expect(page.locator(".compare-lanes").getByRole("group")).toHaveCount(8);
  await expect(page.getByRole("group", { name: "Selected baseline", exact: true })).toBeVisible();
  await expect(page.getByRole("group", { name: "Pinned #1", exact: true })).toHaveCount(0);
  await expect(page.getByRole("group", { name: "Pinned #8", exact: true })).toContainText("Pinned weapon 7");
});
