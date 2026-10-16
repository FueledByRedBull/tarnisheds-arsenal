// Text entry keeps its own undo and typing; buttons, checkboxes and the page do not.
export function isTextEntry(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable
    || target.matches("textarea, select, input:not([type=checkbox]):not([type=radio]):not([type=button]):not([type=submit])"));
}

// The palette and shortcut list own the keyboard while they are open.
export function keyboardOwnedByOverlay(): boolean {
  return document.querySelector("dialog[open], .command-palette:popover-open") !== null;
}

// WebView2 keeps browser reload and print keys; in the app they would discard the unsaved session.
// The physical key code also catches non-Latin layouts, whose key value is the typed letter.
export function blockBrowserReload(event: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "preventDefault">) {
  const key = event.key.toLowerCase();
  const reloadOrPrint = key === "r" || key === "p" || event.code === "KeyR" || event.code === "KeyP";
  if (key === "f5" || ((event.ctrlKey || event.metaKey) && reloadOrPrint)) event.preventDefault();
}
