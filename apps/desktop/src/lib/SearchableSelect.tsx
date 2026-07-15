import { useEffect, useId, useMemo, useRef, useState } from "react";

export interface SelectOption {
  value: string | null;
  label: string;
}

export function SearchableSelect({
  label,
  value,
  options,
  onChange,
  disabled,
  placeholder = "Open",
}: {
  label: string;
  value: string | null;
  options: SelectOption[];
  onChange: (value: string | null) => void;
  disabled?: boolean;
  placeholder?: string;
}) {
  const id = useId();
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const blurTimeoutRef = useRef<number | null>(null);
  const selectedLabel =
    options.find((option) => option.value === value)?.label ?? (value ?? "");
  const shownValue = open ? query : selectedLabel;
  const matching = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) {
      return options;
    }
    return options
      .filter((option) => option.label.toLocaleLowerCase().includes(needle));
  }, [options, query]);
  const filtered = matching.slice(0, 80);
  const resultStatus = matching.length === 0
    ? "No matches"
    : matching.length > filtered.length
      ? `Showing ${filtered.length} of ${matching.length} matches. Type to narrow the list.`
      : `${matching.length} ${matching.length === 1 ? "match" : "matches"}`;

  const active = filtered[Math.min(activeIndex, Math.max(filtered.length - 1, 0))];

  useEffect(
    () => () => {
      if (blurTimeoutRef.current !== null) {
        window.clearTimeout(blurTimeoutRef.current);
      }
    },
    [],
  );

  function cancelPendingBlur() {
    if (blurTimeoutRef.current === null) {
      return;
    }
    window.clearTimeout(blurTimeoutRef.current);
    blurTimeoutRef.current = null;
  }

  function choose(option: SelectOption | undefined) {
    if (!option) {
      return;
    }
    cancelPendingBlur();
    onChange(option.value);
    setOpen(false);
    setQuery("");
    setActiveIndex(0);
  }

  function commitTypedValue() {
    const typed = query.trim();
    if (!typed) {
      return;
    }
    const exact = options.find(
      (option) => option.label.toLocaleLowerCase() === typed.toLocaleLowerCase(),
    );
    if (exact) {
      choose(exact);
    }
  }

  return (
    <label className="searchable-select" htmlFor={`${id}-input`}>
      {label}
      <input
        id={`${id}-input`}
        role="combobox"
        aria-label={label}
        aria-autocomplete="list"
        aria-expanded={open && !disabled}
        aria-controls={`${id}-listbox`}
        aria-describedby={`${id}-status`}
        aria-activedescendant={open && active ? `${id}-option-${Math.min(activeIndex, filtered.length - 1)}` : undefined}
        disabled={disabled}
        value={shownValue}
        placeholder={placeholder}
        onBlur={() => {
          cancelPendingBlur();
          blurTimeoutRef.current = window.setTimeout(() => {
            blurTimeoutRef.current = null;
            commitTypedValue();
            setOpen(false);
            setQuery("");
            setActiveIndex(0);
          }, 120);
        }}
        onChange={(event) => {
          setQuery(event.target.value);
          setOpen(true);
          setActiveIndex(0);
        }}
        onFocus={() => {
          cancelPendingBlur();
          setOpen(true);
          setQuery("");
          setActiveIndex(0);
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            setOpen(false);
            setQuery("");
            setActiveIndex(0);
            return;
          }
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            setOpen(true);
            const direction = event.key === "ArrowDown" ? 1 : -1;
            setActiveIndex((current) => {
              const count = Math.max(filtered.length, 1);
              return (current + direction + count) % count;
            });
            return;
          }
          if (event.key === "Enter") {
            event.preventDefault();
            const exact = filtered.find(
              (option) => option.label.toLocaleLowerCase() === query.trim().toLocaleLowerCase(),
            );
            choose(exact ?? active ?? filtered.find((option) => option.value !== null) ?? filtered[0]);
            return;
          }
          if (event.key === "Tab") {
            commitTypedValue();
            setOpen(false);
            setQuery("");
            setActiveIndex(0);
          }
        }}
      />
      {open && !disabled ? (
        <div className="select-menu" id={`${id}-listbox`} role="listbox">
          {filtered.map((option, index) => (
            <button
              key={`${option.value ?? "open"}-${option.label}`}
              id={`${id}-option-${index}`}
              role="option"
              aria-selected={option.value === value}
              className={index === activeIndex ? "active" : undefined}
              type="button"
              onMouseDown={(event) => event.preventDefault()}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={() => {
                choose(option);
              }}
            >
              {option.label}
            </button>
          ))}
          {filtered.length === 0 ? <span>No matches</span> : null}
          <small id={`${id}-status`} className="select-status" role="status" aria-live="polite">
            {resultStatus}
          </small>
        </div>
      ) : <span id={`${id}-status`} className="sr-only">{options.length} options</span>}
    </label>
  );
}

export function openOption(label = "Open"): SelectOption {
  return { value: null, label };
}
