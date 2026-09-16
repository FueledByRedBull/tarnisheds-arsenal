import assert from "node:assert/strict";
import test from "node:test";
import { medianSample } from "./native-responsiveness-metrics.mjs";

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
