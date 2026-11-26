import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import type { AnalysisOutcome } from "./analysis-status";
import { errorMessage } from "./format";
import { buildOptimizeRequest } from "./session";
import { progressSignature } from "./polling";
import { createNativeJobQueue, isCancelled, type FinishedJob, type NativeJobSuccess } from "./native-jobs";
import { createRunSlot, type RunSlot } from "./run-slot";
import { useKeyedResource } from "./keyed-resource";
import { catalogVersion } from "./preset-activation";
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
  // A new catalog clears the cache and stops its pending loads, so it must also start a new one.
  const dataVersion = useDesktopStore((state) => state.catalog && catalogVersion(state.catalog));
  const key = weaponName ? JSON.stringify([dataVersion, profileId, weaponName, affinity]) : null;
  const load = useCallback((signal: AbortSignal) =>
    cachedWeaponProfile(profileId, weaponName!, affinity, signal), [profileId, weaponName, affinity]);
  const resource = useKeyedResource(key, load);
  return { ...resource, profile: resource.data };
}

type JobStatus<P extends { jobId: string }, F extends FinishedJob> = { progress: P | null; finished: F | null };

interface AnalysisRun<P extends { jobId: string }, F extends FinishedJob, S extends JobStatus<P, F>> {
  slot: RunSlot<S>;
  scope: "paths" | "affinity_watch";
  stoppedMessage: string;
  setProgress: (progress: P | null) => void;
}

// Selected rows that became stale cannot start or publish an analysis.
const currentResults = (state: DesktopState) => !state.resultsStale;
const OUT_OF_DATE_MESSAGE = "The selected build belongs to the previous query. Go to Rankings, then press Run updated search.";

export const PATH_RUN: AnalysisRun<PathProgressDto, PathFinishedDto, PathJobStatusDto> = {
  slot: createRunSlot<PathJobStatusDto>({
    queue: createNativeJobQueue<PathJobStatusDto>(
      jobId => api.pathPreviewStatus(jobId),
      jobId => api.cancelPathPreview(jobId),
      status => progressSignature(status.progress),
    ),
    read: (state) => ({ busy: state.isPathBusy, generation: state.pathGeneration, signature: state.activePathSignature, jobId: state.activePathJobId }),
    begin: (signature) => useDesktopStore.getState().beginPath(signature),
    setJobId: (jobId) => useDesktopStore.getState().setActivePathJobId(jobId),
    end: () => useDesktopStore.setState({ isPathBusy: false, activePathJobId: null, pathProgress: null }),
    usable: currentResults,
  }),
  scope: "paths",
  stoppedMessage: "Path preview stopped.",
  setProgress: (progress) => useDesktopStore.getState().setPathProgress(progress),
};

export const AFFINITY_RUN: AnalysisRun<AffinityWatchProgressDto, AffinityWatchFinishedDto, AffinityWatchJobStatusDto> = {
  slot: createRunSlot<AffinityWatchJobStatusDto>({
    queue: createNativeJobQueue<AffinityWatchJobStatusDto>(
      jobId => api.affinityWatchStatus(jobId),
      jobId => api.cancelAffinityWatch(jobId),
      status => progressSignature(status.progress),
    ),
    read: (state) => ({ busy: state.isAffinityBusy, generation: state.affinityGeneration, signature: state.activeAffinitySignature, jobId: state.activeAffinityJobId }),
    begin: (signature) => useDesktopStore.getState().beginAffinity(signature),
    setJobId: (jobId) => useDesktopStore.getState().setActiveAffinityJobId(jobId),
    end: () => useDesktopStore.setState({ isAffinityBusy: false, activeAffinityJobId: null, affinityProgress: null }),
    usable: currentResults,
  }),
  scope: "affinity_watch",
  stoppedMessage: "Affinity watch stopped.",
  setProgress: (progress) => useDesktopStore.getState().setAffinityProgress(progress),
};

// Owns one Paths or Affinity Watch run through its kind's run slot. A run is cancelled when the
// store invalidates it (Stop, input, selection or profile change), never by effect cleanup: a
// hidden workspace's effects stop, but its run keeps going.
export function useAnalysisRun<P extends { jobId: string }, F extends FinishedJob, S extends JobStatus<P, F>>(
  run: AnalysisRun<P, F, S>,
  signature: string,
) {
  // A stopped or failed run is reported only while its inputs are still the current ones.
  const [reported, setReported] = useState<{ signature: string; outcome: AnalysisOutcome }>({ signature, outcome: null });
  const outcome = reported.signature === signature ? reported.outcome : null;
  const setOutcome = (next: AnalysisOutcome) => setReported({ signature, outcome: next });

  const start = (
    launch: () => Promise<{ jobId: string }>,
    publish: (finished: NativeJobSuccess<NonNullable<S["finished"]>>) => void,
    notice?: Notice,
  ) => runAnalysis(run, signature, launch, publish, setOutcome, notice);

  // The slot's queue sends the native cancellation; the run then ends as stopped, or as failed
  // when the cancellation itself fails.
  function stop() {
    setOutcome("cancelled");
    run.slot.cancel();
  }

  return { outcome, start, stop };
}

/**
 * One Paths or Affinity Watch run, start to end. A run on out-of-date results is refused before
 * it takes the slot, and a run always ends its own busy state, whether it published, stopped,
 * failed or lost its job; only a later run or an invalidation that took the slot keeps it busy.
 */
export async function runAnalysis<P extends { jobId: string }, F extends FinishedJob, S extends JobStatus<P, F>>(
  run: AnalysisRun<P, F, S>,
  signature: string,
  launch: () => Promise<{ jobId: string }>,
  publish: (finished: NativeJobSuccess<NonNullable<S["finished"]>>) => void,
  report: (outcome: AnalysisOutcome) => void,
  notice?: Notice,
) {
  report(null);
  if (!run.slot.ready(useDesktopStore.getState())) {
    useDesktopStore.getState().pushNotice({ scope: run.scope, tone: "warning", message: OUT_OF_DATE_MESSAGE });
    return;
  }
  const token = run.slot.begin(signature);
  if (notice) useDesktopStore.getState().pushNotice(notice);
  try {
    const finished = await run.slot.run(token, launch, (status, jobId) => {
      if (status.progress?.jobId === jobId) run.setProgress(status.progress);
    });
    if (!run.slot.accepts(useDesktopStore.getState(), token, finished.jobId)) return;
    publish(finished);
    report(null);
  } catch (error) {
    const state = useDesktopStore.getState();
    if (!run.slot.owns(state, token)) return;
    if (isCancelled(error)) {
      state.pushNotice({ scope: run.scope, tone: "warning", message: run.stoppedMessage });
      report("cancelled");
    } else {
      state.setError(errorMessage(error));
      report("failed");
    }
  } finally {
    run.slot.end(token);
  }
}
