import { KeyboardEvent, useEffect, useRef, useState } from "react";

// Commits on Enter, blur, or after a short idle pause, so typing "40" never searches with "4".
export function DraftNumberInput({
  value,
  min,
  max,
  onCommit,
  onDraftChange,
  readOnly = false,
  className,
}: {
  value: number;
  min: number;
  max: number;
  onCommit: (value: number) => void;
  onDraftChange?: () => void;
  readOnly?: boolean;
  className?: string;
}) {
  const [draft, setDraft] = useState(String(value));
  const idleCommit = useRef<number | null>(null);

  function clearIdleCommit() {
    if (idleCommit.current !== null) {
      window.clearTimeout(idleCommit.current);
      idleCommit.current = null;
    }
  }

  useEffect(() => {
    clearIdleCommit();
    setDraft(String(value));
  }, [value]);

  useEffect(() => () => clearIdleCommit(), []);

  function commit(raw: string) {
    clearIdleCommit();
    const parsed = parseInteger(raw);
    if (!Number.isInteger(parsed)) {
      setDraft(String(value));
      return;
    }
    const next = clamp(parsed, min, max);
    setDraft(String(next));
    if (next !== value) {
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
    <input
      type="number"
      className={className}
      min={min}
      max={max}
      readOnly={readOnly}
      value={draft}
      onBlur={(event) => commit(event.target.value)}
      onChange={(event) => {
        const next = event.target.value;
        clearIdleCommit();
        setDraft(next);
        if (next !== String(value)) {
          onDraftChange?.();
        }
        const parsed = parseInteger(next);
        if (Number.isInteger(parsed) && parsed >= min && parsed <= max && parsed !== value) {
          idleCommit.current = window.setTimeout(() => commit(next), 700);
        }
      }}
      onKeyDown={handleKeyDown}
    />
  );
}

function parseInteger(value: string): number {
  if (!/^\d+$/.test(value.trim())) {
    return Number.NaN;
  }
  return Number(value);
}

function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
