export const ANNOUNCER_ID = "app-announcer";

// Speaks a message through the app's one polite live region (mounted by App). It serves messages
// that belong to no visible element, such as a value that was limited. A live region inside a
// label would become part of that label's name, so these never render beside their input.
export function announce(message: string) {
  if (typeof document === "undefined") return;
  const region = document.getElementById(ANNOUNCER_ID);
  if (!region) return;
  // Cleared first so the same message twice in a row is announced twice.
  region.textContent = "";
  window.requestAnimationFrame(() => { region.textContent = message; });
}
