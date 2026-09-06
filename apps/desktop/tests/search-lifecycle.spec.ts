import { expect, test } from "@playwright/test";

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
