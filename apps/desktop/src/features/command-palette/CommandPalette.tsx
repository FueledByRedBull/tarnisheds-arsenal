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

// Ctrl+K editing: type "str 40", "uchigatana" or "bleed" and apply with Enter. A native
// popover closes on Escape or an outside click; unlike a modal dialog it does not make the
// rest of the page inert, which would restyle every element on each open.
export function CommandPalette({ open, context, onRun, onClose }: {
  open: boolean;
  context: CommandContext | null;
  onRun: (action: CommandAction) => void;
  onClose: () => void;
}) {
  const id = useId();
  const panel = useRef<HTMLDivElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const latestOnClose = useRef(onClose);
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const [activeIndex, setActiveIndex] = useState(0);
  const [recent, setRecent] = useState(readRecent);
  // A closed palette stays mounted for an instant first open, so it skips the search.
  const results = useMemo(
    () => (open && context ? findCommands(deferredQuery, { ...context, recent }) : []),
    [context, deferredQuery, open, recent],
  );
  const active = Math.min(activeIndex, Math.max(results.length - 1, 0));

  useEffect(() => {
    latestOnClose.current = onClose;
  }, [onClose]);

  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    // Escape and outside clicks close the popover natively. The open state follows at once, in
    // beforetoggle: the toggle event arrives a task later, and a Ctrl+K in between would toggle
    // the stale state shut instead of reopening. Focus returns to where it was only when it was
    // in the palette or got lost; a click that closed it keeps its new target.
    const onBeforeToggle = (event: Event) => {
      if ((event as ToggleEvent).newState === "closed") latestOnClose.current();
    };
    const onToggle = (event: Event) => {
      if ((event as ToggleEvent).newState !== "closed") return;
      const active = document.activeElement;
      const focusLost = !active || active === document.body || element.contains(active);
      if (focusLost && returnFocus.current?.isConnected && !element.contains(returnFocus.current)) {
        returnFocus.current.focus({ preventScroll: true });
      }
      returnFocus.current = null;
    };
    element.addEventListener("beforetoggle", onBeforeToggle);
    element.addEventListener("toggle", onToggle);
    return () => {
      element.removeEventListener("beforetoggle", onBeforeToggle);
      element.removeEventListener("toggle", onToggle);
    };
  }, []);

  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    const shown = element.matches(":popover-open");
    if (open && !shown) {
      returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      element.showPopover();
      input.current?.focus({ preventScroll: true });
      input.current?.select();
    } else if (!open && shown) {
      element.hidePopover();
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
      // The list follows a deferred query; Enter applies what is typed, even before it renders.
      const current = deferredQuery === query && results.length ? results
        : context ? findCommands(query, { ...context, recent }) : [];
      run(current[current === results ? active : 0]);
    }
  }

  return (
    <div
      ref={panel}
      popover="auto"
      role="dialog"
      className="command-palette"
      aria-label="Edit anything"
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
    </div>
  );
}
