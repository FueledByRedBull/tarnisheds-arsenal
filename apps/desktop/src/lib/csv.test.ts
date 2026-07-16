import { describe, expect, it } from "vitest";

import { rankingsToCsv } from "./csv";
import type { SolvedBuildDto } from "./types";

const row: SolvedBuildDto = {
  weaponId: 100,
  weaponName: "Uchigatana",
  affinity: "Blood",
  isSomber: false,
  upgrade: 25,
  stats: { strStat: 12, dex: 40, intStat: 9, fai: 8, arc: 45 },
  ar: { physical: 500, magic: 0, fire: 0, lightning: 0, holy: 0, total: 500 },
  aowId: 1,
  aowName: "Seppuku",
  bleedBuildup: 110,
  bleedBuildupAdd: 30,
  frostBuildup: 0,
  poisonBuildup: 0,
  scarletRotBuildup: 0,
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
    });

    const [headers, values] = csv.trim().split("\r\n");
    expect(headers).toContain("profile_id,app_version,schema_version,dataset_version,model_version,objective,assumptions");
    expect(values).toContain("convergence,0.8.0,7,2026.07,aow-routes-v2,max_ar_plus_bleed");
    expect(values).toContain("enemy defense and negation not applied");
    expect(values).toContain("resistance growth and proc damage excluded");
    expect(values).toContain("temporary buff stacking not universal");
  });
});
