import { useEffect, useMemo, useRef, useState } from "react";
import { api, hasTauriRuntime } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import { buildOptimizeRequest, budgetSnapshot } from "./session";
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
    let disposed = false;
    let finished = false;
    let timer: number | undefined;

    function schedule() {
      if (!disposed && !finished) {
        timer = window.setTimeout(() => void pollSearchStatus(), 200);
      }
    }

    async function pollSearchStatus() {
      try {
        const status = await api.searchStatus(jobId);
        if (disposed) return;
        if (!status) {
          finished = true;
          finishRef.current({
            jobId,
            cancelled: true,
            rows: [],
            error: "Search job disappeared before returning a result.",
          }, generation);
          return;
        }
        if (status.progress?.jobId === jobId) setProgressRef.current(status.progress);
        if (status.finished?.jobId === jobId) {
          finished = true;
          finishRef.current(status.finished, generation);
          return;
        }
        schedule();
      } catch (error) {
        if (!disposed) {
          finished = true;
          finishRef.current({
            jobId,
            cancelled: false,
            rows: [],
            error: error instanceof Error ? error.message : String(error),
          }, generation);
        }
      }
    }

    void pollSearchStatus();
    return () => {
      disposed = true;
      if (timer !== undefined) window.clearTimeout(timer);
      if (!finished && hasTauriRuntime()) void api.cancelSearch(jobId).catch(() => undefined);
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
    let disposed = false;
    let finished = false;
    let timer: number | undefined;

    function schedule() {
      if (!disposed && !finished) {
        timer = window.setTimeout(() => void pollPathStatus(), 200);
      }
    }

    async function pollPathStatus() {
      try {
        const status = await api.pathPreviewStatus(jobId);
        if (disposed) return;
        if (!status) {
          finished = true;
          finishRef.current({
            jobId,
            cancelled: true,
            paths: [],
            error: "Path job disappeared before returning a result.",
          }, generation);
          return;
        }
        if (status.progress?.jobId === jobId) setProgressRef.current(status.progress);
        if (status.finished?.jobId === jobId) {
          finished = true;
          finishRef.current(status.finished, generation);
          return;
        }
        schedule();
      } catch (error) {
        if (!disposed) {
          finished = true;
          finishRef.current({
            jobId,
            cancelled: false,
            paths: [],
            error: error instanceof Error ? error.message : String(error),
          }, generation);
        }
      }
    }

    void pollPathStatus();
    return () => {
      disposed = true;
      if (timer !== undefined) window.clearTimeout(timer);
      if (!finished && hasTauriRuntime()) {
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
    let disposed = false;
    let finished = false;
    let timer: number | undefined;

    function schedule() {
      if (!disposed && !finished) {
        timer = window.setTimeout(() => void pollAffinityStatus(), 200);
      }
    }

    async function pollAffinityStatus() {
      try {
        const status = await api.affinityWatchStatus(jobId);
        if (disposed) return;
        if (!status) {
          finished = true;
          finishRef.current({
            jobId,
            cancelled: true,
            payload: null,
            error: "Affinity watch job disappeared before returning a result.",
          }, generation);
          return;
        }
        if (status.progress?.jobId === jobId) {
          setProgressRef.current(status.progress);
        }
        if (status.finished?.jobId === jobId) {
          finished = true;
          finishRef.current(status.finished, generation);
          return;
        }
        schedule();
      } catch (error) {
        if (!disposed) {
          finished = true;
          finishRef.current({
            jobId,
            cancelled: false,
            payload: null,
            error: error instanceof Error ? error.message : String(error),
          }, generation);
        }
      }
    }

    void pollAffinityStatus();
    return () => {
      disposed = true;
      if (timer !== undefined) window.clearTimeout(timer);
      if (!finished && hasTauriRuntime()) {
        void api.cancelAffinityWatch(jobId).catch(() => undefined);
      }
    };
  }, [activeAffinityJobId, generation, isAffinityBusy]);
}

export function hasRuntimeTauriJob() {
  return hasTauriRuntime();
}
