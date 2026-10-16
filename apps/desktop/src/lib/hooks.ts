import { useCallback, useEffect, useMemo, useRef } from "react";
import { api } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import { buildOptimizeRequest } from "./session";
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
import { withoutHistory } from "./query-history";
import { useDesktopStore, type DesktopState } from "./state";

export function useBuiltRequest(
  catalog: CatalogDto | null,
  request: OptimizeRequestDto,
  lockedStatMode: boolean,
) {
  return useMemo(() => buildOptimizeRequest(catalog, request, lockedStatMode), [catalog, lockedStatMode, request]);
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
    if (Object.keys(patch).length > 0) withoutHistory(() => patchRequest(patch));
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

type NativeJobQueue<S extends { finished: FinishedJob | null }> = ReturnType<typeof createNativeJobQueue<S>>;

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

// A run is cancelled when the store invalidates it (Stop, input, selection or profile change),
// never by effect cleanup: a hidden workspace's effects stop, but its run keeps going.
function usePollingJob<P extends JobEvent, F extends FinishedJob, S extends JobStatus<P, F>>(options: {
  isCurrent: (state: DesktopState, generation: number) => boolean;
  queue: NativeJobQueue<S>;
  setProgress: (progress: P | null) => void;
  onStarted: (jobId: string, generation: number) => void;
}): (start: () => Promise<{ jobId: string }>, generation: number) => Promise<NativeJobSuccess<F>> {
  const latest = useRef(options);
  latest.current = options;
  const active = useRef<AbortController | null>(null);

  return useCallback((start: () => Promise<{ jobId: string }>, generation: number) => {
    active.current?.abort();
    const controller = new AbortController();
    active.current = controller;
    const unsubscribe = useDesktopStore.subscribe((state) => {
      if (!latest.current.isCurrent(state, generation)) controller.abort();
    });
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
      unsubscribe();
      if (active.current === controller) active.current = null;
    });
  }, []);
}

export function usePathJob(options: {
  setPathProgress: (progress: PathProgressDto | null) => void;
  onStarted: (jobId: string, generation: number) => void;
}) {
  return usePollingJob<PathProgressDto, PathFinishedDto, PathJobStatusDto>({
    isCurrent: (state, generation) => state.isPathBusy && state.pathGeneration === generation,
    queue: pathQueue,
    setProgress: options.setPathProgress,
    onStarted: options.onStarted,
  });
}

export function useAffinityJob(options: {
  setAffinityProgress: (progress: AffinityWatchProgressDto | null) => void;
  onStarted: (jobId: string, generation: number) => void;
}) {
  return usePollingJob<AffinityWatchProgressDto, AffinityWatchFinishedDto, AffinityWatchJobStatusDto>({
    isCurrent: (state, generation) => state.isAffinityBusy && state.affinityGeneration === generation,
    queue: affinityQueue,
    setProgress: options.setAffinityProgress,
    onStarted: options.onStarted,
  });
}
