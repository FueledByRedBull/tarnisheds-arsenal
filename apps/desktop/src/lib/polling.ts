export const INITIAL_POLL_DELAY_MS = 200;
export const MAX_POLL_DELAY_MS = 1_000;

export function nextPollDelay(currentDelay: number, progressChanged: boolean): number {
  if (progressChanged) return INITIAL_POLL_DELAY_MS;
  return Math.min(MAX_POLL_DELAY_MS, Math.ceil(Math.max(currentDelay, INITIAL_POLL_DELAY_MS) * 1.5));
}

export function progressSignature(progress: unknown): string {
  return JSON.stringify(progress ?? null);
}
