import { useMemo, useState } from "react";

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
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const selectedLabel =
    options.find((option) => option.value === value)?.label ?? (value ?? "");
  const shownValue = open ? query : selectedLabel;
  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) {
      return options.slice(0, 80);
    }
    return options
      .filter((option) => option.label.toLocaleLowerCase().includes(needle))
      .slice(0, 80);
  }, [options, query]);

  return (
    <label className="searchable-select">
      {label}
      <input
        disabled={disabled}
        value={shownValue}
        placeholder={placeholder}
        onBlur={() => {
          window.setTimeout(() => {
            const typed = query.trim();
            if (typed) {
              const exact = options.find(
                (option) => option.label.toLocaleLowerCase() === typed.toLocaleLowerCase(),
              );
              if (exact) {
                onChange(exact.value);
              }
            }
            setOpen(false);
            setQuery("");
          }, 120);
        }}
        onChange={(event) => {
          setQuery(event.target.value);
          setOpen(true);
        }}
        onFocus={() => {
          setOpen(true);
          setQuery("");
        }}
        onKeyDown={(event) => {
          if (event.key !== "Enter") {
            return;
          }
          event.preventDefault();
          const exact = filtered.find(
            (option) => option.label.toLocaleLowerCase() === query.trim().toLocaleLowerCase(),
          );
          const chosen = exact ?? filtered.find((option) => option.value !== null) ?? filtered[0];
          if (chosen) {
            onChange(chosen.value);
            setOpen(false);
            setQuery("");
          }
        }}
      />
      {open && !disabled ? (
        <div className="select-menu">
          {filtered.map((option) => (
            <button
              key={`${option.value ?? "open"}-${option.label}`}
              type="button"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                onChange(option.value);
                setOpen(false);
                setQuery("");
              }}
            >
              {option.label}
            </button>
          ))}
          {filtered.length === 0 ? <span>No matches</span> : null}
        </div>
      ) : null}
    </label>
  );
}

export function openOption(label = "Open"): SelectOption {
  return { value: null, label };
}
