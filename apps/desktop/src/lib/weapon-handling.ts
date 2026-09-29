import type { WeaponProfileDto } from "./types";

export function effectiveWeaponStrength(
  strength: number,
  userTwoHanding: boolean,
  profile: Pick<WeaponProfileDto, "forcesTwoHanding" | "disablesTwoHandBonus"> | null,
): number | null {
  if (!profile) return null;
  return (userTwoHanding || profile.forcesTwoHanding) && !profile.disablesTwoHandBonus
    ? Math.floor(strength * 3 / 2) : strength;
}
