import { expect, test } from "@playwright/test";

async function openCompare(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
}

test("tradeoff views select exact threshold choices without recalculating", async ({ page }) => {
  await openCompare(page);
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const original = api.arBleedFrontier;
    (window as any).frontierCalls = 0;
    api.arBleedFrontier = async (...args) => {
      (window as any).frontierCalls++;
      const points = await original(...args);
      // Rounded AR looks eligible at 1%; the exact threshold says otherwise.
      points[1].minimumArLossBps = 101;
      return points;
    };
  });
  const section = page.getByRole("region", { name: "AR / Bleed tradeoffs" });
  await section.getByRole("button", { name: "Compute trade-offs", exact: true }).click();
  await expect(section.getByRole("status")).toContainText("4 exact trade-off points");
  const shortlist = section.locator(".tradeoff-shortlist");
  await expect(shortlist.locator("tbody tr")).toHaveCount(3);
  await expect(shortlist.getByRole("button", { name: /^Max AR/ })).toContainText("Within 1%");
  const input = section.getByRole("spinbutton", { name: "Max AR sacrifice (%)" });
  await input.fill("1");
  await expect(section.locator(".tradeoff-inspection")).toContainText("Gain 0.0 bleed buildup");
  await input.fill("1.01");
  await expect(section.locator(".tradeoff-inspection")).toContainText("Gain 5.0 bleed buildup");
  await input.fill("1.001");
  await expect(section.getByRole("alert")).toContainText("steps of 0.01%");
  await section.getByText("Explore all tradeoffs (4)", { exact: true }).click();
  await expect(section.locator(".tradeoff-all tbody tr")).toHaveCount(4);
  const point = section.getByRole("button", { name: /^Point 4:/ });
  await point.press("Enter");
  await expect(point).toHaveAttribute("aria-pressed", "true");
  await expect(section.locator(".tradeoff-inspection")).toContainText("Gain 15.0 bleed buildup");
  await expect(section.locator(".tradeoff-inspection")).toContainText("VIG 12 / MND 11 / END 13");
  expect(await page.evaluate(() => (window as any).frontierCalls)).toBe(1);
  await expect(page.locator(".selected-build")).toContainText("Blood / Seppuku / +25");
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const original = api.startSearch;
    api.startSearch = request => {
      (window as any).appliedTradeoffRequest = request;
      return original(request);
    };
  });
  await section.getByRole("button", { name: "Use exact allocation", exact: true }).click();
  const applied = await page.evaluate(() => (window as any).appliedTradeoffRequest);
  expect(applied).toMatchObject({
    weaponName: "Uchigatana", affinity: "Blood", aowName: "Seppuku",
    objective: "max_ar", exactUpgrade: true, standardMaxUpgrade: 25,
    lockStr: 13, lockDex: 19, lockInt: 9, lockFai: 8, lockArc: 63,
  });
});

test("leaving Compare aborts a pending frontier and discards its late result", async ({ page }) => {
  await openCompare(page);
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    (window as any).frontierProbe = { aborted: false };
    api.arBleedFrontier = (_base, _selected, signal) => new Promise(resolve => {
      (window as any).frontierProbe.finish = resolve;
      signal?.addEventListener("abort", () => { (window as any).frontierProbe.aborted = true; });
    });
  });
  await page.getByRole("button", { name: "Compute trade-offs", exact: true }).click();
  await expect(page.locator(".loadout-tradeoffs").getByRole("status")).toContainText("Calculating");
  await page.getByRole("navigation").getByRole("button", { name: "Rankings", exact: true }).click();
  expect(await page.evaluate(() => (window as any).frontierProbe.aborted)).toBe(true);
  await page.evaluate(() => (window as any).frontierProbe.finish([]));
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await expect(page.locator(".loadout-tradeoffs").getByRole("status")).toContainText("Ready to calculate");
  await expect(page.locator(".tradeoff-shortlist")).toHaveCount(0);
});
