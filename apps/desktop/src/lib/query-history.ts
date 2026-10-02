import { create } from "zustand";
import { objectiveLabel } from "./format";
import { stableSignature } from "./session";
import { useDesktopStore } from "./state";
import { ObjectiveId, OptimizeRequestDto } from "./types";

// Undo and redo for query edits. Each step records the query before and after one edit.
// Changes in the same moment (a palette lock and its mode) or repeated edits of the same
// fields within MERGE_MS collapse into one step; different edits stay separate.
// Automatic follow-ups such as weapon-metadata normalisation run through `withoutHistory`
// so undo never lands on a state that immediately rewrites itself.

export interface QuerySnapshot {
  request: OptimizeRequestDto;
  lockedStatMode: boolean;
}

export interface QueryStep {
  before: QuerySnapshot;
  after: QuerySnapshot;
  at: number;
}

const LIMIT = 50;
const MERGE_MS = 500;
const SAME_ACTION_MS = 50;

export const useQueryHistory = create<{ past: QueryStep[]; future: QueryStep[]; announcement: string }>(() => ({
  past: [],
  future: [],
  announcement: "",
}));

let suppressed = 0;

export function withoutHistory<T>(run: () => T): T {
  suppressed += 1;
  try {
    return run();
  } finally {
    suppressed -= 1;
  }
}

const snapshot = (state: QuerySnapshot): QuerySnapshot => ({ request: state.request, lockedStatMode: state.lockedStatMode });
const same = (a: QuerySnapshot, b: QuerySnapshot) =>
  a.lockedStatMode === b.lockedStatMode && stableSignature(a.request) === stableSignature(b.request);

// Records edits from the desktop store until the returned function is called.
export function trackQueryHistory(now: () => number = Date.now): () => void {
  return useDesktopStore.subscribe((state, previous) => {
    if (state.request === previous.request && state.lockedStatMode === previous.lockedStatMode && state.catalog === previous.catalog) return;
    // A new catalog or profile changes what a query means, so its history no longer applies.
    if (state.catalog !== previous.catalog || state.request.profileId !== previous.request.profileId) {
      useQueryHistory.setState({ past: [], future: [] });
      return;
    }
    const after = snapshot(state);
    const { past, future } = useQueryHistory.getState();
    const last = past.at(-1);
    if (suppressed) {
      // Keep redo and merging aligned with the normalised query the player now sees.
      if (last && !future.length) useQueryHistory.setState({ past: [...past.slice(0, -1), { ...last, after }] });
      return;
    }
    const before = snapshot(previous);
    if (same(before, after)) return;
    const time = now();
    const elapsed = last ? time - last.at : Infinity;
    if (last && !future.length && (elapsed < SAME_ACTION_MS
      || (elapsed < MERGE_MS && changedKeys(last.before, last.after).join() === changedKeys(before, after).join()))) {
      const merged = { ...last, after, at: time };
      useQueryHistory.setState({ past: same(merged.before, after) ? past.slice(0, -1) : [...past.slice(0, -1), merged] });
      return;
    }
    useQueryHistory.setState({ past: [...past, { before, after, at: time }].slice(-LIMIT), future: [] });
  });
}

function restore(target: QuerySnapshot) {
  withoutHistory(() => {
    const store = useDesktopStore.getState();
    store.patchRequest(target.request);
    if (store.lockedStatMode !== target.lockedStatMode) store.setLockedStatMode(target.lockedStatMode);
  });
}

export function undoQuery(): QueryStep | null {
  const { past, future } = useQueryHistory.getState();
  const step = past.at(-1);
  if (!step) return null;
  restore(step.before);
  useQueryHistory.setState({ past: past.slice(0, -1), future: [...future, step], announcement: `Undid ${describeStep(step)}` });
  return step;
}

export function redoQuery(): QueryStep | null {
  const { past, future } = useQueryHistory.getState();
  const step = future.at(-1);
  if (!step) return null;
  restore(step.after);
  useQueryHistory.setState({ past: [...past, { ...step, at: 0 }], future: future.slice(0, -1), announcement: `Redid ${describeStep(step)}` });
  return step;
}

const LABELS: Partial<Record<keyof OptimizeRequestDto, string>> = {
  className: "Class",
  vig: "VIG", mnd: "MND", end: "END", strStat: "STR", dex: "DEX", intStat: "INT", fai: "FAI", arc: "ARC",
  minStr: "Min STR", minDex: "Min DEX", minInt: "Min INT", minFai: "Min FAI", minArc: "Min ARC",
  lockStr: "STR lock", lockDex: "DEX lock", lockInt: "INT lock", lockFai: "FAI lock", lockArc: "ARC lock",
  standardMaxUpgrade: "Standard cap", somberMaxUpgrade: "Somber cap", exactUpgrade: "Exact levels",
  twoHanding: "Two-handing", dlcScaling: "DLC scaling", scadutreeLevel: "Blessing",
  weaponName: "Weapon", affinity: "Affinity", aowName: "Skill", weaponTypeKey: "Weapon type",
  somberFilter: "Somber filter", filters: "Filters", resultGrouping: "Grouping", objective: "Objective", topK: "Top results",
};

function formatValue(key: keyof OptimizeRequestDto, value: unknown): string {
  if (key === "objective") return objectiveLabel(value as ObjectiveId);
  if (value === null || value === undefined) return key.startsWith("lock") ? "open" : "any";
  if (typeof value === "boolean") return value ? "on" : "off";
  if (key === "standardMaxUpgrade" || key === "somberMaxUpgrade") return `+${value}`;
  return String(value);
}

function changedKeys(before: QuerySnapshot, after: QuerySnapshot): Array<keyof OptimizeRequestDto> {
  return (Object.keys(LABELS) as Array<keyof OptimizeRequestDto>)
    .filter((key) => stableSignature(before.request[key]) !== stableSignature(after.request[key]));
}

// "STR 12 → 40" for one change, otherwise the changed names: "Class, VIG, MND and 6 more".
export function describeStep(step: QueryStep): string {
  const { before, after } = step;
  const keys = changedKeys(before, after);
  const changes = keys.map((key) => ({ key, label: LABELS[key]! }));
  if (before.lockedStatMode !== after.lockedStatMode) {
    changes.push({ key: "lockStr", label: after.lockedStatMode ? "Locked stats on" : "Locked stats off" });
  }
  if (!changes.length) return "query edit";
  if (changes.length === 1 && keys.length === 1 && keys[0] !== "filters") {
    const key = keys[0];
    return `${changes[0].label} ${formatValue(key, before.request[key])} → ${formatValue(key, after.request[key])}`;
  }
  const names = changes.map((change) => change.label);
  return names.length > 3 ? `${names.slice(0, 3).join(", ")} and ${names.length - 3} more` : names.join(", ");
}
