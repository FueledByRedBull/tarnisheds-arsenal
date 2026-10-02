// Text entry keeps its own undo and typing; buttons, checkboxes and the page do not.
export function isTextEntry(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable
    || target.matches("textarea, select, input:not([type=checkbox]):not([type=radio]):not([type=button]):not([type=submit])"));
}

// The palette and shortcut list own the keyboard while they are open.
export function keyboardOwnedByOverlay(): boolean {
  return document.querySelector("dialog[open], .command-palette:popover-open") !== null;
}
