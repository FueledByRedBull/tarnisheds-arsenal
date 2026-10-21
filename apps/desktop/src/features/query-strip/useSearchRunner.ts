import { useEffect, useRef, useState } from "react";
import { resolveAowSelection } from "../../lib/AowSelect";
import { api } from "../../lib/api";
import { cachedWeaponProfile } from "../../lib/analysis-cache";
import { stableSignature } from "../../lib/session";
import { useDesktopStore } from "../../lib/state";
import { OptimizeRequestDto } from "../../lib/types";
import { cancelSearchFromStore, runSearchFromStore } from "../../lib/workflows";
import { withoutHistory } from "../../lib/query-history";
import { errorMessage } from "../../lib/format";

// Owns Search, its loadout preparation and cancellation. `markManualWeapon` records that the
// player picked a weapon by hand, which is the only case that requests its native skill.
export function useSearchRunner() {
  const patchRequest = useDesktopStore((state) => state.patchRequest);
  const setError = useDesktopStore((state) => state.setError);
  const setSearching = useDesktopStore((state) => state.setSearching);
  const setActiveJobId = useDesktopStore((state) => state.setActiveJobId);
  const setProgress = useDesktopStore((state) => state.setProgress);
  const isSearching = useDesktopStore((state) => state.isSearching);
  const [searchStartedAt, setSearchStartedAt] = useState<number | null>(null);
  const [isPreparingSearch, setPreparingSearch] = useState(false);
  const searchPreparation = useRef<AbortController | null>(null);
  const manualWeaponRevision = useRef<number | null>(null);
  const [searchCancellationRequested, setSearchCancellationRequested] = useState(false);

  const idle = !isSearching && !isPreparingSearch;
  const [wasIdle, setWasIdle] = useState(idle);
  if (wasIdle !== idle) {
    setWasIdle(idle);
    if (idle) {
      setSearchStartedAt(null);
      setSearchCancellationRequested(false);
    }
  }
  useEffect(() => () => searchPreparation.current?.abort(), []);

  async function runSearch() {
    const submitted = useDesktopStore.getState();
    const input = submitted.request;
    const weaponChanged = submitted.loadoutSelectionRevision === manualWeaponRevision.current;
    manualWeaponRevision.current = null;
    const controller = new AbortController();
    searchPreparation.current?.abort();
    searchPreparation.current = controller;
    const unchangedInputs = () => {
      const current = useDesktopStore.getState();
      return current.activeWorkspace === submitted.activeWorkspace
        && current.lockedStatMode === submitted.lockedStatMode
        && current.loadoutSelectionRevision === submitted.loadoutSelectionRevision
        && stableSignature(current.request) === stableSignature(input);
    };
    const unsubscribe = useDesktopStore.subscribe(() => {
      if (!unchangedInputs()) controller.abort();
    });
    setSearchCancellationRequested(false);
    setSearchStartedAt(Date.now());
    setError(null);
    setProgress(null);
    setPreparingSearch(true);
    try {
      let loadout: Pick<OptimizeRequestDto, "affinity" | "aowName"> | null = null;
      if (input.weaponName) {
        let affinity = input.affinity;
        let profile = await cachedWeaponProfile(input.profileId, input.weaponName, affinity, controller.signal);
        if (affinity && !profile.affinities.includes(affinity)) {
          affinity = profile.affinities[0] ?? null;
          profile = await cachedWeaponProfile(input.profileId, input.weaponName, affinity, controller.signal);
        }
        const aowName = resolveAowSelection(profile, input.aowName, weaponChanged);
        if (input.affinity !== affinity || input.aowName !== aowName) {
          loadout = { affinity, aowName };
        }
      }
      if (controller.signal.aborted || !unchangedInputs()) return;
      unsubscribe();
      const resolved = loadout;
      if (resolved) withoutHistory(() => patchRequest(resolved));
      searchPreparation.current = null;
      setPreparingSearch(false);
      await runSearchFromStore();
    } catch (error) {
      if (!controller.signal.aborted) setError(errorMessage(error));
    } finally {
      unsubscribe();
      if (searchPreparation.current === controller) {
        if (weaponChanged && useDesktopStore.getState().loadoutSelectionRevision === submitted.loadoutSelectionRevision) {
          manualWeaponRevision.current = submitted.loadoutSelectionRevision;
        }
        searchPreparation.current = null;
        setPreparingSearch(false);
      }
    }
  }

  async function cancelSearch() {
    if (searchPreparation.current) {
      searchPreparation.current.abort();
      return;
    }
    if (searchCancellationRequested) return;
    setSearchCancellationRequested(true);
    const { activeJobId: currentJobId, searchGeneration } = useDesktopStore.getState();
    if (!currentJobId) {
      cancelSearchFromStore(searchGeneration);
      return;
    }
    const isCurrent = () => {
      const current = useDesktopStore.getState();
      return current.searchGeneration === searchGeneration && current.activeJobId === currentJobId;
    };
    try {
      const cancelled = await api.cancelSearch(currentJobId);
      if (!isCurrent()) return;
      if (!cancelled) {
        setSearching(false);
        setActiveJobId(null);
        setProgress(null);
        setSearchStartedAt(null);
        setSearchCancellationRequested(false);
      }
    } catch (error) {
      if (!isCurrent()) return;
      setSearchCancellationRequested(false);
      setError(errorMessage(error));
    }
  }

  return {
    runSearch,
    cancelSearch,
    searchStartedAt,
    isPreparingSearch,
    searchCancellationRequested,
    manualWeaponRevision,
    markManualWeapon: () => {
      manualWeaponRevision.current = useDesktopStore.getState().loadoutSelectionRevision;
    },
    clearManualWeapon: () => {
      manualWeaponRevision.current = null;
    },
  };
}

export type SearchRunner = ReturnType<typeof useSearchRunner>;
