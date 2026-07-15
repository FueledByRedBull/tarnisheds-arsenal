import { beforeEach, describe, expect, it, vi } from "vitest";

const { weaponProfile } = vi.hoisted(() => ({ weaponProfile: vi.fn() }));

vi.mock("./api", () => ({
  api: { weaponProfile },
}));

import { cachedWeaponProfile, clearAnalysisCaches } from "./analysis-cache";
import type { WeaponProfileDto } from "./types";

const profile: WeaponProfileDto = {
  requirements: { strStat: 16, dex: 13, intStat: 9, fai: 9, arc: 7 },
  maxUpgrade: 25,
  isSomber: false,
  disablesTwoHandBonus: false,
  affinities: ["Standard"],
  compatibleAows: ["Lion's Claw"],
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("analysis cache", () => {
  beforeEach(() => {
    clearAnalysisCaches();
    weaponProfile.mockReset();
  });

  it("shares one in-flight request for duplicate subscribers", async () => {
    const pending = deferred<WeaponProfileDto>();
    weaponProfile.mockReturnValueOnce(pending.promise);

    const first = cachedWeaponProfile("Claymore", "Standard");
    const second = cachedWeaponProfile("Claymore", "Standard");
    pending.resolve(profile);

    await expect(first).resolves.toEqual(profile);
    await expect(second).resolves.toEqual(profile);
    expect(weaponProfile).toHaveBeenCalledTimes(1);
  });

  it("evicts rejected work so a retry can succeed", async () => {
    weaponProfile.mockRejectedValueOnce(new Error("cancelled"));
    weaponProfile.mockResolvedValueOnce(profile);

    await expect(cachedWeaponProfile("Claymore", "Standard")).rejects.toThrow("cancelled");
    await expect(cachedWeaponProfile("Claymore", "Standard")).resolves.toEqual(profile);
    expect(weaponProfile).toHaveBeenCalledTimes(2);
  });

  it("does not let an abandoned rejection evict newer work with the same key", async () => {
    const abandoned = deferred<WeaponProfileDto>();
    const replacement = deferred<WeaponProfileDto>();
    weaponProfile
      .mockReturnValueOnce(abandoned.promise)
      .mockReturnValueOnce(replacement.promise);

    const oldPromise = cachedWeaponProfile("Claymore", "Standard");
    clearAnalysisCaches();
    const newPromise = cachedWeaponProfile("Claymore", "Standard");
    abandoned.reject(new Error("cancelled"));
    await expect(oldPromise).rejects.toThrow("cancelled");

    const sharedReplacement = cachedWeaponProfile("Claymore", "Standard");
    replacement.resolve(profile);
    await expect(newPromise).resolves.toEqual(profile);
    await expect(sharedReplacement).resolves.toEqual(profile);
    expect(weaponProfile).toHaveBeenCalledTimes(2);
  });

  it("keeps shared work while one subscriber remains", async () => {
    const pending = deferred<WeaponProfileDto>();
    weaponProfile.mockReturnValueOnce(pending.promise);
    const firstController = new AbortController();
    const secondController = new AbortController();

    const first = cachedWeaponProfile("Claymore", "Standard", firstController.signal);
    const second = cachedWeaponProfile("Claymore", "Standard", secondController.signal);
    firstController.abort();

    await expect(first).rejects.toThrow("cancelled");
    const third = cachedWeaponProfile("Claymore", "Standard");
    pending.resolve(profile);
    await expect(second).resolves.toEqual(profile);
    await expect(third).resolves.toEqual(profile);
    expect(weaponProfile).toHaveBeenCalledTimes(1);
  });

  it("evicts in-flight work after every subscriber cancels", async () => {
    const abandoned = deferred<WeaponProfileDto>();
    weaponProfile.mockReturnValueOnce(abandoned.promise).mockResolvedValueOnce(profile);
    const firstController = new AbortController();
    const secondController = new AbortController();

    const first = cachedWeaponProfile("Claymore", "Standard", firstController.signal);
    const second = cachedWeaponProfile("Claymore", "Standard", secondController.signal);
    firstController.abort();
    secondController.abort();

    await expect(first).rejects.toThrow("cancelled");
    await expect(second).rejects.toThrow("cancelled");
    await expect(cachedWeaponProfile("Claymore", "Standard")).resolves.toEqual(profile);
    expect(weaponProfile).toHaveBeenCalledTimes(2);
    abandoned.resolve(profile);
  });
});
