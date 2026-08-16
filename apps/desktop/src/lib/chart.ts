export interface MetricDomain {
  min: number;
  max: number;
}

export function paddedMetricDomain(values: number[]): MetricDomain {
  if (values.length === 0) return { min: 0, max: 1 };

  const rawMin = Math.min(...values);
  const rawMax = Math.max(...values);
  const spread = rawMax - rawMin;
  const padding = spread > 0
    ? spread * 0.08
    : Math.max(Math.abs(rawMax) * 0.02, 1);

  return {
    min: rawMin - padding,
    max: rawMax + padding,
  };
}

export function metricRatio(metric: number | null, domain: MetricDomain): number | null {
  if (metric === null) return null;
  return Math.min(1, Math.max(0, (metric - domain.min) / Math.max(domain.max - domain.min, Number.EPSILON)));
}

export function contiguousMetricSegments<T extends { metric: number | null }>(points: T[]): T[][] {
  const segments: T[][] = [];
  let segment: T[] | null = null;
  for (const point of points) {
    if (point.metric === null) {
      segment = null;
      continue;
    }
    if (!segment) {
      segment = [];
      segments.push(segment);
    }
    segment.push(point);
  }
  return segments;
}
