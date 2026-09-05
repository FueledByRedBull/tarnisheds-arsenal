import { expect, test, type Page } from "@playwright/test";

declare global {
  interface Window {
    selectorFixture: {
      mount(props: Record<string, unknown>): void;
      unmount(): void;
      patch(props: Record<string, unknown>): void;
      pending(weapon: string): number;
      resolve(weapon: string, affinity?: string | null): void;
      reject(weapon: string): void;
      changes(): Array<string | null>;
      errors(): string[];
    };
  }
}

async function resolveProfile(page: Page, weapon: string, affinity: string | null = null) {
  await expect.poll(() => page.evaluate((name) => window.selectorFixture.pending(name), weapon)).toBeGreaterThan(0);
  await page.evaluate(({ weapon, affinity }) => window.selectorFixture.resolve(weapon, affinity), { weapon, affinity });
}

for (const label of ["AoW", "Compare AoW"]) {
  test.describe(label, () => {
    test.beforeEach(async ({ page }) => {
      await page.goto("/tests/aow-select.html");
      await page.waitForFunction(() => Boolean(window.selectorFixture));
    });

    test("weapon changes default to native while explicit overrides and restored Automatic survive mounting", async ({ page }) => {
      await page.evaluate((label) => window.selectorFixture.mount({ label, allowMatchSelected: label === "Compare AoW" }), label);
      const selector = page.getByRole("combobox", { name: label, exact: true });
      await expect(selector).toBeEnabled();
      await page.evaluate(() => window.selectorFixture.patch({ weaponName: "Buckler", value: null }));
      await resolveProfile(page, "Buckler");
      await expect(selector).toHaveValue("Buckler Parry");
      await selector.click();
      await page.getByRole("option", { name: "No Skill", exact: true }).click();
      await expect(selector).toHaveValue("No Skill");
      await page.evaluate((label) => window.selectorFixture.mount({ label, weaponName: "Buckler", value: "No Skill" }), label);
      await resolveProfile(page, "Buckler");
      await expect(selector).toHaveValue("No Skill");
      await expect.poll(() => page.evaluate(() => window.selectorFixture.changes())).toEqual([]);
      await selector.click();
      await page.getByRole("option", { name: "Automatic (best legal skill)", exact: true }).click();
      await page.evaluate(() => window.selectorFixture.unmount());
      await expect(selector).toHaveCount(0);
      await page.evaluate((label) => window.selectorFixture.mount({ label, weaponName: "Buckler", value: null }), label);
      await resolveProfile(page, "Buckler");
      await expect(selector).toHaveValue("Automatic (best legal skill)");
      await expect.poll(() => page.evaluate(() => window.selectorFixture.changes())).toEqual([]);
    });

    test("fixed native skills override restored invalid or Match Selected values and disable editing", async ({ page }) => {
      await page.evaluate((label) => window.selectorFixture.mount({ label, weaponName: "Rivers of Blood",
        value: label === "Compare AoW" ? "__match_selected__" : "No Skill", allowMatchSelected: true }), label);
      await resolveProfile(page, "Rivers of Blood");
      const selector = page.getByRole("combobox", { name: `${label} (fixed)`, exact: true });
      await expect(selector).toHaveValue("Corpse Piler");
      await expect(selector).toBeDisabled();
      await expect.poll(() => page.evaluate(() => window.selectorFixture.changes())).toEqual(["Corpse Piler"]);
    });

    test("late replies cannot overwrite the latest weapon and unmounts do not update parents", async ({ page }) => {
      await page.evaluate((label) => window.selectorFixture.mount({ label, weaponName: "Buckler" }), label);
      await expect.poll(() => page.evaluate(() => window.selectorFixture.pending("Buckler"))).toBeGreaterThan(0);
      await page.evaluate(() => window.selectorFixture.patch({ weaponName: "Rivers of Blood", value: null }));
      await resolveProfile(page, "Rivers of Blood");
      await page.evaluate(() => window.selectorFixture.resolve("Buckler"));
      await expect(page.getByRole("combobox", { name: `${label} (fixed)`, exact: true })).toHaveValue("Corpse Piler");
      await expect.poll(() => page.evaluate(() => window.selectorFixture.changes())).toEqual(["Corpse Piler"]);
      await page.evaluate(() => window.selectorFixture.patch({ weaponName: "Buckler", affinity: "Blood", value: null }));
      await expect.poll(() => page.evaluate(() => window.selectorFixture.pending("Buckler"))).toBeGreaterThan(0);
      await page.evaluate(() => window.selectorFixture.unmount());
      await expect(page.getByRole("combobox")).toHaveCount(0);
      await page.evaluate(() => window.selectorFixture.resolve("Buckler", "Blood"));
      await expect.poll(() => page.evaluate(() => window.selectorFixture.changes())).toEqual(["Corpse Piler"]);
      await expect.poll(() => page.evaluate(() => window.selectorFixture.errors())).toEqual([]);
    });

    test("affinity changes clear incompatible native skills and lookup errors are surfaced", async ({ page }) => {
      await page.evaluate((label) => window.selectorFixture.mount({ label, weaponName: "Buckler", value: "Buckler Parry" }), label);
      await resolveProfile(page, "Buckler");
      await page.evaluate(() => window.selectorFixture.patch({ affinity: "Blood" }));
      await resolveProfile(page, "Buckler", "Blood");
      const selector = page.getByRole("combobox", { name: label, exact: true });
      await expect(selector).toHaveValue("Automatic (best legal skill)");
      await selector.click();
      await expect(page.getByRole("option", { name: "Buckler Parry", exact: true })).toHaveCount(0);
      await page.evaluate(() => window.selectorFixture.patch({ weaponName: "Rivers of Blood", affinity: null }));
      await expect.poll(() => page.evaluate(() => window.selectorFixture.pending("Rivers of Blood"))).toBeGreaterThan(0);
      await page.evaluate(() => window.selectorFixture.reject("Rivers of Blood"));
      await expect.poll(() => page.evaluate(() => window.selectorFixture.errors())).toEqual(["Profile lookup failed"]);
      await expect(selector).toBeDisabled();
    });
  });
}
