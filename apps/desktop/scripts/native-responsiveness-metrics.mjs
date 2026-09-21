export function sampleStatistics(samples) {
  const fields = ["heavyMs", "lightMs", "cancelMs", "totalMs"];
  const statistics = {};
  for (const field of fields) {
    const fieldValues = samples
      .filter((sample) => field !== "cancelMs" || sample.cancellationMeasured === true)
      .map((sample) => sample[field])
      .filter(Number.isFinite);
    if (fieldValues.length) {
      const sorted = [...fieldValues].sort((a, b) => a - b);
      statistics[field] = {
        count: sorted.length,
        min: sorted[0],
        median: median(sorted),
        max: sorted[sorted.length - 1],
      };
    }
  }
  return statistics;
}

export function medianSample(samples) {
  const statistics = sampleStatistics(samples);
  const fields = Object.entries(statistics);
  return fields.length
    ? Object.fromEntries(fields.map(([field, values]) => [field, values.median]))
    : undefined;
}

export function cancellationMeasured(cancelAccepted, finished) {
  if (typeof cancelAccepted !== "boolean") {
    throw new TypeError("cancelAccepted must be a boolean.");
  }
  if (!finished || typeof finished !== "object" || typeof finished.cancelled !== "boolean") {
    throw new TypeError("finished.cancelled must be a boolean.");
  }
  if (finished.error !== null && finished.error !== undefined) {
    throw new Error("finished.error must be null when validating cancellation.");
  }
  if (cancelAccepted && !finished.cancelled) {
    throw new Error("An accepted cancellation cannot publish a successful completion.");
  }
  if (finished.cancelled) {
    for (const field of ["result", "rows", "paths", "points", "frontier"]) {
      if (hasPayload(finished[field])) {
        throw new Error(`Cancelled terminal state contains nonempty ${field} payload.`);
      }
    }
  }
  return cancelAccepted === true && finished.cancelled === true;
}

function hasPayload(value) {
  if (value === null || value === undefined) return false;
  if (Array.isArray(value) || typeof value === "string") return value.length > 0;
  if (typeof value === "object") return Object.keys(value).length > 0;
  return true;
}

function median(sorted) {
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
}
