import { api } from "./api";
import { rowFingerprint, stableSignature } from "./session";
import { OptimizeRequestDto, SolvedBuildDto, UpgradePointDto, WeaponProfileDto } from "./types";

type CacheKey = string;
type CacheEntry<T> = {
  promise: Promise<T>;
  expiresAt: number;
  pending: boolean;
  subscribers: number;
};

const CACHE_TTL_MS = 15 * 60 * 1000;
const solveBuildCache = new Map<CacheKey, CacheEntry<SolvedBuildDto | null>>();
const upgradeSeriesCache = new Map<CacheKey, CacheEntry<UpgradePointDto[]>>();
const weaponProfileCache = new Map<CacheKey, CacheEntry<WeaponProfileDto>>();
let cacheDataVersion = "unknown";

export function setAnalysisCacheVersion(dataVersion: string): void {
  if (cacheDataVersion === dataVersion) return;
  cacheDataVersion = dataVersion;
  clearAnalysisCaches();
}

export function clearAnalysisCaches(): void {
  solveBuildCache.clear();
  upgradeSeriesCache.clear();
  weaponProfileCache.clear();
}

export function cachedWeaponProfile(profileId: string, weaponName: string, affinity: string | null, signal?: AbortSignal): Promise<WeaponProfileDto> {
  return cached(weaponProfileCache, 128, { profileId, weaponName, affinity }, () => api.weaponProfile(profileId, weaponName, affinity), signal);
}

export function cachedSolveBuild(
  base: OptimizeRequestDto,
  weaponName: string,
  affinity: string | null,
  aowName: string | null,
  signal?: AbortSignal,
): Promise<SolvedBuildDto | null> {
  return cached(solveBuildCache, 128, { base, weaponName, affinity, aowName }, () =>
    api.solveBuild(base, weaponName, affinity, aowName), signal);
}

export function cachedUpgradeSeries(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  maxUpgrade: number,
  signal?: AbortSignal,
): Promise<UpgradePointDto[]> {
  return cached(upgradeSeriesCache, 64, { base, solved: rowFingerprint(solved), maxUpgrade }, () =>
    api.buildUpgradeSeries(base, solved, maxUpgrade), signal);
}

function cached<T>(
  cache: Map<CacheKey, CacheEntry<T>>,
  maxEntries: number,
  keyParts: unknown,
  loader: () => Promise<T>,
  signal?: AbortSignal,
): Promise<T> {
  if (signal?.aborted) return Promise.reject(new Error("cancelled"));
  const key = stableSignature({ dataVersion: cacheDataVersion, keyParts });
  const now = Date.now();
  const entry = cache.get(key);
  if (entry && entry.expiresAt > now) {
    cache.delete(key);
    cache.set(key, entry);
    return subscribe(cache, key, entry, signal);
  }
  if (entry) cache.delete(key);
  let nextEntry!: CacheEntry<T>;
  const promise = loader().then((value) => {
    nextEntry.pending = false;
    return value;
  }).catch((error) => {
    nextEntry.pending = false;
    if (cache.get(key) === nextEntry) cache.delete(key);
    throw error;
  });
  nextEntry = { promise, expiresAt: now + CACHE_TTL_MS, pending: true, subscribers: 0 };
  cache.set(key, nextEntry);
  while (cache.size > maxEntries) {
    const oldest = cache.keys().next().value as string | undefined;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
  return subscribe(cache, key, nextEntry, signal);
}

function subscribe<T>(
  cache: Map<CacheKey, CacheEntry<T>>,
  key: CacheKey,
  entry: CacheEntry<T>,
  signal?: AbortSignal,
): Promise<T> {
  if (!entry.pending) return entry.promise;
  entry.subscribers += 1;
  let released = false;
  const release = () => {
    if (released) return;
    released = true;
    entry.subscribers -= 1;
    if (entry.pending && entry.subscribers === 0 && cache.get(key) === entry) {
      cache.delete(key);
    }
  };
  if (!signal) return entry.promise.finally(release);
  return new Promise<T>((resolve, reject) => {
    const abort = () => {
      release();
      reject(new Error("cancelled"));
    };
    signal.addEventListener("abort", abort, { once: true });
    entry.promise.then(
      (value) => {
        if (released) return;
        signal.removeEventListener("abort", abort);
        release();
        resolve(value);
      },
      (error: unknown) => {
        if (released) return;
        signal.removeEventListener("abort", abort);
        release();
        reject(error);
      },
    );
  });
}
