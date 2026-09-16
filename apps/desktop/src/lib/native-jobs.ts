import { INITIAL_POLL_DELAY_MS, nextPollDelay } from "./polling";

type FinishedJob = { jobId: string; cancelled: boolean; error: string | null };

// Both queues retain native ownership independently of the caller's promise.
export function createNativeJobQueue<S extends { finished: FinishedJob | null }>(
  status: (jobId: string) => Promise<S | null>,
  cancel: (jobId: string) => Promise<boolean>,
  progressKey: (status: S) => string = () => "",
) {
  let tail: Promise<void> = Promise.resolve();
  let uncertainJobId: string | null = null;
  let pendingStatus: Promise<S | null> | null = null;
  const stopped = () => new DOMException("Calculation stopped.", "AbortError");
  const unknown = () => new Error("Worker state is unknown. Try again to reconnect, or restart the app before calculating.");

  async function watch(
    jobId: string,
    reject: (error: unknown) => void,
    signal?: AbortSignal,
    onStatus?: (status: S) => void,
    recovering = false,
  ): Promise<S["finished"] | undefined> {
    const timedOut = Symbol("worker state unknown");
    let expire!: (value: typeof timedOut) => void;
    const expired = new Promise<typeof timedOut>(resolve => { expire = resolve; });
    let recoveryTimer: ReturnType<typeof setTimeout> | undefined;
    let deadline = Infinity;
    let cancellationSent = false;
    let delay = INITIAL_POLL_DELAY_MS;
    let lastProgress: string | undefined;
    const requestCancellation = () => {
      if (recoveryTimer === undefined) {
        deadline = Date.now() + 3_000;
        recoveryTimer = setTimeout(() => expire(timedOut), 3_000);
      }
      if (cancellationSent) return;
      cancellationSent = true;
      void cancel(jobId).catch(reject);
    };
    signal?.addEventListener("abort", requestCancellation, { once: true });
    if (recovering || signal?.aborted) requestCancellation();
    try {
      while (Date.now() < deadline) {
        try {
          pendingStatus ??= status(jobId);
          const current = await Promise.race([pendingStatus, expired]);
          if (current === timedOut) return undefined;
          pendingStatus = null;
          // The registry removes only completed jobs; absence establishes no owner.
          if (!current) return null;
          if (current.finished) {
            if (current.finished.jobId !== jobId) throw new Error("Native status returned a different job.");
            return current.finished;
          }
          if (!cancellationSent) onStatus?.(current);
          const progress = progressKey(current);
          delay = nextPollDelay(delay, progress !== lastProgress);
          lastProgress = progress;
        } catch (error) {
          pendingStatus = null;
          reject(error);
          requestCancellation();
        }
        await new Promise(resolve => setTimeout(resolve, Math.min(delay, Math.max(0, deadline - Date.now()))));
      }
      return undefined;
    } finally {
      if (recoveryTimer !== undefined) clearTimeout(recoveryTimer);
      signal?.removeEventListener("abort", requestCancellation);
    }
  }

  return (
    start: () => Promise<{ jobId: string }>,
    signal?: AbortSignal,
    onStatus?: (status: S) => void,
    onStarted?: (jobId: string) => void,
  ): Promise<NonNullable<S["finished"]>> => new Promise((resolve, reject) => {
    tail = tail.then(async () => {
      if (signal?.aborted) throw stopped();
      if (uncertainJobId) {
        const recovered = await watch(uncertainJobId, () => {}, undefined, undefined, true);
        if (recovered === undefined) throw unknown();
        uncertainJobId = null;
      }
      if (signal?.aborted) throw stopped();
      const { jobId } = await start();
      uncertainJobId = jobId;
      try {
        onStarted?.(jobId);
      } catch (error) {
        reject(error);
        const recovered = await watch(jobId, () => {}, undefined, undefined, true);
        if (recovered !== undefined) uncertainJobId = null;
        return;
      }
      const finished = await watch(jobId, reject, signal, onStatus);
      if (finished === undefined) throw unknown();
      uncertainJobId = null;
      if (signal?.aborted || finished?.cancelled) throw stopped();
      if (!finished) throw new Error("Native job disappeared before returning a result.");
      if (finished.error) throw new Error(finished.error);
      resolve(finished as NonNullable<S["finished"]>);
    }).catch(reject);
  });
}
