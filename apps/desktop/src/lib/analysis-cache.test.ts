import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { weaponProfile } = vi.hoisted(() => ({ weaponProfile: vi.fn() }));

vi.mock("./api", () => ({
  api: { weaponProfile },
}));

import { cachedWeaponProfile as cachedProfileWeaponProfile, clearAnalysisCaches, setAnalysisCacheVersion } from "./analysis-cache";
import type { WeaponProfileDto } from "./types";

const profile: WeaponProfileDto = {
  canChangeAow: true,
  nativeSkillName: "Stamp (Upward Cut)",
  requirements: { strStat: 16, dex: 13, intStat: 9, fai: 9, arc: 7 },
  maxUpgrade: 25,
  isSomber: false,
  disablesTwoHandBonus: false,
  forcesTwoHanding: false,
  weight: 9,
  moveCount: 5,
  oneHandedPoise: { light: "5", heavy: "10", chargedHeavy: "30", jumpingLight: "7.5", jumpingHeavy: "20" },
  twoHandedPoise: { light: "6.5", heavy: "11", chargedHeavy: "33", jumpingLight: "9.75", jumpingHeavy: "22" },
  affinities: ["Standard"],
  compatibleAows: ["Lion's Claw"],
};

const cachedWeaponProfile = (
  weaponName: string,
  affinity: string | null,
  signal?: AbortSignal,
) => cachedProfileWeaponProfile("vanilla", weaponName, affinity, signal);

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
    vi.useRealTimers();
    clearAnalysisCaches();
    weaponProfile.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("reuses a resolved cache hit without calling the loader again", async () => {
    weaponProfile.mockResolvedValue(profile);

    await expect(cachedWeaponProfile("Claymore", "Standard")).resolves.toEqual(profile);
    await expect(cachedWeaponProfile("Claymore", "Standard")).resolves.toEqual(profile);

    expect(weaponProfile).toHaveBeenCalledTimes(1);
  });

  it("keeps identical loadouts isolated across game profiles", async () => {
    weaponProfile.mockResolvedValue(profile);

    await cachedProfileWeaponProfile("vanilla", "Claymore", "Standard");
    await cachedProfileWeaponProfile("convergence", "Claymore", "Standard");

    expect(weaponProfile).toHaveBeenCalledTimes(2);
  });

  it("expires resolved entries after the TTL", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
    weaponProfile.mockResolvedValue(profile);

    await cachedWeaponProfile("Claymore", "Standard");
    vi.advanceTimersByTime(15 * 60 * 1000 - 1);
    await cachedWeaponProfile("Claymore", "Standard");
    expect(weaponProfile).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(2);
    await cachedWeaponProfile("Claymore", "Standard");
    expect(weaponProfile).toHaveBeenCalledTimes(2);
  });

  it("evicts the least-recently-used resolved entry at capacity", async () => {
    weaponProfile.mockResolvedValue(profile);
    for (let index = 0; index < 128; index += 1) {
      await cachedWeaponProfile(`Weapon ${index}`, "Standard");
    }
    await cachedWeaponProfile("Weapon 0", "Standard");
    await cachedWeaponProfile("Weapon 128", "Standard");
    await cachedWeaponProfile("Weapon 0", "Standard");
    await cachedWeaponProfile("Weapon 1", "Standard");

    expect(weaponProfile).toHaveBeenCalledTimes(130);
  });

  it("invalidates every entry when the data version changes", async () => {
    weaponProfile.mockResolvedValue(profile);
    setAnalysisCacheVersion("cache-test-v1");
    await cachedWeaponProfile("Claymore", "Standard");
    setAnalysisCacheVersion("cache-test-v1");
    await cachedWeaponProfile("Claymore", "Standard");
    setAnalysisCacheVersion("cache-test-v2");
    await cachedWeaponProfile("Claymore", "Standard");

    expect(weaponProfile).toHaveBeenCalledTimes(2);
  });

  it("stays bounded across a long session while retaining the hottest entries", async () => {
    weaponProfile.mockResolvedValue(profile);
    for (let index = 0; index < 2_000; index += 1) {
      await cachedWeaponProfile(`Session Weapon ${index}`, "Standard");
    }
    for (let index = 1_872; index < 2_000; index += 1) {
      await cachedWeaponProfile(`Session Weapon ${index}`, "Standard");
    }
    expect(weaponProfile).toHaveBeenCalledTimes(2_000);

    await cachedWeaponProfile("Session Weapon 0", "Standard");
    expect(weaponProfile).toHaveBeenCalledTimes(2_001);
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

  it("does not cache a late completion after every subscriber cancels", async () => {
    const abandoned = deferred<WeaponProfileDto>();
    const replacement = { ...profile, maxUpgrade: 10, isSomber: true };
    weaponProfile.mockReturnValueOnce(abandoned.promise).mockResolvedValueOnce(replacement);
    const controller = new AbortController();

    const cancelled = cachedWeaponProfile("Claymore", "Standard", controller.signal);
    controller.abort();
    await expect(cancelled).rejects.toThrow("cancelled");
    await expect(cachedWeaponProfile("Claymore", "Standard")).resolves.toEqual(replacement);
    abandoned.resolve(profile);
    await abandoned.promise;
    await expect(cachedWeaponProfile("Claymore", "Standard")).resolves.toEqual(replacement);

    expect(weaponProfile).toHaveBeenCalledTimes(2);
  });
});
