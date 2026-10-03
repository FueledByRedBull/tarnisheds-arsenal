import type { Locator, Page } from "@playwright/test";

export type EditorName =
  | "Class" | "Objective" | "Loadout" | "Upgrades" | "Scaling" | "Results" | "Limits" | "Comparison filters";

// Query-strip and Compare editors are native popovers: their controls exist but are hidden
// until the token is opened, exactly as a player has to open them.
export async function openEditor(page: Page, editor: EditorName): Promise<Locator> {
  const panel = page.getByRole("dialog", { name: editor, exact: true });
  // A closing editor stays visible while it fades out, so ask whether it is open.
  const open = await page.evaluate((name) => [...document.querySelectorAll(".popover-panel:popover-open")]
    .some((node) => node.getAttribute("aria-label") === name), editor);
  if (!open) {
    // An open editor can cover a token that wrapped onto the next line, as it would for a player.
    await closeEditors(page);
    await page.getByRole("button", { name: new RegExp(`^${editor}: `) }).click();
    await panel.waitFor();
  }
  return panel;
}

const CONTROL_EDITORS: Record<string, EditorName> = {
  Class: "Class",
  Weapon: "Loadout",
  "Weapon Type": "Loadout",
  Affinity: "Loadout",
  AoW: "Loadout",
  "AoW (fixed)": "Loadout",
  Somber: "Loadout",
};

// Opens the editor that owns a labelled control; controls outside any editor are left alone.
export async function openEditorFor(page: Page, control: string): Promise<void> {
  const editor = control.startsWith("Compare ") ? "Comparison filters" : CONTROL_EDITORS[control];
  if (editor) await openEditor(page, editor);
}

export async function closeEditors(page: Page): Promise<void> {
  await page.evaluate(() => {
    for (const panel of document.querySelectorAll<HTMLElement>(".popover-panel:popover-open")) panel.hidePopover();
  });
}

export type FoldedSection = "Saved Builds" | "Build details" | "Report a problem";

// Saved Builds, Compare's build cards and the problem report are folded until opened, so a
// test opens them as a player would. Opening an already open section is a no-op.
export async function openSection(page: Page, section: FoldedSection): Promise<void> {
  const summary = page.locator("summary").filter({ hasText: section }).first();
  if (!await summary.evaluate((node) => (node.parentElement as HTMLDetailsElement).open)) await summary.click();
}

// Compare with its build cards unfolded, for tests that read the lanes.
export async function openCompare(page: Page): Promise<void> {
  await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
  await openSection(page, "Build details");
}
