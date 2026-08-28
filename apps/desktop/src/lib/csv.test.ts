import { describe, expect, it } from "vitest";

import { rankingsToCsv } from "./csv";
import type { SolvedBuildDto } from "./types";

const row: SolvedBuildDto = {
  weaponId: 100,
  weaponName: "Uchigatana",
  weaponTypeName: "Katana",
  affinity: "Blood",
  isSomber: false,
  upgrade: 25,
  stats: { strStat: 12, dex: 40, intStat: 9, fai: 8, arc: 45 },
  requirements: { strStat: 11, dex: 15, intStat: 0, fai: 0, arc: 0 },
  effectiveScaling: { str: 0.4, dex: 2.25, int: 0, fai: 0, arc: 1.1 },
  ar: { physical: 500, magic: 0, fire: 0, lightning: 0, holy: 0, total: 500 },
  aowId: 1,
  aowName: "Seppuku",
  bleedBuildup: 110,
  bleedBuildupAdd: 30,
  frostBuildup: 0,
  poisonBuildup: 0,
  scarletRotBuildup: 0,
  sleepBuildup: 0,
  madnessBuildup: 0,
  deathBuildup: 0,
  aowFirstHitDamage: 0,
  aowFullSequenceDamage: 0,
  aowRoute: null,
  score: 110_500,
};

describe("rankings CSV model provenance", () => {
  it("keeps model identity and unsupported-mechanic assumptions on every exported row", () => {
    const csv = rankingsToCsv([row], {
      profileId: "convergence",
      appVersion: "0.8.0",
      schemaVersion: "7",
      datasetVersion: "2026.07",
      modelVersion: "aow-routes-v2",
      objective: "max_ar_plus_bleed",
      assumptions: "raw values; enemy defense and negation not applied; resistance growth and proc damage excluded; temporary buff stacking not universal",
      separateUpgradeCaps: false,
      aowModelSupported: false,
      extendedScalingGrades: true,
    });

    const [headers, values] = csv.trim().split("\r\n");
    expect(headers).toContain("profile_id,app_version,schema_version,dataset_version,model_version,objective,assumptions");
    expect(values).toContain("convergence,0.8.0,7,2026.07,aow-routes-v2,max_ar_plus_bleed");
    expect(values).toContain("enemy defense and negation not applied");
    expect(values).toContain("resistance growth and proc damage excluded");
    expect(values).toContain("temporary buff stacking not universal");
    expect(headers).toContain("weapon_id,weapon,weapon_type");
    expect(headers).toContain("upgrade_path,is_somber");
    expect(headers).toContain("grade_str,grade_dex,grade_int,grade_fai,grade_arc");
    expect(headers).toContain("bleed,bleed_add,frost,poison,scarlet_rot,sleep,madness,death");
    expect(values).toContain("100,Uchigatana,Katana");
    expect(values).toContain("unified,,11,15,0,0,0");
    expect(values).toContain("D,S++,'-,'-,B");
  });

  it("neutralizes spreadsheet formulas in exported text", () => {
    const csv = rankingsToCsv([{ ...row, weaponName: "=HYPERLINK(\"bad\")" }], {
      profileId: "vanilla",
      appVersion: "0.10.2",
      schemaVersion: "7",
      datasetVersion: "vanilla-1.16.1",
      modelVersion: "aow-routes-v2",
      objective: "max_ar",
      assumptions: "+external input",
    });

    expect(csv).toContain("\"'=HYPERLINK(\"\"bad\"\")\"");
    expect(csv).toContain("'+external input");
  });
});
