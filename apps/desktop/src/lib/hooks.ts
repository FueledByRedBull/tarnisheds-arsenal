import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import { buildOptimizeRequest, budgetSnapshot } from "./session";
import { stableSignature } from "./session";
import { progressSignature } from "./polling";
import { createNativeJobQueue } from "./native-jobs";
import { LatestRequest } from "./request-generation";
import {
  AffinityWatchFinishedDto,
  AffinityWatchJobStatusDto,
  AffinityWatchProgressDto,
  CatalogDto,
  OptimizeRequestDto,
  PathFinishedDto,
  PathJobStatusDto,
  PathProgressDto,
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
      profileId: request.profileId,
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
        request.profileId,
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
  }, [patchRequest, request.affinity, request.aowName, request.profileId, request.weaponName, setError]);

  return weaponProfile;
}

type JobEvent = { jobId: string };
type JobStatus<P extends JobEvent, F extends JobEvent> = { progress: P | null; finished: F | null };

type NativeJobQueue<S extends { finished: JobEvent | null }> = (
  start: () => Promise<{ jobId: string }>,
  signal?: AbortSignal,
  onStatus?: (status: S) => void,
  onStarted?: (jobId: string) => void,
) => Promise<NonNullable<S["finished"]>>;

const pathQueue = createNativeJobQueue<PathJobStatusDto>(
  jobId => api.pathPreviewStatus(jobId),
  jobId => api.cancelPathPreview(jobId),
  status => progressSignature(status.progress),
);

const affinityQueue = createNativeJobQueue<AffinityWatchJobStatusDto>(
  jobId => api.affinityWatchStatus(jobId),
  jobId => api.cancelAffinityWatch(jobId),
  status => progressSignature(status.progress),
);

function usePollingJob<P extends JobEvent, F extends JobEvent, S extends JobStatus<P, F>>(options: {
  busy: boolean;
  generation: number;
  queue: NativeJobQueue<S>;
  setProgress: (progress: P | null) => void;
  onStarted: (jobId: string, generation: number) => void;
}): (start: () => Promise<{ jobId: string }>, generation: number) => Promise<F> {
  const latest = useRef(options);
  latest.current = options;
  const active = useRef<{ controller: AbortController; generation: number } | null>(null);

  useEffect(() => {
    const effectGeneration = options.generation;
    return () => {
      if (active.current?.generation === effectGeneration) active.current.controller.abort();
    };
  }, [options.busy, options.generation]);

  return useCallback((start: () => Promise<{ jobId: string }>, generation: number) => {
    active.current?.controller.abort();
    const controller = new AbortController();
    active.current = { controller, generation };
    let jobId: string | null = null;
    const result = latest.current.queue(
      start,
      controller.signal,
      status => {
        if (status.progress?.jobId === jobId) latest.current.setProgress(status.progress);
      },
      startedJobId => {
        jobId = startedJobId;
        if (controller.signal.aborted) throw new DOMException("Calculation stopped.", "AbortError");
        latest.current.onStarted(startedJobId, generation);
      },
    );
    return result.finally(() => {
      if (active.current?.controller === controller) active.current = null;
    });
  }, []);
}

export function usePathJob(options: {
  isPathBusy: boolean;
  generation: number;
  setPathProgress: (progress: PathProgressDto | null) => void;
  onStarted: (jobId: string, generation: number) => void;
}) {
  return usePollingJob<PathProgressDto, PathFinishedDto, PathJobStatusDto>({
    busy: options.isPathBusy,
    generation: options.generation,
    queue: pathQueue,
    setProgress: options.setPathProgress,
    onStarted: options.onStarted,
  });
}

export function useAffinityJob(options: {
  isAffinityBusy: boolean;
  generation: number;
  setAffinityProgress: (progress: AffinityWatchProgressDto | null) => void;
  onStarted: (jobId: string, generation: number) => void;
}) {
  return usePollingJob<AffinityWatchProgressDto, AffinityWatchFinishedDto, AffinityWatchJobStatusDto>({
    busy: options.isAffinityBusy,
    generation: options.generation,
    queue: affinityQueue,
    setProgress: options.setAffinityProgress,
    onStarted: options.onStarted,
  });
}
