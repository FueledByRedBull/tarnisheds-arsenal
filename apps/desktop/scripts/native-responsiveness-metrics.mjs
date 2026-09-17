export function medianSample(samples) {
  const fields = ["heavyMs", "lightMs", "cancelMs", "totalMs"];
  const values = fields
    .map((field) => [field, samples
      .filter((sample) => field !== "cancelMs" || sample.cancellationMeasured === true)
      .map((sample) => sample[field])
      .filter(Number.isFinite)])
    .filter(([, fieldValues]) => fieldValues.length);
  return values.length
    ? Object.fromEntries(values.map(([field, fieldValues]) => [field, median(fieldValues)]))
    : undefined;
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
}
