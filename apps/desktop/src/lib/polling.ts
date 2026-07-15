export const INITIAL_POLL_DELAY_MS = 200;
export const MAX_POLL_DELAY_MS = 1_000;

export function nextPollDelay(currentDelay: number, progressChanged: boolean): number {
  if (progressChanged) return INITIAL_POLL_DELAY_MS;
  return Math.min(MAX_POLL_DELAY_MS, Math.ceil(Math.max(currentDelay, INITIAL_POLL_DELAY_MS) * 1.5));
}

export function progressSignature(progress: unknown): string {
  return JSON.stringify(progress ?? null);
}

export interface AdaptivePolling<T> {
  isFinished: () => boolean;
  stop: () => void;
}

export function startAdaptivePolling<T>(options: {
  poll: () => Promise<T | null>;
  progressKey: (status: T) => string;
  onStatus: (status: T) => boolean;
  onMissing: () => void;
  onError: (error: unknown) => void;
}): AdaptivePolling<T> {
  let disposed = false;
  let finished = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let delay = INITIAL_POLL_DELAY_MS;
  let lastProgress = "";

  function schedule() {
    if (!disposed && !finished) {
      timer = setTimeout(() => void pollOnce(), delay);
    }
  }

  async function pollOnce() {
    try {
      const status = await options.poll();
      if (disposed) return;
      if (!status) {
        finished = true;
        options.onMissing();
        return;
      }
      const nextProgress = options.progressKey(status);
      delay = nextPollDelay(delay, nextProgress !== lastProgress);
      lastProgress = nextProgress;
      finished = options.onStatus(status);
      schedule();
    } catch (error) {
      if (!disposed) {
        finished = true;
        options.onError(error);
      }
    }
  }

  void pollOnce();
  return {
    isFinished: () => finished,
    stop: () => {
      disposed = true;
      if (timer !== undefined) clearTimeout(timer);
    },
  };
}
