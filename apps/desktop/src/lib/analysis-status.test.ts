import { describe, expect, it } from "vitest";

import { analysisStatus, analysisStatusLabel } from "./analysis-status";

const base = {
  busy: false,
  resultSignature: "request-1",
  requestSignature: "request-1",
  hasResult: true,
  outcome: null as null,
};

describe("analysis lifecycle status", () => {
  it("reports each meaningful lifecycle state", () => {
    expect(analysisStatus({ ...base, busy: false })).toBe("completed");
    expect(analysisStatus({ ...base, busy: true })).toBe("running");
    expect(analysisStatus({ ...base, requestSignature: "request-2" })).toBe("stale");
    expect(analysisStatus({ ...base, resultSignature: null, hasResult: false, outcome: "cancelled" })).toBe("cancelled");
    expect(analysisStatus({ ...base, resultSignature: null, hasResult: false, outcome: "failed" })).toBe("failed");
    expect(analysisStatus({ ...base, resultSignature: null, hasResult: false })).toBe("ready");
  });

  it("uses concise labels for the progress strip", () => {
    expect(analysisStatusLabel("stale")).toBe("Stale results");
    expect(analysisStatusLabel("ready")).toBe("Ready");
  });
});
