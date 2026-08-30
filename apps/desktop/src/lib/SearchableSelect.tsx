import { useEffect, useId, useMemo, useRef, useState } from "react";

export interface SelectOption {
  value: string | null;
  label: string;
}

export interface MultiSelectOption {
  value: string;
  label: string;
  count?: number;
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

export function CheckboxMultiSelect({
  label,
  values,
  excludedValues = [],
  options,
  onChange,
  disabled,
  placeholder = "All",
}: {
  label: string;
  values: string[];
  excludedValues?: string[];
  options: MultiSelectOption[];
  onChange: (values: string[], excludedValues: string[]) => void;
  disabled?: boolean;
  placeholder?: string;
}) {
  const id = useId();
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const selected = options.filter((option) => values.includes(option.value) || excludedValues.includes(option.value));
  const matching = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return options.filter((option) => !needle || option.label.toLocaleLowerCase().includes(needle));
  }, [options, query]);
  const filtered = matching.slice(0, 80);

  useEffect(() => {
    if (open) searchRef.current?.focus();
  }, [open]);

  function toggle(value: string) {
    const included = new Set(values);
    const excluded = new Set(excludedValues);
    if (included.delete(value)) excluded.add(value);
    else if (excluded.delete(value)) { /* neutral */ }
    else included.add(value);
    onChange(
      options.filter((option) => included.has(option.value)).map((option) => option.value),
      options.filter((option) => excluded.has(option.value)).map((option) => option.value),
    );
  }

  function close() {
    setOpen(false);
    setQuery("");
  }

  return (
    <div
      className="checkbox-multi-select"
      ref={wrapperRef}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) close();
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          close();
          triggerRef.current?.focus();
        }
      }}
    >
      <span className="checkbox-multi-label">{label}</span>
      <button
        ref={triggerRef}
        type="button"
        className="checkbox-multi-trigger"
        aria-label={label}
        aria-expanded={open && !disabled}
        aria-controls={`${id}-menu`}
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
      >
        {selected.length ? (
          <>
            {selected.slice(0, 2).map((option) => (
              <span className={`filter-chip${excludedValues.includes(option.value) ? " excluded" : ""}`} key={option.value}>
                {excludedValues.includes(option.value) ? `Not ${option.label}` : option.label}
              </span>
            ))}
            {selected.length > 2 ? <span className="filter-chip">+{selected.length - 2}</span> : null}
          </>
        ) : <span className="checkbox-multi-placeholder">{placeholder}</span>}
      </button>
      {open && !disabled ? (
        <div className="select-menu checkbox-select-menu" id={`${id}-menu`}>
          <input
            ref={searchRef}
            type="search"
            aria-label={`Search ${label}`}
            value={query}
            placeholder={`Search ${label.toLocaleLowerCase()}`}
            onChange={(event) => setQuery(event.target.value)}
          />
          <div className="checkbox-select-options" role="group" aria-label={label}>
            {filtered.map((option) => (
              <label key={option.value} data-mode={values.includes(option.value) ? "include" : excludedValues.includes(option.value) ? "exclude" : "neutral"}>
                <input
                  type="checkbox"
                  checked={values.includes(option.value)}
                  ref={(node) => { if (node) node.indeterminate = excludedValues.includes(option.value); }}
                  onChange={() => toggle(option.value)}
                />
                <span>{option.label}</span>
                {option.count === undefined ? null : <small>{option.count.toLocaleString()}</small>}
              </label>
            ))}
            {filtered.length === 0 ? <small>No matches</small> : null}
          </div>
          <div className="checkbox-select-footer">
            <small>{matching.length} {matching.length === 1 ? "match" : "matches"}</small>
            {values.length || excludedValues.length ? <button type="button" onClick={() => onChange([], [])}>Clear all</button> : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
