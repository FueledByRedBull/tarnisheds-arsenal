export type AnalysisStatus =
  | "ready"
  | "running"
  | "completed"
  | "cancelled"
  | "stale"
  | "failed";

export type AnalysisOutcome = "cancelled" | "failed" | null;

export function analysisStatus(input: {
  busy: boolean;
  resultSignature: string | null;
  requestSignature: string;
  hasResult: boolean;
  outcome: AnalysisOutcome;
}): AnalysisStatus {
  if (input.busy) return "running";
  if (input.hasResult && input.resultSignature !== input.requestSignature) return "stale";
  if (input.outcome) return input.outcome;
  if (input.resultSignature === input.requestSignature) return "completed";
  return "ready";
}

export function analysisStatusLabel(status: AnalysisStatus): string {
  switch (status) {
    case "running":
      return "Running";
    case "completed":
      return "Completed";
    case "cancelled":
      return "Cancelled";
    case "stale":
      return "Stale results";
    case "failed":
      return "Failed";
    default:
      return "Ready";
  }
}
