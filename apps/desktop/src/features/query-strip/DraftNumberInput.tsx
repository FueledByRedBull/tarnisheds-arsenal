import { KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { announce } from "../../lib/announce";
import { flashChanged } from "../../lib/motion";
import { clamp } from "../../lib/session";

// Commits on Enter, blur, or after a short idle pause, so typing "40" never searches with "4".
export function DraftNumberInput({
  value,
  min,
  max,
  onCommit,
  description,
  describedBy,
  disabled,
}: {
  value: number;
  min: number;
  max: number;
  onCommit: (value: number) => void;
  /** Read by screen readers after the name, e.g. a shortfall shown only as colour and a tooltip. */
  description?: string;
  /** Ids of visible elements that explain this input, read after its range. */
  describedBy?: string;
  disabled?: boolean;
}) {
  const [draft, setDraft] = useState(String(value));
  const [syncedValue, setSyncedValue] = useState(value);
  if (syncedValue !== value) {
    setSyncedValue(value);
    setDraft(String(value));
  }
  const idleCommit = useRef<number | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const shownValue = useRef(value);
  const descriptionId = useId();
  const rangeId = useId();
  const rangeText = `Allowed ${min} to ${max}`;
  // A draft outside the range is marked until it is committed (limited) or replaced.
  const typed = parseInteger(draft);
  const outOfRange = draft.trim() !== "" && (!Number.isInteger(typed) || typed < min || typed > max);

  function clearIdleCommit() {
    if (idleCommit.current !== null) {
      window.clearTimeout(idleCommit.current);
      idleCommit.current = null;
    }
  }

  useEffect(() => {
    clearIdleCommit();
    // A value changed from elsewhere (class, palette, undo) flashes; typing here does not.
    if (shownValue.current !== value && document.activeElement !== input.current) flashChanged(input.current);
    shownValue.current = value;
  }, [value]);

  useEffect(() => () => clearIdleCommit(), []);

  function commit(raw: string) {
    clearIdleCommit();
    const parsed = parseInteger(raw);
    if (!Number.isInteger(parsed)) {
      setDraft(String(value));
      announce(`Not a whole number between ${min} and ${max}; kept ${value}`);
      return;
    }
    const next = clamp(parsed, min, max);
    setDraft(String(next));
    if (next !== parsed) announce(`Limited to ${next} (allowed ${min}-${max})`);
    if (next !== value) {
      // Already on screen: committing (and the blur after Enter) must not flash it as news.
      shownValue.current = next;
      onCommit(next);
    }
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      commit(event.currentTarget.value);
      event.currentTarget.blur();
    }
  }

  return (
    <>
    <input
      ref={input}
      type="number"
      aria-describedby={[description ? descriptionId : null, rangeId, describedBy].filter(Boolean).join(" ")}
      aria-invalid={outOfRange || undefined}
      disabled={disabled}
      min={min}
      max={max}
      value={draft}
      onBlur={(event) => commit(event.target.value)}
      onChange={(event) => {
        const next = event.target.value;
        clearIdleCommit();
        setDraft(next);
        const parsed = parseInteger(next);
        if (Number.isInteger(parsed) && parsed >= min && parsed <= max && parsed !== value) {
          idleCommit.current = window.setTimeout(() => commit(next), 700);
        }
      }}
      onKeyDown={handleKeyDown}
    />
    <span id={rangeId} hidden>{rangeText}</span>
    {description ? <span id={descriptionId} hidden>{description}</span> : null}
    </>
  );
}

function parseInteger(value: string): number {
  if (!/^\d+$/.test(value.trim())) {
    return Number.NaN;
  }
  return Number(value);
}
