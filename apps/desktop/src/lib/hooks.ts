import { useEffect, useMemo, useRef, useState } from "react";
import { api, hasTauriRuntime } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import { buildOptimizeRequest, budgetSnapshot } from "./session";
import { stableSignature } from "./session";
import { progressSignature, startAdaptivePolling } from "./polling";
import { LatestRequest } from "./request-generation";
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
  const profileRequest = useRef(new LatestRequest());

  useEffect(() => {
    const controller = new AbortController();
    const token = profileRequest.current.begin(stableSignature({
      weaponName: request.weaponName,
      affinity: request.affinity,
      aowName: request.aowName,
    }));
    async function loadWeaponProfile() {
      if (!request.weaponName) {
        setWeaponProfile(null);
        return;
      }
      const profile = await cachedWeaponProfile(
        request.weaponName,
        request.affinity,
        controller.signal,
      );
      if (!profileRequest.current.isCurrent(token)) return;
      setWeaponProfile(profile);

      const patch: Partial<OptimizeRequestDto> = {};
      if (request.affinity && !profile.affinities.includes(request.affinity)) {
        patch.affinity = profile.affinities[0] ?? null;
      }
      if (request.aowName && !profile.compatibleAows.includes(request.aowName)) {
        patch.aowName = null;
      }
      if (Object.keys(patch).length > 0) patchRequest(patch);
    }

    loadWeaponProfile().catch((error) => {
      if (profileRequest.current.isCurrent(token)) {
        setWeaponProfile(null);
        setError(error instanceof Error ? error.message : String(error));
      }
    });

    return () => {
      controller.abort();
      profileRequest.current.invalidate(token);
    };
  }, [patchRequest, request.affinity, request.aowName, request.weaponName, setError]);

  return weaponProfile;
}

export function useSearchJob(options: {
  activeJobId: string | null;
  isSearching: boolean;
  generation: number;
  setProgress: (progress: SearchProgressDto | null) => void;
  finish: (payload: SearchFinishedDto, generation: number) => void;
}) {
  const { activeJobId, isSearching, generation, setProgress, finish } = options;
  const setProgressRef = useRef(setProgress);
  const finishRef = useRef(finish);
  setProgressRef.current = setProgress;
  finishRef.current = finish;

  useEffect(() => {
    if (!activeJobId || !isSearching) return undefined;
    const jobId = activeJobId;
    const polling = startAdaptivePolling({
      poll: () => api.searchStatus(jobId),
      progressKey: (status) => progressSignature(status.progress),
      onStatus: (status) => {
        if (status.progress?.jobId === jobId) setProgressRef.current(status.progress);
        if (status.finished?.jobId === jobId) {
          finishRef.current(status.finished, generation);
          return true;
        }
        return false;
      },
      onMissing: () => {
        finishRef.current({
            jobId,
            cancelled: true,
            rows: [],
            error: "Search job disappeared before returning a result.",
        }, generation);
      },
      onError: (error) => {
        finishRef.current({
          jobId,
          cancelled: false,
          rows: [],
          error: error instanceof Error ? error.message : String(error),
        }, generation);
      },
    });
    return () => {
      const unfinished = !polling.isFinished();
      polling.stop();
      if (unfinished && hasTauriRuntime()) void api.cancelSearch(jobId).catch(() => undefined);
    };
  }, [activeJobId, generation, isSearching]);
}

export function usePathJob(options: {
  activePathJobId: string | null;
  isPathBusy: boolean;
  generation: number;
  setPathProgress: (progress: PathProgressDto | null) => void;
  finish: (payload: PathFinishedDto, generation: number) => void;
}) {
  const { activePathJobId, isPathBusy, generation, setPathProgress, finish } = options;
  const setProgressRef = useRef(setPathProgress);
  const finishRef = useRef(finish);
  setProgressRef.current = setPathProgress;
  finishRef.current = finish;

  useEffect(() => {
    if (!activePathJobId || !isPathBusy) return undefined;
    const jobId = activePathJobId;
    const polling = startAdaptivePolling({
      poll: () => api.pathPreviewStatus(jobId),
      progressKey: (status) => progressSignature(status.progress),
      onStatus: (status) => {
        if (status.progress?.jobId === jobId) setProgressRef.current(status.progress);
        if (status.finished?.jobId === jobId) {
          finishRef.current(status.finished, generation);
          return true;
        }
        return false;
      },
      onMissing: () => {
        finishRef.current({
            jobId,
            cancelled: true,
            paths: [],
            error: "Path job disappeared before returning a result.",
        }, generation);
      },
      onError: (error) => {
        finishRef.current({
          jobId,
          cancelled: false,
          paths: [],
          error: error instanceof Error ? error.message : String(error),
        }, generation);
      },
    });
    return () => {
      const unfinished = !polling.isFinished();
      polling.stop();
      if (unfinished && hasTauriRuntime()) {
        void api.cancelPathPreview(jobId).catch(() => undefined);
      }
    };
  }, [activePathJobId, generation, isPathBusy]);
}

export function useAffinityJob(options: {
  activeAffinityJobId: string | null;
  isAffinityBusy: boolean;
  generation: number;
  setAffinityProgress: (progress: AffinityWatchProgressDto | null) => void;
  finish: (payload: AffinityWatchFinishedDto, generation: number) => void;
}) {
  const { activeAffinityJobId, isAffinityBusy, generation, setAffinityProgress, finish } = options;
  const setProgressRef = useRef(setAffinityProgress);
  const finishRef = useRef(finish);
  setProgressRef.current = setAffinityProgress;
  finishRef.current = finish;

  useEffect(() => {
    if (!activeAffinityJobId || !isAffinityBusy) return undefined;
    const jobId = activeAffinityJobId;
    const polling = startAdaptivePolling({
      poll: () => api.affinityWatchStatus(jobId),
      progressKey: (status) => progressSignature(status.progress),
      onStatus: (status) => {
        if (status.progress?.jobId === jobId) setProgressRef.current(status.progress);
        if (status.finished?.jobId === jobId) {
          finishRef.current(status.finished, generation);
          return true;
        }
        return false;
      },
      onMissing: () => {
        finishRef.current({
            jobId,
            cancelled: true,
            payload: null,
            error: "Affinity watch job disappeared before returning a result.",
        }, generation);
      },
      onError: (error) => {
        finishRef.current({
          jobId,
          cancelled: false,
          payload: null,
          error: error instanceof Error ? error.message : String(error),
        }, generation);
      },
    });
    return () => {
      const unfinished = !polling.isFinished();
      polling.stop();
      if (unfinished && hasTauriRuntime()) {
        void api.cancelAffinityWatch(jobId).catch(() => undefined);
      }
    };
  }, [activeAffinityJobId, generation, isAffinityBusy]);
}

export function hasRuntimeTauriJob() {
  return hasTauriRuntime();
}
