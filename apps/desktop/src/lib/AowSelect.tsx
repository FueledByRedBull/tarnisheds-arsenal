import { useCallback, useEffect, useRef } from "react";
import { api } from "./api";
import { useWeaponProfileResource } from "./hooks";
import { useKeyedResource } from "./keyed-resource";
import { SearchableSelect, openOption } from "./SearchableSelect";
import { WeaponProfileDto } from "./types";
import { withoutHistory } from "./query-history";

export function resolveAowSelection(
  profile: Pick<WeaponProfileDto, "canChangeAow" | "nativeSkillName" | "compatibleAows">,
  value: string | null,
  weaponChanged: boolean,
): string | null {
  if (!profile.canChangeAow) return value === null && !weaponChanged ? null : profile.nativeSkillName;
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
  defaultNativeSkill?: boolean;
  onWeaponResolved?: () => void;
}) {
  const { profileId, weaponName, affinity, catalogNames } = props;
  const current = useRef(props);
  current.current = props;
  const previousWeapon = useRef(weaponName ? JSON.stringify([profileId, weaponName]) : null);
  const weaponResource = useWeaponProfileResource(profileId, weaponName, affinity);
  const profile = weaponResource.profile;
  const loadNames = useCallback(() => affinity
    ? api.compatibleAowNamesForAffinity(profileId, affinity)
    : Promise.resolve(catalogNames ?? []), [profileId, affinity, catalogNames]);
  const namesResource = useKeyedResource(
    weaponName ? null : JSON.stringify([profileId, affinity, catalogNames]), loadNames,
  );
  const resource = weaponName ? weaponResource : namesResource;
  const names = profile?.compatibleAows ?? namesResource.data ?? [];

  useEffect(() => {
    if (resource.status !== "ready") return;
    const weaponKey = weaponName ? JSON.stringify([profileId, weaponName]) : null;
    if (profile) {
      const next = resolveAowSelection(profile, current.current.value,
        current.current.defaultNativeSkill ?? weaponKey !== previousWeapon.current);
      if (next !== current.current.value) withoutHistory(() => current.current.onChange(next));
    }
    previousWeapon.current = weaponKey;
    current.current.onWeaponResolved?.();
  }, [profileId, weaponName, profile, resource.status, props.defaultNativeSkill]);

  const ready = resource.status === "ready";
  const fixed = ready && profile?.canChangeAow === false;
  const nativeSkill = profile?.nativeSkillName ?? null;
  return (
    <>
    <SearchableSelect
      label={fixed ? `${props.label} (fixed)` : props.label}
      value={fixed ? nativeSkill : props.value}
      options={fixed ? [{ value: nativeSkill, label: nativeSkill ?? "Native skill" }] : [
        ...(props.allowMatchSelected ? [{ value: "__match_selected__", label: "<Match Selected>" }] : []),
        openOption("Automatic (best legal skill)"),
        ...(ready ? names : []).map((name) => ({ value: name, label: name })),
      ]}
      disabled={!ready || fixed}
      placeholder={ready ? "Automatic (best legal skill)" : resource.error !== null ? "Skills unavailable" : "Loading skills..."}
      onChange={props.onChange}
    />
    {resource.status === "error" ? (
      <div role="alert">
        <small>Skills unavailable: {resource.error}</small>
        <button type="button" onClick={resource.retry}>Retry {props.label} skills</button>
      </div>
    ) : null}
    </>
  );
}
