import { COMBAT_STATS } from "./combat-stats";
import { hasAowDamage } from "./objectives";
import { scalingLetter } from "./session";
import { SolvedBuildDto } from "./types";

export interface RankingsExportMetadata {
  profileId: string;
  appVersion: string;
  schemaVersion: string;
  datasetVersion: string;
  modelVersion: string;
  objective: string;
  assumptions: string;
  separateUpgradeCaps?: boolean;
  aowModelSupported: boolean;
  extendedScalingGrades?: boolean;
}

type CsvColumn = {
  header: string;
  value: (row: SolvedBuildDto, index: number, metadata: RankingsExportMetadata) => string | number | boolean | null;
};

type CombatStat = (typeof COMBAT_STATS)[number];

// One column per combat stat, named by its short label: "req_str" through "req_arc".
function statColumns(
  prefix: string,
  value: (row: SolvedBuildDto, stat: CombatStat, metadata: RankingsExportMetadata) => string | number | null,
): CsvColumn[] {
  return COMBAT_STATS.map((stat) => ({
    header: `${prefix}${stat.label.toLowerCase()}`,
    value: (row, _index, metadata) => value(row, stat, metadata),
  }));
}

const CSV_COLUMNS: CsvColumn[] = [
  { header: "rank", value: (_row, index) => index + 1 },
  { header: "weapon_id", value: (row) => row.weaponId },
  { header: "weapon", value: (row) => row.weaponName },
  { header: "weapon_type", value: (row) => row.weaponTypeName ?? null },
  { header: "affinity", value: (row) => row.affinity },
  { header: "aow", value: (row) => row.aowName ?? "No skill" },
  { header: "upgrade", value: (row) => row.upgrade },
  { header: "upgrade_path", value: (row, _index, metadata) => metadata.separateUpgradeCaps === false ? "unified" : row.isSomber ? "somber" : "standard" },
  { header: "is_somber", value: (row, _index, metadata) => metadata.separateUpgradeCaps === false ? null : row.isSomber },
  ...statColumns("req_", (row, stat) => row.requirements?.[stat.key] ?? null),
  ...statColumns("scaling_", (row, stat) => row.effectiveScaling?.[stat.scaling] ?? null),
  ...statColumns("grade_", (row, stat, metadata) => row.effectiveScaling
    ? scalingLetter(row.effectiveScaling[stat.scaling], metadata.extendedScalingGrades) : null),
  ...statColumns("", (row, stat) => row.stats[stat.key]),
  { header: "ar_total", value: (row) => row.ar.total },
  { header: "ar_physical", value: (row) => row.ar.physical },
  { header: "ar_magic", value: (row) => row.ar.magic },
  { header: "ar_fire", value: (row) => row.ar.fire },
  { header: "ar_lightning", value: (row) => row.ar.lightning },
  { header: "ar_holy", value: (row) => row.ar.holy },
  { header: "bleed", value: (row) => row.bleedBuildup },
  { header: "bleed_add", value: (row) => row.bleedBuildupAdd },
  { header: "frost", value: (row) => row.frostBuildup },
  { header: "poison", value: (row) => row.poisonBuildup },
  { header: "scarlet_rot", value: (row) => row.scarletRotBuildup },
  { header: "sleep", value: (row) => row.sleepBuildup },
  { header: "madness", value: (row) => row.madnessBuildup },
  { header: "death", value: (row) => row.deathBuildup },
  { header: "aow_first_hit", value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowFirstHitDamage : null },
  { header: "aow_full_sequence", value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowFullSequenceDamage : null },
  { header: "aow_route_id", value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowRoute?.routeId ?? null : null },
  { header: "aow_route", value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowRoute?.routeLabel ?? null : null },
  { header: "aow_stamina", value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowRoute?.totalStaminaCost ?? null : null },
  { header: "aow_route_bleed", value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowRoute?.totalStatusBuildup.bleed ?? null : null },
  { header: "aow_route_frost", value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowRoute?.totalStatusBuildup.frost ?? null : null },
  { header: "aow_route_poison", value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowRoute?.totalStatusBuildup.poison ?? null : null },
  {
    header: "aow_physical_attributes",
    value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowRoute?.actions.flatMap((action) => action.hits.map((hit) => hit.physicalAttackAttribute)).join("|") ?? null : null,
  },
  {
    header: "aow_warnings",
    value: (row, _index, metadata) => hasAowDamage(row, metadata.aowModelSupported) ? row.aowRoute?.actions.flatMap((action) => action.hits.flatMap((hit) => hit.warnings)).join(" | ") ?? null : null,
  },
  { header: "score", value: (row) => row.score },
];

export function rankingsToCsv(rows: SolvedBuildDto[], metadata: RankingsExportMetadata): string {
  const metadataHeaders = ["profile_id", "app_version", "schema_version", "dataset_version", "model_version", "objective", "assumptions"];
  const metadataCells = [metadata.profileId, metadata.appVersion, metadata.schemaVersion, metadata.datasetVersion, metadata.modelVersion, metadata.objective, metadata.assumptions];
  const lines = [
    [...metadataHeaders, ...CSV_COLUMNS.map((column) => column.header)].join(","),
    ...rows.map((row, index) =>
      [...metadataCells.map(csvCell), ...CSV_COLUMNS.map((column) => csvCell(column.value(row, index, metadata)))].join(","),
    ),
  ];
  // Excel reads a CSV without a byte order mark in the system code page, garbling accented names.
  return `﻿${lines.join("\r\n")}\r\n`;
}

export function rankingsCsvFilename(profileId: string, now = new Date()): string {
  const stamp = [
    now.getFullYear(),
    pad2(now.getMonth() + 1),
    pad2(now.getDate()),
    "-",
    pad2(now.getHours()),
    pad2(now.getMinutes()),
    pad2(now.getSeconds()),
  ].join("");
  return `tarnisheds-arsenal-${profileId}-rankings-${stamp}.csv`;
}

export function downloadText(filename: string, text: string, type: string) {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  // WebView2 may read the blob after click() returns, so revoking at once could fail the download.
  window.setTimeout(() => URL.revokeObjectURL(url), 30_000);
}

function csvCell(value: string | number | boolean | null): string {
  if (value === null) {
    return "";
  }
  const text = String(value);
  // Only text can carry a spreadsheet formula; numbers and a lone "-" grade stay as they are.
  const safeText = typeof value === "string" && text.length > 1 && /^[=+\-@\t\r]/.test(text) ? `'${text}` : text;
  if (!/[",\r\n]/.test(safeText)) {
    return safeText;
  }
  return `"${safeText.replace(/"/g, '""')}"`;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}
