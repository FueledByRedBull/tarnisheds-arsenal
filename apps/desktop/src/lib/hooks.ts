import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
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

function usePollingJob<P extends JobEvent, F extends JobEvent>(options: {
  activeJobId: string | null;
  busy: boolean;
  generation: number;
  poll: (jobId: string) => Promise<JobStatus<P, F> | null>;
  cancel: (jobId: string) => Promise<boolean>;
  setProgress: (progress: P | null) => void;
  finish: (payload: F, generation: number) => void;
  missing: (jobId: string) => F;
  failed: (jobId: string, error: unknown) => F;
}) {
  const { activeJobId, busy, generation } = options;
  const latest = useRef(options);
  latest.current = options;

  useEffect(() => {
    if (!activeJobId || !busy) return undefined;
    const jobId = activeJobId;
    const polling = startAdaptivePolling({
      poll: () => latest.current.poll(jobId),
      progressKey: (status) => progressSignature(status.progress),
      onStatus: (status) => {
        if (status.progress?.jobId === jobId) latest.current.setProgress(status.progress);
        const finished = status.finished;
        if (!finished || finished.jobId !== jobId) return false;
        latest.current.finish(finished, generation);
        return true;
      },
      onMissing: () => latest.current.finish(latest.current.missing(jobId), generation),
      onError: (error) => latest.current.finish(latest.current.failed(jobId, error), generation),
    });
    return () => {
      const unfinished = !polling.isFinished();
      polling.stop();
      if (unfinished) void latest.current.cancel(jobId).catch(() => undefined);
    };
  }, [activeJobId, busy, generation]);
}

export function useSearchJob(options: {
  activeJobId: string | null;
  isSearching: boolean;
  generation: number;
  setProgress: (progress: SearchProgressDto | null) => void;
  finish: (payload: SearchFinishedDto, generation: number) => void;
}) {
  usePollingJob({
    activeJobId: options.activeJobId,
    busy: options.isSearching,
    generation: options.generation,
    poll: api.searchStatus,
    cancel: api.cancelSearch,
    setProgress: options.setProgress,
    finish: options.finish,
    missing: (jobId) => ({ jobId, cancelled: true, rows: [], error: "Search job disappeared before returning a result." }),
    failed: (jobId, error) => ({ jobId, cancelled: false, rows: [], error: errorMessage(error) }),
  });
}

export function usePathJob(options: {
  activePathJobId: string | null;
  isPathBusy: boolean;
  generation: number;
  setPathProgress: (progress: PathProgressDto | null) => void;
  finish: (payload: PathFinishedDto, generation: number) => void;
}) {
  usePollingJob({
    activeJobId: options.activePathJobId,
    busy: options.isPathBusy,
    generation: options.generation,
    poll: api.pathPreviewStatus,
    cancel: api.cancelPathPreview,
    setProgress: options.setPathProgress,
    finish: options.finish,
    missing: (jobId) => ({ jobId, cancelled: true, paths: [], error: "Path job disappeared before returning a result." }),
    failed: (jobId, error) => ({ jobId, cancelled: false, paths: [], error: errorMessage(error) }),
  });
}

export function useAffinityJob(options: {
  activeAffinityJobId: string | null;
  isAffinityBusy: boolean;
  generation: number;
  setAffinityProgress: (progress: AffinityWatchProgressDto | null) => void;
  finish: (payload: AffinityWatchFinishedDto, generation: number) => void;
}) {
  usePollingJob({
    activeJobId: options.activeAffinityJobId,
    busy: options.isAffinityBusy,
    generation: options.generation,
    poll: api.affinityWatchStatus,
    cancel: api.cancelAffinityWatch,
    setProgress: options.setAffinityProgress,
    finish: options.finish,
    missing: (jobId) => ({ jobId, cancelled: true, payload: null, error: "Affinity watch job disappeared before returning a result." }),
    failed: (jobId, error) => ({ jobId, cancelled: false, payload: null, error: errorMessage(error) }),
  });
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
