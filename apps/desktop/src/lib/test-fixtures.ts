import type { SolvedBuildDto } from "./types";

// One complete solved build for unit tests; each test overrides only the fields it is about.
export function solvedBuild(overrides: Partial<SolvedBuildDto> = {}): SolvedBuildDto {
  return {
    weaponId: 1,
    weaponName: "Uchigatana",
    affinity: "Keen",
    isSomber: false,
    upgrade: 25,
    stats: { strStat: 18, dex: 40, intStat: 9, fai: 8, arc: 8 },
    ar: { physical: 500, magic: 0, fire: 0, lightning: 0, holy: 0, total: 500 },
    aowId: 100,
    aowName: "Unsheathe",
    bleedBuildup: 45,
    bleedBuildupAdd: 0,
    frostBuildup: 0,
    poisonBuildup: 0,
    scarletRotBuildup: 0,
    sleepBuildup: 0,
    madnessBuildup: 0,
    deathBuildup: 0,
    aowFirstHitDamage: 300,
    aowFullSequenceDamage: 300,
    aowRoute: null,
    score: 500,
    ...overrides,
  };
}
