import { useEffect, useMemo, useState } from "react";
import { api, hasTauriRuntime } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import { buildOptimizeRequest, budgetSnapshot } from "./session";
import { useDesktopStore } from "./state";
import {
  AffinityWatchFinishedDto,
  AffinityWatchProgressDto,
  CatalogDto,
  OptimizeRequestDto,
  PathFinishedDto,
  PathProgressDto,
  SearchFinishedDto,
  SearchProgressDto,
  WeaponProfileDto,
} from "./types";

export function useRequestBudget(
  catalog: CatalogDto | null,
  request: OptimizeRequestDto,
  lockedStatMode: boolean,
) {
  return useMemo(() => {
    const base = buildOptimizeRequest(catalog, request, lockedStatMode);
    return {
      base,
      budget: budgetSnapshot(catalog, request),
    };
  }, [catalog, lockedStatMode, request]);
}

export function useWeaponProfile(
  request: OptimizeRequestDto,
  patchRequest: (patch: Partial<OptimizeRequestDto>) => void,
  setError: (message: string | null) => void,
) {
  const [weaponProfile, setWeaponProfile] = useState<WeaponProfileDto | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function loadWeaponProfile() {
      if (!request.weaponName) {
        setWeaponProfile(null);
        return;
      }
      const profile = await cachedWeaponProfile(request.weaponName, request.affinity);
      if (cancelled) return;
      setWeaponProfile(profile);

      const patch: Partial<OptimizeRequestDto> = {};
      if (request.maxUpgrade > profile.maxUpgrade) patch.maxUpgrade = profile.maxUpgrade;
      if (request.fixedUpgrade !== null && request.fixedUpgrade > profile.maxUpgrade) {
        patch.fixedUpgrade = profile.maxUpgrade;
      }
      if (request.affinity && !profile.affinities.includes(request.affinity)) {
        patch.affinity = profile.affinities[0] ?? null;
      }
      if (request.aowName && !profile.compatibleAows.includes(request.aowName)) {
        patch.aowName = null;
      }
      if (Object.keys(patch).length > 0) patchRequest(patch);
    }

    loadWeaponProfile().catch((error) => {
      if (!cancelled) {
        setWeaponProfile(null);
        setError(error instanceof Error ? error.message : String(error));
      }
    });

    return () => {
      cancelled = true;
    };
  }, [patchRequest, request.affinity, request.aowName, request.fixedUpgrade, request.maxUpgrade, request.weaponName, setError]);

  return weaponProfile;
}

export function useSearchJob(options: {
  activeJobId: string | null;
  isSearching: boolean;
  setProgress: (progress: SearchProgressDto | null) => void;
  finish: (payload: SearchFinishedDto) => void;
}) {
  const { activeJobId, isSearching, setProgress, finish } = options;

  useEffect(() => {
    if (!activeJobId || !isSearching) return undefined;
    let disposed = false;

    async function pollSearchStatus() {
      try {
        const currentJobId = useDesktopStore.getState().activeJobId;
        if (!currentJobId) return;
        const status = await api.searchStatus(currentJobId);
        if (disposed) return;
        if (!status) {
          finish({
            jobId: currentJobId,
            cancelled: true,
            rows: [],
            error: "Search job disappeared before returning a result.",
          });
          return;
        }
        if (status.progress) setProgress(status.progress);
        if (status.finished) finish(status.finished);
      } catch (error) {
        if (!disposed) {
          finish({
            jobId: useDesktopStore.getState().activeJobId ?? "search",
            cancelled: false,
            rows: [],
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }

    void pollSearchStatus();
    const interval = window.setInterval(pollSearchStatus, 200);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [activeJobId, finish, isSearching, setProgress]);
}

export function usePathJob(options: {
  activePathJobId: string | null;
  isPathBusy: boolean;
  setPathProgress: (progress: PathProgressDto | null) => void;
  finish: (payload: PathFinishedDto) => void;
}) {
  const { activePathJobId, isPathBusy, setPathProgress, finish } = options;

  useEffect(() => {
    if (!activePathJobId || !isPathBusy) return undefined;
    let disposed = false;

    async function pollPathStatus() {
      try {
        const currentJobId = useDesktopStore.getState().activePathJobId;
        if (!currentJobId) return;
        const status = await api.pathPreviewStatus(currentJobId);
        if (disposed) return;
        if (!status) {
          finish({
            jobId: currentJobId,
            cancelled: true,
            paths: [],
            error: "Path job disappeared before returning a result.",
          });
          return;
        }
        if (status.progress) setPathProgress(status.progress);
        if (status.finished) finish(status.finished);
      } catch (error) {
        if (!disposed) {
          finish({
            jobId: useDesktopStore.getState().activePathJobId ?? "path",
            cancelled: false,
            paths: [],
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }

    void pollPathStatus();
    const interval = window.setInterval(pollPathStatus, 200);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [activePathJobId, finish, isPathBusy, setPathProgress]);
}

export function useAffinityJob(options: {
  activeAffinityJobId: string | null;
  isAffinityBusy: boolean;
  setAffinityProgress: (progress: AffinityWatchProgressDto | null) => void;
  finish: (payload: AffinityWatchFinishedDto) => void;
}) {
  const { activeAffinityJobId, isAffinityBusy, setAffinityProgress, finish } = options;

  useEffect(() => {
    if (!activeAffinityJobId || !isAffinityBusy) return undefined;
    let disposed = false;

    async function pollAffinityStatus() {
      try {
        const currentJobId = useDesktopStore.getState().activeAffinityJobId;
        if (!currentJobId) return;
        const status = await api.affinityWatchStatus(currentJobId);
        if (disposed) return;
        if (!status) {
          finish({
            jobId: currentJobId,
            cancelled: true,
            payload: null,
            error: "Affinity watch job disappeared before returning a result.",
          });
          return;
        }
        if (status.progress) setAffinityProgress(status.progress);
        if (status.finished) finish(status.finished);
      } catch (error) {
        if (!disposed) {
          finish({
            jobId: useDesktopStore.getState().activeAffinityJobId ?? "affinity",
            cancelled: false,
            payload: null,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }

    void pollAffinityStatus();
    const interval = window.setInterval(pollAffinityStatus, 200);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [activeAffinityJobId, finish, isAffinityBusy, setAffinityProgress]);
}

export function hasRuntimeTauriJob() {
  return hasTauriRuntime();
}
