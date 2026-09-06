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
            if (node.flags & 1 && node.type?.name) counts[node.type.name] = (counts[node.type.name] ?? 0) + 1;
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
    useDesktopStore.getState().setSearching(true);
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    (window as any).renderCounts.CommandRail = 0;
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
  expect(await page.evaluate(() => (window as any).renderCounts.CommandRail)).toBe(0);
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
  await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
  await expect.poll(() => page.evaluate(() => (window as any).comparisonSearches)).toBeGreaterThan(initialSearches);
  await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
});
