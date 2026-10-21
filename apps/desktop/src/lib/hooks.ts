import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import type { AnalysisOutcome } from "./analysis-status";
import { errorMessage } from "./format";
import { buildOptimizeRequest } from "./session";
import { progressSignature } from "./polling";
import { cancelledError, createNativeJobQueue, isCancelled, type FinishedJob, type NativeJobSuccess } from "./native-jobs";
import { useKeyedResource } from "./keyed-resource";
import {
  AffinityWatchFinishedDto,
  AffinityWatchJobStatusDto,
  AffinityWatchProgressDto,
  CatalogDto,
  Notice,
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
  useLayoutEffect(() => { latest.current = options; });
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
        if (controller.signal.aborted) throw cancelledError();
        latest.current.onStarted(startedJobId, generation);
      },
    );
    return result.finally(() => {
      unsubscribe();
      if (active.current === controller) active.current = null;
    });
  }, []);
}

interface AnalysisRun<P extends JobEvent, F extends FinishedJob, S extends JobStatus<P, F>> {
  queue: NativeJobQueue<S>;
  cancel: (jobId: string) => Promise<boolean>;
  scope: "paths" | "affinity_watch";
  stoppedMessage: string;
  read: (state: DesktopState) => { busy: boolean; generation: number; signature: string | null; jobId: string | null };
  begin: (signature: string) => number;
  setJobId: (jobId: string | null) => void;
  setBusy: (busy: boolean) => void;
  setProgress: (progress: P | null) => void;
}

export const PATH_RUN: AnalysisRun<PathProgressDto, PathFinishedDto, PathJobStatusDto> = {
  queue: pathQueue,
  cancel: (jobId) => api.cancelPathPreview(jobId),
  scope: "paths",
  stoppedMessage: "Path preview stopped.",
  read: (state) => ({ busy: state.isPathBusy, generation: state.pathGeneration, signature: state.activePathSignature, jobId: state.activePathJobId }),
  begin: (signature) => useDesktopStore.getState().beginPath(signature),
  setJobId: (jobId) => useDesktopStore.getState().setActivePathJobId(jobId),
  setBusy: (busy) => useDesktopStore.getState().setPathBusy(busy),
  setProgress: (progress) => useDesktopStore.getState().setPathProgress(progress),
};

export const AFFINITY_RUN: AnalysisRun<AffinityWatchProgressDto, AffinityWatchFinishedDto, AffinityWatchJobStatusDto> = {
  queue: affinityQueue,
  cancel: (jobId) => api.cancelAffinityWatch(jobId),
  scope: "affinity_watch",
  stoppedMessage: "Affinity watch stopped.",
  read: (state) => ({ busy: state.isAffinityBusy, generation: state.affinityGeneration, signature: state.activeAffinitySignature, jobId: state.activeAffinityJobId }),
  begin: (signature) => useDesktopStore.getState().beginAffinity(signature),
  setJobId: (jobId) => useDesktopStore.getState().setActiveAffinityJobId(jobId),
  setBusy: (busy) => useDesktopStore.getState().setAffinityBusy(busy),
  setProgress: (progress) => useDesktopStore.getState().setAffinityProgress(progress),
};

// Owns one Paths or Affinity Watch run. A reply, error or cancellation may change state only
// while the run's generation, input signature and native job ID all still match.
export function useAnalysisRun<P extends JobEvent, F extends FinishedJob, S extends JobStatus<P, F>>(
  run: AnalysisRun<P, F, S>,
  signature: string,
) {
  // A stopped or failed run is reported only while its inputs are still the current ones.
  const [reported, setReported] = useState<{ signature: string; outcome: AnalysisOutcome }>({ signature, outcome: null });
  const outcome = reported.signature === signature ? reported.outcome : null;
  const setOutcome = (next: AnalysisOutcome) => setReported({ signature, outcome: next });
  const owns = (state: DesktopState, generation: number) => {
    const current = run.read(state);
    return current.busy && current.generation === generation && current.signature === signature;
  };
  const idle = () => {
    run.setBusy(false);
    run.setJobId(null);
    run.setProgress(null);
  };

  const startJob = usePollingJob<P, F, S>({
    isCurrent: (state, generation) => {
      const current = run.read(state);
      return current.busy && current.generation === generation;
    },
    queue: run.queue,
    setProgress: run.setProgress,
    onStarted: (jobId, generation) => {
      const state = useDesktopStore.getState();
      if (state.resultsStale || !owns(state, generation)) throw cancelledError();
      run.setJobId(jobId);
    },
  });

  async function start(launch: () => Promise<{ jobId: string }>, publish: (finished: NativeJobSuccess<F>) => void, notice?: Notice) {
    setOutcome(null);
    const generation = run.begin(signature);
    if (notice) useDesktopStore.getState().pushNotice(notice);
    try {
      const finished = await startJob(launch, generation);
      const state = useDesktopStore.getState();
      const current = run.read(state);
      if (state.resultsStale || current.generation !== generation || current.signature !== signature || current.jobId !== finished.jobId) return;
      publish(finished);
      setOutcome(null);
      idle();
    } catch (error) {
      const state = useDesktopStore.getState();
      if (!owns(state, generation)) return;
      if (isCancelled(error)) {
        state.pushNotice({ scope: run.scope, tone: "warning", message: run.stoppedMessage });
        setOutcome("cancelled");
      } else {
        state.setError(errorMessage(error));
        setOutcome("failed");
      }
      idle();
    }
  }

  async function stop() {
    setOutcome("cancelled");
    const jobId = run.read(useDesktopStore.getState()).jobId;
    if (!jobId) {
      run.setBusy(false);
      return;
    }
    try {
      await run.cancel(jobId);
    } catch (error) {
      const state = useDesktopStore.getState();
      const current = run.read(state);
      if (current.busy && current.jobId === jobId) {
        state.setError(errorMessage(error));
        setOutcome("failed");
      }
    }
  }

  return { outcome, start, stop };
}
