import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import { cachedWeaponProfile } from "./analysis-cache";
import { SearchableSelect, openOption } from "./SearchableSelect";
import { WeaponProfileDto } from "./types";

export function resolveAowSelection(
  profile: Pick<WeaponProfileDto, "canChangeAow" | "nativeSkillName" | "compatibleAows">,
  value: string | null,
  weaponChanged: boolean,
): string | null {
  if (!profile.canChangeAow) return profile.nativeSkillName;
  if (weaponChanged && (value === null || value === "__match_selected__")) {
    return profile.nativeSkillName && profile.compatibleAows.includes(profile.nativeSkillName)
      ? profile.nativeSkillName : null;
  }
  return value && value !== "__match_selected__" && !profile.compatibleAows.includes(value) ? null : value;
}

export function AowSelect(props: {
  label: string;
  profileId: string;
  weaponName: string | null;
  affinity: string | null;
  catalogNames?: string[];
  value: string | null;
  allowMatchSelected?: boolean;
  onChange: (value: string | null) => void;
  setError: (message: string | null) => void;
}) {
  const { profileId, weaponName, affinity, catalogNames } = props;
  const current = useRef(props);
  current.current = props;
  const previousWeapon = useRef(weaponName ? JSON.stringify([profileId, weaponName]) : null);
  const key = JSON.stringify([profileId, weaponName, affinity]);
  const [loaded, setLoaded] = useState<{
    key: string;
    names: string[];
    profile: WeaponProfileDto | null;
  } | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    async function load() {
      const profile = weaponName
        ? await cachedWeaponProfile(profileId, weaponName, affinity, controller.signal)
        : null;
      const names = profile?.compatibleAows ?? (affinity
        ? await api.compatibleAowNamesForAffinity(profileId, affinity)
        : catalogNames ?? []);
      if (controller.signal.aborted) return;
      const weaponKey = weaponName ? JSON.stringify([profileId, weaponName]) : null;
      if (profile) {
        const next = resolveAowSelection(profile, current.current.value, weaponKey !== previousWeapon.current);
        if (next !== current.current.value) current.current.onChange(next);
      }
      previousWeapon.current = weaponKey;
      setLoaded({ key, names, profile });
    }
    load().catch((error) => {
      if (!controller.signal.aborted) current.current.setError(error instanceof Error ? error.message : String(error));
    });
    return () => controller.abort();
  }, [profileId, weaponName, affinity, catalogNames, key]);

  const ready = loaded?.key === key;
  const fixed = ready && loaded.profile?.canChangeAow === false;
  const nativeSkill = loaded?.profile?.nativeSkillName ?? null;
  return (
    <SearchableSelect
      label={fixed ? `${props.label} (fixed)` : props.label}
      value={fixed ? nativeSkill : props.value}
      options={fixed ? [{ value: nativeSkill, label: nativeSkill ?? "Native skill" }] : [
        ...(props.allowMatchSelected ? [{ value: "__match_selected__", label: "<Match Selected>" }] : []),
        openOption("Automatic (best legal skill)"),
        ...(ready ? loaded.names : []).map((name) => ({ value: name, label: name })),
      ]}
      disabled={!ready || fixed}
      placeholder={ready ? "Automatic (best legal skill)" : "Loading skills..."}
      onChange={props.onChange}
    />
  );
}
