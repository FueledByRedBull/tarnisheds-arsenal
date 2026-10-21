import { expect, test } from "@playwright/test";

test("scrolling and progress updates leave unchanged controls alone", async ({ page }) => {
  await page.addInitScript(() => {
    const counts: Record<string, number> = {};
    Object.assign(window, {
      renderCounts: counts,
      __REACT_DEVTOOLS_GLOBAL_HOOK__: {
        supportsFiber: true,
        renderers: new Map(),
        inject: () => 1,
        onCommitFiberUnmount: () => {},
        onCommitFiberRoot: (_id: number, root: { current: unknown }) => {
          function visit(node: any) {
            if (!node) return;
            // memo() and forwardRef() wrap the component, so read the inner name as well.
            const name = node.type?.displayName ?? node.type?.name ?? node.type?.type?.name ?? node.type?.render?.name;
            if (node.flags & 1 && name) counts[name] = (counts[name] ?? 0) + 1;
            visit(node.child);
            visit(node.sibling);
          }
          visit(root.current);
        },
      },
    });
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  // Positive control: the counter sees RankingsBoard, so a zero below is not vacuous.
  expect(await page.evaluate(() => (window as any).renderCounts.RankingsBoard)).toBeGreaterThan(0);
  await page.evaluate(async () => {
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    const counts = (window as any).renderCounts;
    counts.RankingsBoard = 0;
    for (let index = 0; index < 20; index++) {
      document.querySelector(".result-board")!.dispatchEvent(new Event("scroll"));
      await new Promise(requestAnimationFrame);
    }
  });
  expect(await page.evaluate(() => (window as any).renderCounts.RankingsBoard)).toBe(0);
  await page.evaluate(async () => {
    const modulePath = "/src/lib/state.ts";
    const { useDesktopStore } = await import(modulePath);
    (window as any).renderCounts.QueryStrip = 0;
    useDesktopStore.getState().setSearching(true);
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    if (!((window as any).renderCounts.QueryStrip > 0)) throw new Error("QueryStrip renders were not counted");
    (window as any).renderCounts.QueryStrip = 0;
    (window as any).renderCounts.SearchProgressPanel = 0;
  });
  await expect(page.getByLabel("Elapsed time", { exact: true })).not.toHaveText("0.0s");
  await page.evaluate(async () => {
    const modulePath = "/src/lib/state.ts";
    const { useDesktopStore } = await import(modulePath);
    useDesktopStore.getState().setProgress({ jobId: "render-check", checked: 10, total: 100,
      eligible: 4, bestScore: 700, elapsedMs: 2400 });
  });
  await expect(page.locator(".progress-strip")).toContainText("2.4s");
  expect(await page.evaluate(() => (window as any).renderCounts.QueryStrip)).toBe(0);
  expect(await page.evaluate(() => (window as any).renderCounts.SearchProgressPanel)).toBeGreaterThan(0);
});

test("returning to an unchanged custom comparison reuses the completed search", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.evaluate(async () => {
    const modulePath = "/src/lib/api.ts";
    const { api } = await import(modulePath);
    const original = api.startSearch;
    (window as any).comparisonSearches = 0;
    api.startSearch = async (...args: Parameters<typeof original>) => {
      (window as any).comparisonSearches++;
      return original(...args);
    };
    const statePath = "/src/lib/state.ts";
    const { useDesktopStore } = await import(statePath);
    useDesktopStore.getState().patchCompareControls({ weaponName: "Uchigatana", matchSelectedAow: false });
  });
  const nav = page.getByRole("navigation");
  await nav.getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  const initialSearches = await page.evaluate(() => (window as any).comparisonSearches);
  expect(initialSearches).toBeGreaterThan(0);
  const lanes = await page.locator(".compare-lane").allTextContents();
  await nav.getByRole("button", { name: "Rankings", exact: true }).click();
  await nav.getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
  expect(await page.locator(".compare-lane").allTextContents()).toEqual(lanes);
  expect(await page.evaluate(() => (window as any).comparisonSearches)).toBe(initialSearches);
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const original = api.buildUpgradeSeries;
    (window as any).comparisonUpgradeSeries = 0;
    api.buildUpgradeSeries = async (...args: Parameters<typeof original>) => {
      (window as any).comparisonUpgradeSeries++;
      return original(...args);
    };
  });
  const initialUpgradeSeries = await page.evaluate(() => (window as any).comparisonUpgradeSeries);
  await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
  await expect(page.getByText("Update Rankings before comparing", { exact: true })).toBeVisible();
  await expect(page.locator(".compare-lane")).toHaveCount(0);
  await expect(page.locator(".compare-deltas")).toHaveCount(0);
  await expect(page.locator(".matrix-wrap")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).comparisonSearches)).toBe(initialSearches);
  expect(await page.evaluate(() => (window as any).comparisonUpgradeSeries)).toBe(initialUpgradeSeries);
  await page.getByRole("button", { name: "Go to Rankings", exact: true }).click();
  await page.getByRole("button", { name: "Update Results", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await nav.getByRole("button", { name: "Compare", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).comparisonSearches)).toBeGreaterThan(initialSearches);
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
});

test("the ranking rows stay one composited flex group", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  // Sorting sets the rows' flex order inside this group, and re-forging fades it as one layer
  // (styles.css).
  await expect(page.locator(".result-rows")).toHaveCSS("will-change", "opacity");
  await expect(page.locator(".result-rows")).toHaveCSS("display", "flex");
});

test("rows away from the board's view rest while new results land", async ({ page }) => {
  await page.setViewportSize({ width: 1650, height: 950 });
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  // Records the most rows seen resting at once, and whether a row in view ever rested.
  await page.evaluate(() => {
    const record = window as unknown as { restingSeen: number; viewRested: boolean };
    Object.assign(record, { restingSeen: 0, viewRested: false });
    new MutationObserver(() => {
      record.restingSeen = Math.max(record.restingSeen, document.querySelectorAll(".result-row[data-resting]").length);
      record.viewRested ||= document.querySelector(".result-row:nth-child(-n+8)[data-resting]") !== null;
    }).observe(document.querySelector(".result-rows")!, { subtree: true, attributeFilter: ["data-resting"] });
  });
  const land = () => page.evaluate(async () => {
    const modulePath = "/src/lib/state.ts";
    const { useDesktopStore } = await import(modulePath);
    const state = useDesktopStore.getState();
    const rows = Array.from({ length: 50 }, (_, index) => ({ ...state.rows[index % state.rows.length] }));
    (window as unknown as { restingSeen: number }).restingSeen = 0;
    state.setRows(rows, state.rowsSetups ?? 0);
  });
  const record = () => page.evaluate(() => {
    const { restingSeen, viewRested } = window as unknown as { restingSeen: number; viewRested: boolean };
    return { restingSeen, viewRested };
  });

  // More rows mount resting below the view; the same count keeps the rows drawn in view.
  for (let landing = 0; landing < 2; landing += 1) {
    await land();
    await expect(page.getByText("50 ranked rows")).toBeVisible();
    await expect.poll(async () => (await record()).restingSeen).toBeGreaterThan(20);
    await expect(page.locator(".result-row[data-resting]")).toHaveCount(0);
    expect((await record()).viewRested).toBe(false);
  }
  await expect(page.locator(".result-row").last().locator(".weapon-line strong")).toBeVisible();
});

test("rows away from the board's view rest during a resize and a return to Rankings", async ({ page }) => {
  await page.setViewportSize({ width: 1650, height: 950 });
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.evaluate(async () => {
    const modulePath = "/src/lib/state.ts";
    const { useDesktopStore } = await import(modulePath);
    const rows = useDesktopStore.getState().rows;
    useDesktopStore.setState({ rows: Array.from({ length: 50 }, (_, index) => rows[index % rows.length]) });
  });
  await expect(page.getByText("50 ranked rows")).toBeVisible();
  // Records the most rows seen resting at once.
  await page.evaluate(() => {
    const record = window as unknown as { restingSeen: number };
    record.restingSeen = 0;
    new MutationObserver(() => {
      record.restingSeen = Math.max(record.restingSeen, document.querySelectorAll(".result-row[data-resting]").length);
    }).observe(document.querySelector(".result-rows")!, { subtree: true, attributeFilter: ["data-resting"] });
  });
  const restingSeen = () => page.evaluate(() => (window as unknown as { restingSeen: number }).restingSeen);
  const lastName = page.locator(".result-row").last().locator(".weapon-line strong");

  await page.setViewportSize({ width: 1600, height: 950 });
  await expect.poll(restingSeen).toBeGreaterThan(20);
  await expect(page.locator(".result-row[data-resting]")).toHaveCount(0);
  await expect(lastName).toBeVisible();

  await page.evaluate(() => { (window as unknown as { restingSeen: number }).restingSeen = 0; });
  const nav = page.getByRole("navigation");
  await page.keyboard.press("Control+2");
  await expect(nav.getByRole("button", { name: "Compare", exact: true })).toHaveAttribute("aria-current", "page");
  await page.keyboard.press("Control+1");
  await expect(nav.getByRole("button", { name: "Rankings", exact: true })).toHaveAttribute("aria-current", "page");
  await expect.poll(restingSeen).toBeGreaterThan(20);
  await expect(page.locator(".result-row[data-resting]")).toHaveCount(0);
  await expect(lastName).toBeVisible();
});
