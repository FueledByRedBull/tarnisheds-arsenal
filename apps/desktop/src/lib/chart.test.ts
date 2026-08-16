import { describe, expect, it } from "vitest";

import { contiguousMetricSegments, metricRatio, paddedMetricDomain } from "./chart";

describe("chart metric domains", () => {
  it("frames the observed range instead of anchoring positive values to zero", () => {
    const domain = paddedMetricDomain([734.7, 773.5]);

    expect(domain.min).toBeGreaterThan(700);
    expect(domain.min).toBeLessThan(734.7);
    expect(domain.max).toBeGreaterThan(773.5);
  });

  it("gives a constant series a visible, finite range", () => {
    const domain = paddedMetricDomain([500, 500]);

    expect(domain.max).toBeGreaterThan(domain.min);
    expect(metricRatio(500, domain)).toBeCloseTo(0.5);
  });

  it("clamps ratios and preserves missing values", () => {
    const domain = { min: 10, max: 20 };

    expect(metricRatio(null, domain)).toBeNull();
    expect(metricRatio(5, domain)).toBe(0);
    expect(metricRatio(25, domain)).toBe(1);
  });

  it("keeps small but meaningful changes visible", () => {
    const domain = paddedMetricDomain([734.7, 734.8]);

    expect(metricRatio(734.8, domain)! - metricRatio(734.7, domain)!).toBeGreaterThan(0.8);
  });

  it("does not bridge unavailable metrics", () => {
    const points = [{ metric: 100 }, { metric: null }, { metric: 120 }, { metric: 130 }];

    expect(contiguousMetricSegments(points)).toEqual([
      [{ metric: 100 }],
      [{ metric: 120 }, { metric: 130 }],
    ]);
  });
});
