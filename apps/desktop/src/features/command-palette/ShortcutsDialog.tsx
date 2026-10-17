import { X } from "lucide-react";
import { Ref, useRef } from "react";
import { createPortal } from "react-dom";

type Shortcut = { keys: string[][]; action: string; range?: boolean };

const GROUPS: Array<{ title: string; items: Shortcut[] }> = [
  {
    title: "Anywhere",
    items: [
      { keys: [["Ctrl", "K"]], action: "Edit anything by typing" },
      { keys: [["Ctrl", "Enter"]], action: "Search, or update stale results" },
      { keys: [["Ctrl", "Z"]], action: "Undo the last query change" },
      { keys: [["Ctrl", "Y"], ["Ctrl", "Shift", "Z"]], action: "Redo" },
      { keys: [["Ctrl", "1"], ["Ctrl", "4"]], action: "Rankings, Compare, Paths, Affinity Watch", range: true },
      { keys: [["?"]], action: "Show these shortcuts" },
    ],
  },
  {
    title: "Rankings",
    items: [
      { keys: [["Up"], ["Down"]], action: "Move between ranked rows" },
      { keys: [["Home"], ["End"]], action: "First or last row" },
      { keys: [["Enter"]], action: "Select the focused row" },
      { keys: [["P"]], action: "Pin or unpin the focused row for Compare" },
      { keys: [["L"]], action: "Lock stats to the focused row and search" },
    ],
  },
  {
    title: "Editors and palette",
    items: [
      { keys: [["Up"], ["Down"]], action: "Move through options or commands" },
      { keys: [["Enter"]], action: "Apply the highlighted choice" },
      { keys: [["Esc"]], action: "Close the list, then the editor" },
    ],
  },
];

// Every shortcut in one modal list, opened with "?" or from the palette through `ref`. Opening
// and closing stay native, so neither re-renders the query strip that owns the list. It is a
// popover with a backdrop rather than a modal <dialog>: showModal() makes the rest of the page
// inert, which restyles every element as the list opens and again as it closes. Instead the
// list sits beside the app root, which it hides from screen readers while open (aria-modal
// alone does not), Tab stays on its one control, Escape closes it natively and focus returns
// where it was.
export function ShortcutsDialog({ ref }: { ref: Ref<HTMLDivElement> }) {
  const returnFocus = useRef<HTMLElement | null>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  return createPortal(
    <div
      ref={ref}
      popover="auto"
      role="dialog"
      aria-modal="true"
      className="shortcuts-dialog"
      aria-labelledby="shortcuts-title"
      onClick={(event) => {
        if (event.target === event.currentTarget) event.currentTarget.hidePopover();
      }}
      onKeyDown={(event) => {
        if (event.key !== "Tab") return;
        event.preventDefault();
        closeButton.current?.focus();
      }}
      onToggle={(event) => {
        const list = event.currentTarget;
        // The app is hidden once focus is in the list. WebView2 kept exposing the page with
        // aria-hidden on the React root element, so it goes on the app's own outer element.
        const app = document.querySelector(".desktop-shell");
        if (event.newState === "open") {
          returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
          closeButton.current?.focus({ preventScroll: true });
          app?.setAttribute("aria-hidden", "true");
          return;
        }
        app?.removeAttribute("aria-hidden");
        const active = document.activeElement;
        if (!active || active === document.body || list.contains(active)) returnFocus.current?.focus({ preventScroll: true });
        returnFocus.current = null;
      }}
    >
      <div className="shortcuts-head">
        <h2 id="shortcuts-title">Keyboard shortcuts</h2>
        <button ref={closeButton} type="button" aria-label="Close keyboard shortcuts" onClick={(event) => event.currentTarget.closest<HTMLElement>("[popover]")?.hidePopover()}>
          <X size={15} aria-hidden="true" />
        </button>
      </div>
      {GROUPS.map((group) => (
        <section key={group.title} aria-label={group.title}>
          <h3>{group.title}</h3>
          <dl>
            {group.items.map(({ keys, action, range }) => (
              <div key={action}>
                <dt>
                  {keys.map((combo, index) => (
                    <span key={combo.join("+")}>
                      {index ? <span className="shortcut-or">{range ? "to" : "or"}</span> : null}
                      {combo.map((key) => <kbd key={key}>{key}</kbd>)}
                    </span>
                  ))}
                </dt>
                <dd>{action}</dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
    </div>,
    document.body,
  );
}
