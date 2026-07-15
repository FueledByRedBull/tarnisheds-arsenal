import { describe, expect, it } from "vitest";
import { LatestRequest } from "./request-generation";

describe("LatestRequest", () => {
  it("rejects an older completion after a replacement starts", () => {
    const requests = new LatestRequest();
    const older = requests.begin("weapon=A");
    const newer = requests.begin("weapon=B");

    expect(requests.isCurrent(older)).toBe(false);
    expect(requests.isCurrent(newer)).toBe(true);
  });

  it("invalidates an in-flight request on unmount", () => {
    const requests = new LatestRequest();
    const token = requests.begin("profile=Claymore");

    requests.invalidate(token);

    expect(requests.isCurrent(token)).toBe(false);
  });

  it("keeps rapid start-cancel-restart generations distinct", () => {
    const requests = new LatestRequest();
    const first = requests.begin("search=one");
    requests.invalidate(first);
    const second = requests.begin("search=one");

    expect(second.generation).toBeGreaterThan(first.generation);
    expect(requests.isCurrent(first)).toBe(false);
    expect(requests.isCurrent(second)).toBe(true);
  });
});
