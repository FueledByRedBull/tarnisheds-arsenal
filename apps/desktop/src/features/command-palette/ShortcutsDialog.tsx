import { X } from "lucide-react";
import { Ref } from "react";

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
// and closing stay native, so neither re-renders the query strip that owns the list.
export function ShortcutsDialog({ ref }: { ref: Ref<HTMLDialogElement> }) {
  return (
    <dialog
      ref={ref}
      className="shortcuts-dialog"
      aria-labelledby="shortcuts-title"
      onClick={(event) => {
        if (event.target === event.currentTarget) event.currentTarget.close();
      }}
    >
      <div className="shortcuts-head">
        <h2 id="shortcuts-title">Keyboard shortcuts</h2>
        <button type="button" aria-label="Close keyboard shortcuts" onClick={(event) => event.currentTarget.closest("dialog")?.close()}>
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
    </dialog>
  );
}
