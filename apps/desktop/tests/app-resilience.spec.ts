import { expect, test } from "@playwright/test";
import { openSection } from "./editors";

test("saving waits for verified catalog readiness without writing partial presets", async ({ page }) => {
  let release!: () => void;
  const loading = new Promise<void>(resolve => { release = resolve; });
  await page.route(/\/src\/lib\/browser-backend\.ts(?:\?.*)?$/, async route => {
    await loading;
    await route.continue();
  });
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await openSection(page, "Saved Builds");
  const save = page.getByRole("button", { name: "Save new", exact: true });
  const update = page.getByRole("button", { name: "Update selected", exact: true });
  try {
    await expect(save).toBeDisabled();
    await save.evaluate(button => (button as HTMLButtonElement).click());
    expect(await page.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith("tarnisheds-arsenal.savedBuild")))).toEqual([]);
  } finally { release(); }
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await save.click();
  await expect(page.getByText("Saved Build Preset.", { exact: true })).toBeVisible();
  await expect(update).toBeEnabled();
  const original = await page.evaluate(() => JSON.stringify(localStorage));
  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    const catalog = await api.catalog("convergence");
    api.catalog = () => new Promise(resolve => {
      Object.assign(window, { finishSavingCatalog: () => resolve(catalog) });
    });
  });
  await page.getByRole("radio", { name: "Convergence Beta 3.0.0.1", exact: true }).click();
  await expect(save).toBeDisabled();
  await expect(update).toBeDisabled();
  await save.evaluate(button => (button as HTMLButtonElement).click());
  await update.evaluate(button => (button as HTMLButtonElement).click());
  expect(await page.evaluate(() => JSON.stringify(localStorage))).toBe(original);
  await page.evaluate(() => { (window as any).finishSavingCatalog(); });
  await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
  await expect(save).toBeEnabled();
  await expect(update).toBeEnabled();
  await expect(page.locator(".error-strip")).toHaveCount(0);
});

test("recovers a damaged index without overwriting the original and restores a bulk backup as copies", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await expect(page.getByText("Saved Build Preset.", { exact: true })).toBeVisible();
  await page.evaluate(() => localStorage.setItem("tarnisheds-arsenal.savedBuildIndex.v1", "{damaged-index"));
  await page.reload();
  await expect(page.getByText(/index is damaged/).first()).toBeVisible();
  await page.getByRole("button", { name: "Recover 1 builds", exact: true }).click();
  expect(await page.evaluate(() => Object.keys(localStorage)
    .filter(key => key.startsWith("tarnisheds-arsenal.savedBuildIndex.recovery."))
    .map(key => localStorage.getItem(key)))).toEqual(["{damaged-index"]);
  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.getByText("Loaded Build Preset; saved results verified on current data.", { exact: true })).toBeVisible();
  await page.getByText("Backup and recovery", { exact: true }).click();
  const downloadEvent = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export all builds", exact: true }).click();
  const download = await downloadEvent;
  const stream = await download.createReadStream();
  const chunks: Buffer[] = [];
  for await (const chunk of stream!) chunks.push(Buffer.from(chunk));
  const buffer = Buffer.concat(chunks);
  expect(JSON.parse(buffer.toString()).builds).toHaveLength(1);
  await page.getByLabel("Restore build backup", { exact: false }).setInputFiles({ name: "builds.json", mimeType: "application/json", buffer });
  await page.getByRole("button", { name: "Restore 1 builds as copies", exact: true }).click();
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("tarnisheds-arsenal.savedBuildIndex.v1")!).builds.length)).toBe(2);
});

test("denied storage and invalid backups keep existing builds usable", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  const before = await page.evaluate(() => JSON.stringify(localStorage));
  await page.getByText("Backup and recovery", { exact: true }).click();
  await page.getByLabel("Restore build backup", { exact: false }).setInputFiles({
    name: "invalid.json", mimeType: "application/json", buffer: Buffer.from('{"format":"wrong"}'),
  });
  await expect(page.getByText(/Not a supported saved-build backup/)).toBeVisible();
  expect(await page.evaluate(() => JSON.stringify(localStorage))).toBe(before);
  await page.evaluate(() => { Storage.prototype.setItem = () => { throw new DOMException("quota exceeded", "QuotaExceededError"); }; });
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await expect(page.locator('.error-strip[role="alert"]')).toContainText("quota exceeded");
  await expect(page.getByRole("button", { name: "Load", exact: true })).toBeEnabled();
});

test("report preview freezes a redacted snapshot and explanations clear when results become stale", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.getByText("Why this build?", { exact: true }).click();
  await expect(page.getByText(/The optimizer's exact ranking/)).toBeVisible();
  await openSection(page, "Report a problem");
  await page.getByRole("button", { name: "Preview reproduction report", exact: true }).click();
  const report = page.getByRole("textbox", { name: "Reproduction report preview", exact: true });
  const captured = await report.inputValue();
  expect(JSON.parse(captured).results.selected).not.toBeNull();
  await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
  await expect(page.getByText("Why this build?", { exact: true })).toHaveCount(0);
  await expect(report).toHaveValue(captured);
  await page.getByRole("button", { name: "Preview reproduction report", exact: true }).click();
  expect(JSON.parse(await report.inputValue()).results.selected).toBeNull();
  await page.evaluate(() => { URL.createObjectURL = () => { throw new Error("blocked"); }; });
  await page.getByRole("button", { name: "Download report", exact: true }).click();
  await expect(page.getByText(/The report could not be downloaded/)).toBeVisible();
});

test("storage read denial after selection reports failure without unmounting the app", async ({ page }) => {
  const crashes: string[] = [];
  page.on("pageerror", error => crashes.push(error.message));
  await page.goto("/");
  await expect(page.getByText("Snapshot loaded", { exact: true })).toBeVisible();
  await openSection(page, "Saved Builds");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.evaluate(() => { Storage.prototype.getItem = () => { throw new DOMException("denied", "SecurityError"); }; });
  await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
  await expect(page.getByText(/Saved builds could not be read/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Load", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Search", exact: true })).toBeVisible();
  expect(crashes).toEqual([]);
});
