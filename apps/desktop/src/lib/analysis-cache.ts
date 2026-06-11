import { api } from "./api";
import { rowFingerprint, stableSignature } from "./session";
import { AffinityWatchPayloadDto, OptimizeRequestDto, PathPreviewDto, ScalingDto, SolvedBuildDto, UpgradePointDto, WeaponProfileDto } from "./types";

type CacheKey = string;

const solveBuildCache = new Map<CacheKey, Promise<SolvedBuildDto | null>>();
const upgradeSeriesCache = new Map<CacheKey, Promise<UpgradePointDto[]>>();
const pathPreviewCache = new Map<CacheKey, Promise<PathPreviewDto>>();
const affinityWatchCache = new Map<CacheKey, Promise<AffinityWatchPayloadDto>>();
const weaponProfileCache = new Map<CacheKey, Promise<WeaponProfileDto>>();
const weaponScalingCache = new Map<CacheKey, Promise<ScalingDto>>();

export function cachedWeaponProfile(weaponName: string, affinity: string | null): Promise<WeaponProfileDto> {
  return cached(weaponProfileCache, { weaponName, affinity }, () => api.weaponProfile(weaponName, affinity));
}

export function cachedSolveBuild(
  base: OptimizeRequestDto,
  weaponName: string,
  affinity: string | null,
  aowName: string | null,
): Promise<SolvedBuildDto | null> {
  return cached(solveBuildCache, { base, weaponName, affinity, aowName }, () =>
    api.solveBuild(base, weaponName, affinity, aowName));
}

export function cachedUpgradeSeries(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  maxUpgrade: number,
): Promise<UpgradePointDto[]> {
  return cached(upgradeSeriesCache, { base, solved: rowFingerprint(solved), maxUpgrade }, () =>
    api.buildUpgradeSeries(base, solved, maxUpgrade));
}

export function cachedPathPreview(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  levelsAhead: number,
  title: string,
): Promise<PathPreviewDto> {
  return cached(pathPreviewCache, { base, solved: rowFingerprint(solved), levelsAhead, title }, () =>
    api.buildPathPreview(base, solved, levelsAhead, title));
}

export function cachedAffinityWatch(
  base: OptimizeRequestDto,
  solved: SolvedBuildDto,
  levelsAhead: number,
): Promise<AffinityWatchPayloadDto> {
  return cached(affinityWatchCache, { base, solved: rowFingerprint(solved), levelsAhead }, () =>
    api.buildAffinityWatch(base, solved, levelsAhead));
}

export function cachedWeaponScalingForUpgrade(
  weaponName: string,
  affinity: string,
  upgrade: number,
): Promise<ScalingDto> {
  return cached(weaponScalingCache, { weaponName, affinity, upgrade }, () =>
    api.weaponScalingForUpgrade(weaponName, affinity, upgrade));
}

function cached<T>(
  cache: Map<CacheKey, Promise<T>>,
  keyParts: unknown,
  loader: () => Promise<T>,
): Promise<T> {
  const key = stableSignature(keyParts);
  const cachedPromise = cache.get(key);
  if (cachedPromise) {
    return cachedPromise;
  }
  const promise = loader().catch((error) => {
    cache.delete(key);
    throw error;
  });
  cache.set(key, promise);
  return promise;
}
