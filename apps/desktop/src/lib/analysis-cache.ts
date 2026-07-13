import { api } from "./api";
import { rowFingerprint, stableSignature } from "./session";
import { AffinityWatchPayloadDto, OptimizeRequestDto, PathPreviewDto, ScalingDto, SolvedBuildDto, UpgradePointDto, WeaponProfileDto } from "./types";

type CacheKey = string;
type CacheEntry<T> = {
  promise: Promise<T>;
  expiresAt: number;
};

const CACHE_TTL_MS = 15 * 60 * 1000;
const solveBuildCache = new Map<CacheKey, CacheEntry<SolvedBuildDto | null>>();
const upgradeSeriesCache = new Map<CacheKey, CacheEntry<UpgradePointDto[]>>();
const pathPreviewCache = new Map<CacheKey, CacheEntry<PathPreviewDto>>();
const affinityWatchCache = new Map<CacheKey, CacheEntry<AffinityWatchPayloadDto>>();
const weaponProfileCache = new Map<CacheKey, CacheEntry<WeaponProfileDto>>();
const weaponScalingCache = new Map<CacheKey, CacheEntry<ScalingDto>>();
let cacheDataVersion = "unknown";

export function setAnalysisCacheVersion(dataVersion: string): void {
  if (cacheDataVersion === dataVersion) return;
  cacheDataVersion = dataVersion;
  clearAnalysisCaches();
}

export function clearAnalysisCaches(): void {
  solveBuildCache.clear();
  upgradeSeriesCache.clear();
  pathPreviewCache.clear();
  affinityWatchCache.clear();
  weaponProfileCache.clear();
  weaponScalingCache.clear();
}

export function cachedWeaponProfile(weaponName: string, affinity: string | null): Promise<WeaponProfileDto> {
  return cached(weaponProfileCache, 128, { weaponName, affinity }, () => api.weaponProfile(weaponName, affinity));
}

export function cachedSolveBuild(
  base: OptimizeRequestDto,
  weaponName: string,
  affinity: string | null,
  aowName: string | null,
): Promise<SolvedBuildDto | null> {
  return cached(solveBuildCache, 128, { base, weaponName, affinity, aowName }, () =>
    api.solveBuild(base, weaponName, affinity, aowName));
}

export function cachedUpgradeSeries(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  maxUpgrade: number,
): Promise<UpgradePointDto[]> {
  return cached(upgradeSeriesCache, 64, { base, solved: rowFingerprint(solved), maxUpgrade }, () =>
    api.buildUpgradeSeries(base, solved, maxUpgrade));
}

export function cachedPathPreview(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  levelsAhead: number,
  title: string,
): Promise<PathPreviewDto> {
  return cached(pathPreviewCache, 16, { base, solved: rowFingerprint(solved), levelsAhead, title }, () =>
    api.buildPathPreview(base, solved, levelsAhead, title));
}

export function cachedAffinityWatch(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  levelsAhead: number,
): Promise<AffinityWatchPayloadDto> {
  return cached(affinityWatchCache, 12, { base, solved: rowFingerprint(solved), levelsAhead }, () =>
    api.buildAffinityWatch(base, solved, levelsAhead));
}

export function cachedWeaponScalingForUpgrade(
  weaponName: string,
  affinity: string,
  upgrade: number,
): Promise<ScalingDto> {
  return cached(weaponScalingCache, 256, { weaponName, affinity, upgrade }, () =>
    api.weaponScalingForUpgrade(weaponName, affinity, upgrade));
}

function cached<T>(
  cache: Map<CacheKey, CacheEntry<T>>,
  maxEntries: number,
  keyParts: unknown,
  loader: () => Promise<T>,
): Promise<T> {
  const key = stableSignature({ dataVersion: cacheDataVersion, keyParts });
  const now = Date.now();
  const entry = cache.get(key);
  if (entry && entry.expiresAt > now) {
    cache.delete(key);
    cache.set(key, entry);
    return entry.promise;
  }
  if (entry) cache.delete(key);
  const promise = loader().catch((error) => {
    cache.delete(key);
    throw error;
  });
  cache.set(key, { promise, expiresAt: now + CACHE_TTL_MS });
  while (cache.size > maxEntries) {
    const oldest = cache.keys().next().value as string | undefined;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
  return promise;
}
