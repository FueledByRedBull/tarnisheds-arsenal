import { expect, test } from "@playwright/test";

test("saving with result locks disabled keeps them disabled after loading", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.locator(".result-row-full").first().getByRole("button", { name: /^Lock / }).click();
  await page.getByText("Advanced", { exact: true }).click();
  const locks = page.getByRole("checkbox", { name: "Use Locked Result Stats", exact: true });
  await locks.uncheck();
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(locks).not.toBeChecked();
  const saved = await page.evaluate(() => {
    const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
    return JSON.parse(localStorage.getItem(key)!).request;
  });
  expect([saved.lockStr, saved.lockDex, saved.lockInt, saved.lockFai, saved.lockArc]).toEqual([null, null, null, null, null]);
});

test("a failed deletion reports the error and preserves a loadable build", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.evaluate(() => {
    const write = Storage.prototype.setItem;
    Object.assign(window, { restoreStorage: () => { Storage.prototype.setItem = write; } });
    Storage.prototype.setItem = function (key, value) {
      if (key === "tarnisheds-arsenal.savedBuildIndex.v1") throw new DOMException("index quota exceeded", "QuotaExceededError");
      write.call(this, key, value);
    };
  });
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
  await expect(page.locator('.error-strip[role="alert"]')).toContainText("index quota exceeded");
  await page.evaluate(() => (window as unknown as { restoreStorage: () => void }).restoreStorage());
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.getByText("Loaded Build Preset.", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Saved", exact: true })).toHaveValue("");
});

test("Convergence saves, updates and reloads the displayed fixed stats", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await expect(page.getByText("Saved Build Preset.", { exact: true })).toBeVisible();
  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("40");
  await page.getByRole("button", { name: "Update selected", exact: true }).click();
  const total = Number(await page.getByRole("textbox", { name: "Stat total", exact: true }).inputValue());
  const saved = await page.evaluate(() => {
    const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
    return JSON.parse(localStorage.getItem(key)!);
  });
  expect(saved.request).toMatchObject({ profileId: "convergence", characterLevel: total, strStat: 40 });
  await page.reload();
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.getByRole("spinbutton", { name: "STR", exact: true })).toHaveValue("40");
  await expect(page.getByRole("textbox", { name: "Stat total", exact: true })).toHaveValue(String(total));
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("1 ranked rows")).toBeVisible();
});

for (const action of ["Save new", "Update selected"]) {
  test(`${action} discards solved rows after inputs change`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.locator(".result-row-full").first().getByRole("button", { name: /^Compare / }).click();
    if (action === "Update selected") await page.getByRole("button", { name: "Save new", exact: true }).click();
    await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
    await expect(page.getByText("Inputs changed", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: action, exact: true }).click();
    await expect(page.getByText(/Inputs only; rerun search for current results/)).toBeVisible();
    const saved = await page.evaluate(() => {
      const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
      return JSON.parse(localStorage.getItem(key)!);
    });
    expect(saved).toMatchObject({ request: { twoHanding: true }, selectedBuild: null, compareTarget: null, compareBench: [] });
    await page.getByRole("button", { name: "Load", exact: true }).click();
    await expect(page.locator(".result-row-full")).toHaveCount(0);
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
  });
}

for (const action of ["delete", "edit", "profile", "update"]) {
  test(`migration cannot overwrite a later ${action}`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.getByRole("button", { name: "Save new", exact: true }).click();
    const original = await page.evaluate(() => {
      const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
      const preset = JSON.parse(localStorage.getItem(key)!);
      preset.dataVersion = "vanilla:4:old:old";
      localStorage.setItem(key, JSON.stringify(preset));
      return preset;
    });
    await page.reload();
    await page.evaluate(async () => {
      const modulePath = "/src/lib/api.ts";
      const { api } = await import(modulePath);
      const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
      const row = JSON.parse(localStorage.getItem(key)!).selectedBuild;
      api.solveBuild = () => new Promise((resolve) => {
        Object.assign(window, { finishMigration: () => resolve(row) });
      });
    });
    await page.getByRole("button", { name: "Migrate data", exact: true }).click();
    await page.waitForFunction(() => "finishMigration" in window);
    if (action === "delete") {
      await page.getByRole("button", { name: "Delete", exact: true }).click();
      await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
    } else if (action === "profile") {
      await page.getByRole("radio", { name: /Convergence/ }).click();
      await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
    } else {
      await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
      if (action === "update") await page.getByRole("button", { name: "Update selected", exact: true }).click();
    }
    await page.evaluate(async () => {
      (window as unknown as { finishMigration: () => void }).finishMigration();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    const saved = await page.evaluate((id) => localStorage.getItem(`tarnisheds-arsenal.savedBuild.v2.${id}`), original.id);
    if (action === "delete") {
      expect(saved).toBeNull();
      await expect(page.getByRole("combobox", { name: "Saved", exact: true })).toHaveValue("");
    } else if (action === "update") {
      expect(JSON.parse(saved!)).toMatchObject({ request: { twoHanding: true }, selectedBuild: null });
    } else {
      expect(JSON.parse(saved!)).toEqual(original);
      if (action === "edit") await expect(page.getByRole("checkbox", { name: "Two-handing", exact: true })).toBeChecked();
      else await expect(page.getByRole("radio", { name: /Convergence/ })).toHaveAttribute("aria-checked", "true");
    }
    await expect(page.getByText(/Migrated Build Preset/)).toHaveCount(0);
  });
}
