import { describe, expect, it } from "vitest";

import { chartY, contiguousMetricSegments, metricRatio, niceTicks, paddedMetricDomain } from "./chart";

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

  it("labels axes with round values inside the domain", () => {
    expect(niceTicks({ min: 460, max: 1040 })).toEqual([1000, 800, 600]);
    expect(niceTicks({ min: 612.3, max: 1018.4 })).toEqual([1000, 900, 800, 700]);
    expect(niceTicks(paddedMetricDomain([500, 500]))).toEqual([510, 505, 500, 495, 490]);
    expect(niceTicks({ min: 0.1, max: 1.2 })).toEqual([1, 0.75, 0.5, 0.25]);
  });

  it("maps metrics to the shared plot height and parks missing values on the floor", () => {
    const domain = { min: 0, max: 100 };
    expect(chartY(100, domain)).toBe(14);
    expect(chartY(0, domain)).toBe(206);
    expect(chartY(null, domain)).toBe(206);
  });

  it("does not bridge unavailable metrics", () => {
    const points = [{ metric: 100 }, { metric: null }, { metric: 120 }, { metric: 130 }];

    expect(contiguousMetricSegments(points)).toEqual([
      [{ metric: 100 }],
      [{ metric: 120 }, { metric: 130 }],
    ]);
  });
});
