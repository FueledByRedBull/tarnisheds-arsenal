import { Command as CommandIcon } from "lucide-react";
import { KeyboardEvent, ReactNode, useDeferredValue, useEffect, useId, useMemo, useRef, useState } from "react";
import { Command, CommandAction, CommandContext, findCommands, highlightRanges, recallable } from "../../lib/commands";

const RECENT_KEY = "tarnisheds-arsenal.recentCommands.v1";

function readRecent(): string[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(stored) ? stored.filter((id): id is string => typeof id === "string").slice(0, 5) : [];
  } catch {
    return [];
  }
}

function writeRecent(ids: string[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(ids));
  } catch {
    // Without storage, recent commands last for this session only.
  }
}

function Highlighted({ text, query }: { text: string; query: string }) {
  const parts: ReactNode[] = [];
  let cursor = 0;
  for (const [start, end] of highlightRanges(text, query)) {
    if (start > cursor) parts.push(text.slice(cursor, start));
    parts.push(<mark key={start}>{text.slice(start, end)}</mark>);
    cursor = end;
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return <>{parts}</>;
}

// Ctrl+K editing: type "str 40", "uchigatana" or "bleed" and apply with Enter. A native modal
// dialog traps focus and closes on Escape; results come from the same actions as the controls.
export function CommandPalette({ open, context, onRun, onClose }: {
  open: boolean;
  context: CommandContext | null;
  onRun: (action: CommandAction) => void;
  onClose: () => void;
}) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const [activeIndex, setActiveIndex] = useState(0);
  const [recent, setRecent] = useState(readRecent);
  const results = useMemo(
    () => (context ? findCommands(deferredQuery, { ...context, recent }) : []),
    [context, deferredQuery, recent],
  );
  const active = Math.min(activeIndex, Math.max(results.length - 1, 0));

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (open && !element.open) {
      element.showModal();
      input.current?.select();
    } else if (!open && element.open) {
      element.close();
    }
  }, [open]);

  function run(command: Command | undefined) {
    if (!command) return;
    if (recallable(command)) {
      const next = [command.id, ...recent.filter((id) => id !== command.id)].slice(0, 5);
      setRecent(next);
      writeRecent(next);
    }
    onRun(command.action);
    setQuery("");
    setActiveIndex(0);
    onClose();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!results.length) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActiveIndex((active + step + results.length) % results.length);
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      setActiveIndex(event.key === "Home" ? 0 : Math.max(results.length - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      run(results[active]);
    }
  }

  return (
    <dialog
      ref={dialog}
      className="command-palette"
      aria-label="Edit anything"
      onClose={onClose}
      onClick={(event) => {
        if (event.target === dialog.current) onClose();
      }}
    >
      <div className="palette-input">
        <CommandIcon size={16} aria-hidden="true" />
        <input
          ref={input}
          role="combobox"
          aria-label="Command"
          aria-expanded="true"
          aria-controls={`${id}-results`}
          aria-activedescendant={results.length ? `${id}-option-${active}` : undefined}
          aria-autocomplete="list"
          autoComplete="off"
          spellCheck={false}
          placeholder="Type a stat, weapon, skill, objective or action…"
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setActiveIndex(0);
          }}
          onKeyDown={handleKeyDown}
        />
      </div>
      <ul className="palette-results" id={`${id}-results`} role="listbox" aria-label="Matching commands">
        {results.map((command, index) => (
          <li
            key={command.id}
            id={`${id}-option-${index}`}
            role="option"
            aria-selected={index === active}
            className={index === active ? "active" : undefined}
            onMouseMove={() => setActiveIndex(index)}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => run(command)}
          >
            <span className="palette-group">{command.group}</span>
            <span className="palette-label"><Highlighted text={command.label} query={deferredQuery} /></span>
            {command.detail ? <small>{command.detail}</small> : null}
          </li>
        ))}
      </ul>
      <p className="palette-empty" role="status">
        {results.length ? "" : "No command matches. Try a stat like \"dex 40\", a weapon or a skill."}
      </p>
      <p className="palette-help">
        <span><kbd>Up</kbd> <kbd>Down</kbd> to move</span>
        <span><kbd>Enter</kbd> to apply</span>
        <span><kbd>Esc</kbd> to close</span>
      </p>
    </dialog>
  );
}
