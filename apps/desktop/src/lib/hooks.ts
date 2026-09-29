import { useCallback, useEffect, useMemo, useRef } from "react";
import { api } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import { buildOptimizeRequest, budgetSnapshot } from "./session";
import { progressSignature } from "./polling";
import { createNativeJobQueue, type FinishedJob, type NativeJobSuccess } from "./native-jobs";
import { useKeyedResource } from "./keyed-resource";
import {
  AffinityWatchFinishedDto,
  AffinityWatchJobStatusDto,
  AffinityWatchProgressDto,
  CatalogDto,
  OptimizeRequestDto,
  PathFinishedDto,
  PathJobStatusDto,
  PathProgressDto,
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
) {
  const resource = useWeaponProfileResource(request.profileId, request.weaponName, request.affinity);
  const profile = resource.profile;

  useEffect(() => {
    if (!profile) return;
    const patch: Partial<OptimizeRequestDto> = {};
    if (request.affinity && !profile.affinities.includes(request.affinity)) {
      patch.affinity = profile.affinities[0] ?? null;
    }
    if (request.aowName && !profile.compatibleAows.includes(request.aowName)) {
      patch.aowName = null;
    }
    if (Object.keys(patch).length > 0) patchRequest(patch);
  }, [patchRequest, profile, request.affinity, request.aowName]);

  return resource;
}

export function useWeaponProfileResource(profileId: string, weaponName: string | null, affinity: string | null) {
  const key = weaponName ? JSON.stringify([profileId, weaponName, affinity]) : null;
  const load = useCallback((signal: AbortSignal) =>
    cachedWeaponProfile(profileId, weaponName!, affinity, signal), [profileId, weaponName, affinity]);
  const resource = useKeyedResource(key, load);
  return { ...resource, profile: resource.data };
}

type JobEvent = { jobId: string };
type JobStatus<P extends JobEvent, F extends JobEvent> = { progress: P | null; finished: F | null };

type NativeJobQueue<S extends { finished: FinishedJob | null }> = (
  start: () => Promise<{ jobId: string }>,
  signal?: AbortSignal,
  onStatus?: (status: S) => void,
  onStarted?: (jobId: string) => void,
) => Promise<NativeJobSuccess<NonNullable<S["finished"]>>>;

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

function usePollingJob<P extends JobEvent, F extends FinishedJob, S extends JobStatus<P, F>>(options: {
  busy: boolean;
  generation: number;
  queue: NativeJobQueue<S>;
  setProgress: (progress: P | null) => void;
  onStarted: (jobId: string, generation: number) => void;
}): (start: () => Promise<{ jobId: string }>, generation: number) => Promise<NativeJobSuccess<F>> {
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
