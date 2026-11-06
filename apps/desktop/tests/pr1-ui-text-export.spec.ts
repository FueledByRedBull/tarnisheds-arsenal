import { expect, test, type Page } from "@playwright/test";
import { openCompare } from "./editors";

const EXPORT_REASON = /when the (CSV )?export finishes/;

async function setStore(page: Page, patch: Record<string, unknown>) {
  await page.evaluate(async (next) => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.setState(next);
  }, patch);
}

async function searched(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
}

test("result locks and the updated search say why they wait for a CSV export", async ({ page }) => {
  await searched(page);
  await page.locator(".result-row").first().click();
  await setStore(page, { resultsStale: true, isExporting: true });

  const rowLock = page.locator(".result-row").first().getByRole("button", { name: /^Lock stats to /, includeHidden: true });
  await expect(rowLock).toBeDisabled();
  await expect(rowLock).toHaveAccessibleDescription(EXPORT_REASON);
  await expect(rowLock).toHaveAttribute("title", EXPORT_REASON);

  const detailLock = page.getByRole("button", { name: "Lock stats to this build", exact: true });
  await expect(detailLock).toBeDisabled();
  await expect(detailLock).toHaveAccessibleDescription(EXPORT_REASON);

  const rerun = page.getByRole("button", { name: "Run updated search", exact: true });
  await expect(rerun).toBeDisabled();
  await expect(rerun).toHaveAccessibleDescription(EXPORT_REASON);

  await setStore(page, { isExporting: false });
  await expect(rowLock).toBeEnabled();
  await expect(detailLock).toBeEnabled();
  await expect(rerun).toBeEnabled();
  await expect(rowLock).not.toHaveAccessibleDescription(EXPORT_REASON);
});

test("Use exact allocation keeps the objective and says why it waits for a CSV export", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    useDesktopStore.getState().patchRequest({ objective: "max_physical_ar" });
  });
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await openCompare(page);
  const section = page.getByRole("region", { name: "AR / Bleed trade-offs" });
  await section.getByRole("button", { name: "Compute trade-offs", exact: true }).click();
  await expect(section.getByRole("status")).toContainText("exact trade-off points");

  const apply = section.getByRole("button", { name: "Use exact allocation", exact: true });
  await setStore(page, { isExporting: true });
  await expect(apply).toBeDisabled();
  await expect(apply).toHaveAccessibleDescription(EXPORT_REASON);
  await setStore(page, { isExporting: false });
  await expect(apply).toBeEnabled();

  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    api.startSearch = async (request) => {
      (window as unknown as { appliedRequest: unknown }).appliedRequest = request;
      return { jobId: "keep-objective" };
    };
    api.searchStatus = async (jobId) => ({ progress: null, finished: { jobId, cancelled: false, rows: [], error: null, setups: 0 } });
  });
  await apply.click();
  const applied = await page.evaluate(() => (window as unknown as { appliedRequest: { objective: string; lockStr: number | null } }).appliedRequest);
  expect(applied.objective).toBe("max_physical_ar");
  expect(applied.lockStr).not.toBeNull();
  expect(await page.evaluate(async () => (await import("/src/lib/state.ts")).useDesktopStore.getState().request.objective)).toBe("max_physical_ar");
  await expect(page.getByRole("heading", { name: "Rankings", exact: true })).toBeVisible();
});
