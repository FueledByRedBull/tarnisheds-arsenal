import { expect, test } from "@playwright/test";
import { closeEditors, openEditor } from "./editors";

test("the shared upgrade cap note reads the cap from the profile rules", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("radiogroup", { name: "Game profile" }).getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByText("Experimental model", { exact: true })).toBeVisible();
  await page.evaluate(async () => {
    const { useDesktopStore } = await import("/src/lib/state.ts");
    const catalog = useDesktopStore.getState().catalog!;
    const rules = { ...catalog.dataManifest.rules, standardMaxUpgrade: 12, somberMaxUpgrade: 12 };
    useDesktopStore.setState({ catalog: { ...catalog, dataManifest: { ...catalog.dataManifest, rules } } });
  });
  const editor = await openEditor(page, "Upgrades");
  await expect(editor.getByText(/one \+0 to \+12 reinforcement path/)).toBeVisible();
  await closeEditors(page);
});
