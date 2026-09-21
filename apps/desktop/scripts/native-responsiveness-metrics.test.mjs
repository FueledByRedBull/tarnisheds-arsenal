import assert from "node:assert/strict";
import test from "node:test";
import { cancellationMeasured, medianSample, sampleStatistics } from "./native-responsiveness-metrics.mjs";

test("sampleStatistics reports finite count, range, and even median", () => {
  assert.deepEqual(
    sampleStatistics([
      { heavyMs: 10 },
      { heavyMs: 14 },
      { heavyMs: 12 },
      { heavyMs: Number.NaN },
      { heavyMs: Number.POSITIVE_INFINITY },
    ]),
    { heavyMs: { count: 3, min: 10, median: 12, max: 14 } },
  );
});

test("sampleStatistics omits inconclusive cancellation timings", () => {
  assert.deepEqual(
    sampleStatistics([
      { cancelMs: 100, cancellationMeasured: false },
      { cancelMs: 4, cancellationMeasured: true },
      { cancelMs: 8, cancellationMeasured: true },
    ]),
    { cancelMs: { count: 2, min: 4, median: 6, max: 8 } },
  );
});

test("medianSample ignores cancelMs from inconclusive samples", () => {
  assert.deepEqual(
    medianSample([
      { lightMs: 10, cancelMs: 100, totalMs: 30, cancellationMeasured: false },
      { lightMs: 14, cancelMs: 4, totalMs: 34, cancellationMeasured: true },
      { lightMs: 12, cancelMs: 8, totalMs: 32, cancellationMeasured: true },
    ]),
    { lightMs: 12, cancelMs: 6, totalMs: 32 },
  );
});

test("medianSample returns no median when no timing is measured", () => {
  assert.equal(medianSample([{ cancelMs: 100, cancellationMeasured: false }]), undefined);
});

test("cancellationMeasured accepts a clean terminal cancellation", () => {
  assert.equal(
    cancellationMeasured(true, {
      cancelled: true,
      error: null,
      result: null,
      rows: [],
      paths: [],
      points: [],
      frontier: [],
    }),
    true,
  );
});

test("cancellationMeasured returns false when work completed before cancellation", () => {
  assert.equal(cancellationMeasured(false, { cancelled: false, error: null }), false);
});

test("cancellationMeasured rejects invalid terminal states", () => {
  assert.throws(
    () => cancellationMeasured("true", { cancelled: true, error: null }),
    /cancelAccepted.*boolean/,
  );
  assert.throws(
    () => cancellationMeasured(false, { cancelled: undefined, error: null }),
    /cancelled.*boolean/,
  );
  assert.throws(
    () => cancellationMeasured(false, { cancelled: false, error: "fatal" }),
    /error/,
  );
  assert.throws(
    () => cancellationMeasured(true, { cancelled: false, error: null }),
    /accepted cancellation.*successful/i,
  );
  for (const field of ["result", "rows", "paths", "points", "frontier"]) {
    assert.throws(
      () => cancellationMeasured(true, { cancelled: true, error: null, [field]: [{}] }),
      new RegExp(`${field}.*payload`, "i"),
    );
  }
});
