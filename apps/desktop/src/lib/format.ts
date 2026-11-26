// One formatter: constructing it per call cost ~5 ms per 50 rendered rows.
const wholeNumber = new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 });

export function compactNumber(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "-";
  }
  return wholeNumber.format(value);
}

export function fixed1(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "-";
  }
  return value.toFixed(1);
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") {
    return error.message;
  }
  return String(error);
}
