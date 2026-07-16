import { SolvedBuildDto } from "./types";

export interface RankingsExportMetadata {
  profileId: string;
  appVersion: string;
  schemaVersion: string;
  datasetVersion: string;
  modelVersion: string;
  objective: string;
  assumptions: string;
}

type CsvColumn = {
  header: string;
  value: (row: SolvedBuildDto, index: number) => string | number | boolean | null;
};

const CSV_COLUMNS: CsvColumn[] = [
  { header: "rank", value: (_row, index) => index + 1 },
  { header: "weapon", value: (row) => row.weaponName },
  { header: "affinity", value: (row) => row.affinity },
  { header: "aow", value: (row) => row.aowName ?? "Native" },
  { header: "upgrade", value: (row) => row.upgrade },
  { header: "is_somber", value: (row) => row.isSomber },
  { header: "str", value: (row) => row.stats.strStat },
  { header: "dex", value: (row) => row.stats.dex },
  { header: "int", value: (row) => row.stats.intStat },
  { header: "fai", value: (row) => row.stats.fai },
  { header: "arc", value: (row) => row.stats.arc },
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
  { header: "aow_first_hit", value: (row) => row.aowFirstHitDamage },
  { header: "aow_full_sequence", value: (row) => row.aowFullSequenceDamage },
  { header: "aow_route_id", value: (row) => row.aowRoute?.routeId ?? null },
  { header: "aow_route", value: (row) => row.aowRoute?.routeLabel ?? null },
  { header: "aow_stamina", value: (row) => row.aowRoute?.totalStaminaCost ?? null },
  { header: "aow_route_bleed", value: (row) => row.aowRoute?.totalStatusBuildup.bleed ?? null },
  { header: "aow_route_frost", value: (row) => row.aowRoute?.totalStatusBuildup.frost ?? null },
  { header: "aow_route_poison", value: (row) => row.aowRoute?.totalStatusBuildup.poison ?? null },
  {
    header: "aow_physical_attributes",
    value: (row) => row.aowRoute?.actions.flatMap((action) => action.hits.map((hit) => hit.physicalAttackAttribute)).join("|") ?? null,
  },
  {
    header: "aow_warnings",
    value: (row) => row.aowRoute?.actions.flatMap((action) => action.hits.flatMap((hit) => hit.warnings)).join(" | ") ?? null,
  },
  { header: "score", value: (row) => row.score },
];

export const csvHeaders = CSV_COLUMNS.map((column) => column.header);

export function rankingsToCsv(rows: SolvedBuildDto[], metadata: RankingsExportMetadata): string {
  const metadataHeaders = ["profile_id", "app_version", "schema_version", "dataset_version", "model_version", "objective", "assumptions"];
  const metadataCells = [metadata.profileId, metadata.appVersion, metadata.schemaVersion, metadata.datasetVersion, metadata.modelVersion, metadata.objective, metadata.assumptions];
  const lines = [
    [...metadataHeaders, ...csvHeaders].join(","),
    ...rows.map((row, index) =>
      [...metadataCells.map(csvCell), ...CSV_COLUMNS.map((column) => csvCell(column.value(row, index)))].join(","),
    ),
  ];
  return `${lines.join("\r\n")}\r\n`;
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

export function downloadCsv(filename: string, csv: string) {
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

function csvCell(value: string | number | boolean | null): string {
  if (value === null) {
    return "";
  }
  const text = String(value);
  if (!/[",\r\n]/.test(text)) {
    return text;
  }
  return `"${text.replace(/"/g, '""')}"`;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}
