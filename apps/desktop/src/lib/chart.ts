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

// Both line charts draw into a 1000 x 220 view box: levels span x 12-988, metrics y 206-14.
export function chartX(level: number, firstLevel: number, lastLevel: number): number {
  return lastLevel === firstLevel ? 500 : 12 + ((level - firstLevel) / (lastLevel - firstLevel)) * 976;
}

export function chartY(metric: number | null, domain: MetricDomain): number {
  return 206 - (metricRatio(metric, domain) ?? 0) * 192;
}

/** Round axis values inside the domain, top first: steps of 1, 2, 2.5 or 5 times a power of ten. */
export function niceTicks({ min, max }: MetricDomain): number[] {
  const rough = (max - min) / 5;
  const power = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 2.5, 5, 10].map((factor) => factor * power).find((candidate) => candidate >= rough) ?? rough;
  const ticks: number[] = [];
  for (let index = Math.ceil(min / step); index * step <= max; index += 1) ticks.push(Number((index * step).toPrecision(12)));
  return ticks.reverse();
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
