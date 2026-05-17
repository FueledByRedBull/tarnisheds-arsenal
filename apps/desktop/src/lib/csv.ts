import { SolvedBuildDto } from "./types";

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
  { header: "score", value: (row) => row.score },
];

export const csvHeaders = CSV_COLUMNS.map((column) => column.header);

export function rankingsToCsv(rows: SolvedBuildDto[]): string {
  const lines = [
    csvHeaders.join(","),
    ...rows.map((row, index) =>
      CSV_COLUMNS.map((column) => csvCell(column.value(row, index))).join(","),
    ),
  ];
  return `${lines.join("\r\n")}\r\n`;
}

export function rankingsCsvFilename(now = new Date()): string {
  const stamp = [
    now.getFullYear(),
    pad2(now.getMonth() + 1),
    pad2(now.getDate()),
    "-",
    pad2(now.getHours()),
    pad2(now.getMinutes()),
    pad2(now.getSeconds()),
  ].join("");
  return `tarnisheds-arsenal-rankings-${stamp}.csv`;
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
