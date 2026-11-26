import { cancelledError, type createNativeJobQueue, type FinishedJob, type NativeJobSuccess } from "./native-jobs";
import { useDesktopStore, type DesktopState } from "./state";

/** What the store holds for one job kind's current run. */
export interface RunSlotState {
  busy: boolean;
  generation: number;
  signature: string | null;
  jobId: string | null;
}

/** One run: the generation and input signature it began with. */
export interface RunToken {
  generation: number;
  signature: string;
}

type JobStatus = { finished: FinishedJob | null };
type NativeJobQueue<S extends JobStatus> = ReturnType<typeof createNativeJobQueue<S>>;

export interface RunSlot<S extends JobStatus> {
  /** Starts a run on these inputs, which ends any run before it. */
  begin: (signature: string) => RunToken;
  /** Whether the run still owns the slot. Stop and input, selection or profile changes end it. */
  owns: (state: DesktopState, token: RunToken) => boolean;
  /** Whether a reply from `jobId` may change state: the run owns the slot and `jobId` is its current job. */
  accepts: (state: DesktopState, token: RunToken, jobId: string) => boolean;
  /**
   * Runs the native job through the kind's queue. The queue cancels it when the run loses the
   * slot, and status reaches `onStatus` only while the slot accepts the job.
   */
  run: (
    token: RunToken,
    start: () => Promise<{ jobId: string }>,
    onStatus?: (status: S, jobId: string) => void,
  ) => Promise<NativeJobSuccess<NonNullable<S["finished"]>>>;
  /** Clears the slot's busy state, job ID and progress, if the run still owns it. */
  end: (token: RunToken) => void;
  /** Stops the current run through its queue, which owns native cancellation and its recovery. */
  cancel: () => void;
}

/**
 * One slot per job kind (Search, Paths, Affinity Watch). Its generation, signature and job ID
 * live in the store; the slot decides from them whether a reply is still current.
 */
export function createRunSlot<S extends JobStatus>(options: {
  queue: NativeJobQueue<S>;
  read: (state: DesktopState) => RunSlotState;
  begin: (signature: string) => number;
  setJobId: (jobId: string) => void;
  end: () => void;
  /** A run whose results would already be out of date cannot start or publish. */
  usable?: (state: DesktopState) => boolean;
}): RunSlot<S> {
  let active: AbortController | null = null;
  const owns = (state: DesktopState, token: RunToken) => {
    const current = options.read(state);
    return current.busy && current.generation === token.generation && current.signature === token.signature
      && (options.usable?.(state) ?? true);
  };
  const accepts = (state: DesktopState, token: RunToken, jobId: string) =>
    owns(state, token) && options.read(state).jobId === jobId;

  return {
    begin: (signature) => ({ generation: options.begin(signature), signature }),
    owns,
    accepts,
    run(token, start, onStatus) {
      active?.abort();
      const controller = new AbortController();
      active = controller;
      const unsubscribe = useDesktopStore.subscribe((state) => {
        if (!owns(state, token)) controller.abort();
      });
      if (!owns(useDesktopStore.getState(), token)) controller.abort();
      let jobId: string | null = null;
      return options.queue(start, controller.signal, (status) => {
        if (jobId !== null && accepts(useDesktopStore.getState(), token, jobId)) onStatus?.(status, jobId);
      }, (startedJobId) => {
        jobId = startedJobId;
        // A job that started after its run lost the slot is cancelled by the queue, never recorded.
        if (controller.signal.aborted || !owns(useDesktopStore.getState(), token)) throw cancelledError();
        options.setJobId(startedJobId);
      }).finally(() => {
        unsubscribe();
        if (active === controller) active = null;
      });
    },
    end(token) {
      if (owns(useDesktopStore.getState(), token)) options.end();
    },
    cancel: () => active?.abort(),
  };
}
