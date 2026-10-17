import { expect, test, type Page } from "@playwright/test";
import { settled } from "./editors";

// Export opens a choice of row counts; choosing one starts the file.
async function exportRows(page: Page, rows = "25 rows") {
  await page.getByRole("button", { name: "Export CSV", exact: true }).click();
  await page.getByRole("group", { name: "Rows to export" }).getByRole("button", { name: rows, exact: true }).click();
}

test("CSV export downloads successfully and releases the search controls", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  const download = page.waitForEvent("download");
  await exportRows(page);
  expect((await download).suggestedFilename()).toMatch(/\.csv$/);
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Export CSV", exact: true })).toBeEnabled();
});

for (const action of ["cancel", "profile", "edit", "numeric edit", "navigate", "tab bounce"]) {
  test(`CSV export is cancelled by ${action} without a late download`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.evaluate(async () => {
      const modulePath = "/src/lib/api.ts";
      const { api } = await import(modulePath);
      const statePath = "/src/lib/state.ts";
      const { useDesktopStore } = await import(statePath);
      const rows = useDesktopStore.getState().rows;
      Object.assign(window, { exportCancelled: false, finishExport: false });
      api.startSearch = async () => ({ jobId: "export-probe" });
      api.searchStatus = async () => ({
        progress: { jobId: "export-probe", checked: 3, total: 50, eligible: 3, bestScore: 700, elapsedMs: 100 },
        finished: (window as unknown as { finishExport: boolean }).finishExport ? { jobId: "export-probe", rows, cancelled: false, error: null, setups: rows.length } : null,
      });
      api.cancelSearch = async () => { Object.assign(window, { exportCancelled: true }); return true; };
    });
    const downloads: string[] = [];
    page.on("download", (download) => downloads.push(download.suggestedFilename()));
    await exportRows(page, "100 rows");
    await expect(page.getByRole("button", { name: "Search", exact: true })).toBeDisabled();
    await expect(page.getByText("3 / 50", { exact: true })).toBeVisible();
    if (action === "cancel") await page.getByRole("button", { name: "Cancel export", exact: true }).click();
    else if (action === "profile") await page.getByRole("radio", { name: /Convergence/ }).click();
    else if (action === "edit") await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
    else if (action === "numeric edit") await page.locator(".ribbon-stat input").first().fill("20");
    else if (action === "tab bounce") {
      // Leaves and returns before React renders, so the board's deferred visibility never changes.
      await page.evaluate(async () => {
        const { useDesktopStore } = await import("/src/lib/state.ts");
        const { setWorkspace } = useDesktopStore.getState();
        setWorkspace("compare");
        setWorkspace("rankings");
      });
    } else await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
    await page.waitForFunction(() => (window as unknown as { exportCancelled: boolean }).exportCancelled);
    if (action === "navigate") await page.getByRole("navigation").getByRole("button", { name: "Rankings", exact: true }).click();
    await expect(page.getByRole("button", { name: /^(Search|Update Results)$/ })).toBeDisabled();
    await page.evaluate(() => Object.assign(window, { finishExport: true }));
    await expect(page.getByRole("button", { name: /^(Search|Update Results)$/ })).toBeEnabled();
    await settled(page);
    expect(downloads).toEqual([]);
    await expect(page.locator('.error-strip[role="alert"]')).toHaveCount(0);
  });
}
