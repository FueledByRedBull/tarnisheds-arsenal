// Native jobs publish progress about every 100 ms. Quick jobs are caught by the short
// first checks; long ones are noticed within MAX_POLL_DELAY_MS of finishing.
export const INITIAL_POLL_DELAY_MS = 8;
export const PROGRESS_POLL_DELAY_MS = 25;
export const MAX_POLL_DELAY_MS = 50;

export function nextPollDelay(currentDelay: number, progressChanged: boolean): number {
  if (progressChanged) return PROGRESS_POLL_DELAY_MS;
  return Math.min(MAX_POLL_DELAY_MS, Math.ceil(Math.max(currentDelay, INITIAL_POLL_DELAY_MS) * 1.5));
}

export function progressSignature(progress: unknown): string {
  return JSON.stringify(progress ?? null);
}
