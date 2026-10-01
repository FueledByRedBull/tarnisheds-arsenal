import type { Locator, Page } from "@playwright/test";

export type EditorName =
  | "Class" | "Objective" | "Loadout" | "Upgrades" | "Scaling" | "Results" | "Limits" | "Comparison filters";

// Query-strip and Compare editors are native popovers: their controls exist but are hidden
// until the token is opened, exactly as a player has to open them.
export async function openEditor(page: Page, editor: EditorName): Promise<Locator> {
  const panel = page.getByRole("dialog", { name: editor, exact: true });
  if (!(await panel.isVisible())) {
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
