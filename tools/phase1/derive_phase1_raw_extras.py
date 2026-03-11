#!/usr/bin/env python3

from __future__ import annotations

import argparse
import csv
import sys
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.phase1_dump import iter_param_rows, to_float, to_int

WEAPON_EFFECT_FIELDS = (
    "spEffectBehaviorId0",
    "spEffectBehaviorId1",
    "spEffectBehaviorId2",
    "residentSpEffectId",
    "residentSpEffectId1",
)

AFFINITY_ATTRS = {
    "Standard": "configurableWepAttr00",
    "Heavy": "configurableWepAttr01",
    "Keen": "configurableWepAttr02",
    "Quality": "configurableWepAttr03",
    "Fire": "configurableWepAttr04",
    "Flame Art": "configurableWepAttr05",
    "Lightning": "configurableWepAttr06",
    "Sacred": "configurableWepAttr07",
    "Magic": "configurableWepAttr08",
    "Cold": "configurableWepAttr09",
    "Poison": "configurableWepAttr10",
    "Blood": "configurableWepAttr11",
    "Occult": "configurableWepAttr12",
}

STATUS_FIELDS = {
    "bleed": ("bloodAttackPower",),
    "frost": ("freezeAttackPower",),
    "poison": ("poizonAttackPower",),
    "scarlet_rot": ("diseaseAttackPower",),
    "sleep": ("sleepAttackPower",),
    "madness": ("madnessAttackPower",),
    "death": ("curseAttackPower",),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Derive extra CSVs directly from unpacked regulation XML.")
    parser.add_argument(
        "--workdir",
        type=Path,
        default=Path("data") / "_work_phase1" / "regulation-bin",
        help="Directory containing serialized param XML files",
    )
    parser.add_argument(
        "--phase1",
        type=Path,
        default=Path("data") / "phase1",
        help="Directory containing the base Phase 1 CSVs",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("data") / "phase1",
        help="Output directory for derived CSVs",
    )
    parser.add_argument(
        "--workbook",
        type=Path,
        default=Path("data") / "phase1" / "ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx",
        help="Workbook path used as a fallback source for SpEffectParam",
    )
    return parser.parse_args()


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def write_csv(path: Path, fieldnames: list[str], rows: list[dict[str, object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow({key: row.get(key, "") for key in fieldnames})


def canonical_gem_rows(gem_rows: list[dict[str, str]]) -> dict[int, dict[str, str]]:
    grouped_rows: dict[int, list[dict[str, str]]] = {}
    for row in gem_rows:
        raw_name = row.get("paramdexName", "").strip()
        if not raw_name.startswith("Ash of War:"):
            continue
        sword_art_id = to_int(row, "swordArtsParamId", -1)
        if sword_art_id < 0:
            continue
        grouped_rows.setdefault(sword_art_id, []).append(row)

    out: dict[int, dict[str, str]] = {}
    for sword_art_id, rows in grouped_rows.items():
        def score(item: dict[str, str]) -> tuple[int, int, int, int]:
            sort_real = 1 if item.get("sortId") not in (None, "", "999999") else 0
            icon_real = 1 if item.get("iconId") not in (None, "", "0") else 0
            special = 1 if to_int(item, "isSpecialSwordArt", 0) != 0 else 0
            return (sort_real, icon_real, special, to_int(item, "id", 0))

        out[sword_art_id] = max(rows, key=score)
    return out


def workbook_sp_effect_rows(workbook_path: Path) -> dict[int, dict[str, str]]:
    main_ns = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
    workbook_ns = {
        "x": "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
        "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    }
    rel_ns = "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}"

    def column_index(cell_ref: str) -> int:
        value = 0
        for char in "".join(ch for ch in cell_ref if ch.isalpha()):
            value = value * 26 + (ord(char.upper()) - 64)
        return value - 1

    with zipfile.ZipFile(workbook_path) as archive:
        shared_strings: list[str] = []
        if "xl/sharedStrings.xml" in archive.namelist():
            sst = ET.fromstring(archive.read("xl/sharedStrings.xml"))
            for item in sst:
                shared_strings.append(
                    "".join(node.text or "" for node in item.iter(f"{main_ns}t"))
                )

        workbook = ET.fromstring(archive.read("xl/workbook.xml"))
        rels = ET.fromstring(archive.read("xl/_rels/workbook.xml.rels"))
        rel_map = {rel.attrib["Id"]: rel.attrib["Target"] for rel in rels}

        target = None
        sheets = workbook.find("x:sheets", workbook_ns)
        for sheet in [] if sheets is None else sheets:
            if sheet.attrib["name"] == "SpEffectParam":
                target = rel_map[sheet.attrib[f"{rel_ns}id"]]
                break
        if target is None:
            raise ValueError("missing SpEffectParam sheet")

        sheet_xml = ET.fromstring(archive.read(f"xl/{target}"))
        sheet_data = sheet_xml.find(f"{main_ns}sheetData")
        if sheet_data is None:
            raise ValueError("missing sheetData for SpEffectParam")

        parsed_rows: list[list[str]] = []
        width = 0
        for row in sheet_data:
            parsed: dict[int, str] = {}
            for cell in row:
                idx = column_index(cell.attrib["r"])
                value = cell.find(f"{main_ns}v")
                if cell.attrib.get("t") == "s":
                    text = "" if value is None else shared_strings[int(value.text or "0")]
                else:
                    text = value.text if value is not None and value.text is not None else ""
                parsed[idx] = text
                width = max(width, idx + 1)
            parsed_rows.append([parsed.get(idx, "") for idx in range(width)])

    if len(parsed_rows) < 3:
        return {}
    headers = parsed_rows[1]
    header_idx = {header: idx for idx, header in enumerate(headers) if header}
    rows_out: dict[int, dict[str, str]] = {}
    for values in parsed_rows[2:]:
        raw_id = values[header_idx.get("ID", 0)].strip()
        if not raw_id:
            continue
        try:
            effect_id = int(float(raw_id))
        except ValueError:
            continue
        rows_out[effect_id] = {
            header: values[idx] if idx < len(values) else ""
            for header, idx in header_idx.items()
        }
    return rows_out


def weapon_effect_ids_from_csv(weapon_passive_rows: list[dict[str, str]]) -> dict[int, list[int]]:
    out: dict[int, list[int]] = {}
    for row in weapon_passive_rows:
        try:
            weapon_id = int(row["weapon_id"])
        except (KeyError, ValueError):
            continue
        effect_ids: list[int] = []
        for raw_effect_id in row.get("effect_ids", "").split("|"):
            raw_effect_id = raw_effect_id.strip()
            if not raw_effect_id:
                continue
            try:
                effect_id = int(raw_effect_id)
            except ValueError:
                continue
            if effect_id not in effect_ids:
                effect_ids.append(effect_id)
        out[weapon_id] = effect_ids
    return out


def ashable_weapon_names(weapon_csv_rows: list[dict[str, str]]) -> set[str]:
    affinities_by_name: dict[str, set[str]] = {}
    for row in weapon_csv_rows:
        affinities_by_name.setdefault(row["name"], set()).add(row["affinity"])
    return {
        name for name, affinities in affinities_by_name.items() if len(affinities) > 1 or "Standard" not in affinities
    }


def aow_valid_for_weapon(
    gem_row: dict[str, str],
    weapon_row: dict[str, str],
    ashable_names: set[str],
) -> bool:
    if weapon_row["name"] not in ashable_names:
        return False

    affinity_attr = AFFINITY_ATTRS.get(weapon_row["affinity"])
    if affinity_attr is None or to_int(gem_row, affinity_attr, 0) == 0:
        return False

    weapon_types = [value for value in weapon_row["weapon_type_keys"].split("|") if value]
    if not weapon_types:
        return False
    return any(to_int(gem_row, f"canMountWep_{weapon_type}", 0) != 0 for weapon_type in weapon_types)


def build_weapon_passives(
    sp_effect_rows: dict[int, dict[str, str]],
    weapon_csv_rows: list[dict[str, str]],
    weapon_effect_ids: dict[int, list[int]],
    existing_weapon_passives: dict[int, dict[str, str]],
) -> list[dict[str, object]]:
    by_id = {int(row["weapon_id"]): row for row in weapon_csv_rows}
    rows_out: list[dict[str, object]] = []
    for weapon_id, csv_row in sorted(by_id.items()):
        csv_row = by_id.get(weapon_id)
        if csv_row is None:
            continue

        effect_ids = weapon_effect_ids.get(weapon_id, [])
        existing = existing_weapon_passives.get(weapon_id, {})
        totals = {
            "bleed": to_float(existing, "bleed", 0.0),
            "frost": to_float(existing, "frost", 0.0),
            "poison": to_float(existing, "poison", 0.0),
            "scarlet_rot": 0.0,
            "sleep": to_float(existing, "sleep", 0.0),
            "madness": to_float(existing, "madness", 0.0),
            "death": to_float(existing, "death", 0.0),
        }
        for effect_id in effect_ids:
            effect = sp_effect_rows.get(effect_id)
            if effect is None:
                continue
            totals["scarlet_rot"] += _safe_to_float(effect, "diseaseAttackPower", 0.0)
        if totals["scarlet_rot"] > 0.0 and totals["poison"] > 0.0:
            totals["poison"] = max(0.0, totals["poison"] - totals["scarlet_rot"])

        rows_out.append(
            {
                "weapon_id": csv_row["weapon_id"],
                "name": csv_row["name"],
                "affinity": csv_row["affinity"],
                "effect_ids": "|".join(str(effect_id) for effect_id in effect_ids),
                "bleed": _fmt(totals["bleed"]),
                "frost": _fmt(totals["frost"]),
                "poison": _fmt(totals["poison"]),
                "scarlet_rot": _fmt(totals["scarlet_rot"]),
                "sleep": _fmt(totals["sleep"]),
                "madness": _fmt(totals["madness"]),
                "death": _fmt(totals["death"]),
            }
        )
    rows_out.sort(key=lambda row: (row["name"], row["affinity"], int(row["weapon_id"])))
    return rows_out


def build_exact_aow_compat(
    weapon_csv_rows: list[dict[str, str]],
    gem_rows_by_aow_id: dict[int, dict[str, str]],
) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []
    ashable_names = ashable_weapon_names(weapon_csv_rows)
    for weapon in weapon_csv_rows:
        for aow_id, gem_row in gem_rows_by_aow_id.items():
            if not aow_valid_for_weapon(gem_row, weapon, ashable_names):
                continue
            rows_out.append(
                {
                    "aow_id": aow_id,
                    "aow_name": gem_row["paramdexName"].replace("Ash of War:", "", 1).strip(),
                    "weapon_id": weapon["weapon_id"],
                    "weapon_name": weapon["name"],
                    "affinity": weapon["affinity"],
                    "weapon_type_name": weapon["weapon_type_name"],
                    "weapon_type_keys": weapon["weapon_type_keys"],
                }
            )
    rows_out.sort(key=lambda row: (row["aow_name"], row["weapon_name"], row["affinity"], int(row["weapon_id"])))
    return rows_out


def _fmt(value: float) -> str:
    text = f"{value:.2f}".rstrip("0").rstrip(".")
    return text if text else "0"


def _safe_to_float(row: dict[str, str], key: str, default: float = 0.0) -> float:
    raw = row.get(key)
    if raw is None or raw == "":
        return default
    try:
        return float(raw)
    except ValueError:
        return default


def main() -> int:
    args = parse_args()
    workdir = args.workdir
    phase1_dir = args.phase1
    output_dir = args.output
    workbook_path = args.workbook

    weapon_xml = workdir / "EquipParamWeapon.param.xml"
    gem_xml = workdir / "EquipParamGem.param.xml"
    sp_effect_xml = workdir / "SpEffectParam.param.xml"
    weapon_csv_rows = read_csv(phase1_dir / "weapons.csv")
    existing_weapon_passive_rows = read_csv(phase1_dir / "weapon_passives.csv")
    existing_weapon_passives = {
        int(row["weapon_id"]): row for row in existing_weapon_passive_rows
    }
    weapon_effect_ids = weapon_effect_ids_from_csv(existing_weapon_passive_rows)

    if sp_effect_xml.exists():
        sp_effect_rows = {to_int(row, "id"): row for row in iter_param_rows(sp_effect_xml)}
    elif workbook_path.exists():
        sp_effect_rows = workbook_sp_effect_rows(workbook_path)
    else:
        raise FileNotFoundError(
            f"Neither {sp_effect_xml} nor workbook {workbook_path} is available for SpEffectParam data"
        )

    weapon_passives = build_weapon_passives(
        sp_effect_rows,
        weapon_csv_rows,
        weapon_effect_ids,
        existing_weapon_passives,
    )

    exact_aow_compat: list[dict[str, object]] = []
    if gem_xml.exists():
        gem_rows = list(iter_param_rows(gem_xml))
        exact_aow_compat = build_exact_aow_compat(weapon_csv_rows, canonical_gem_rows(gem_rows))

    write_csv(
        output_dir / "weapon_passives.csv",
        [
            "weapon_id",
            "name",
            "affinity",
            "effect_ids",
            "bleed",
            "frost",
            "poison",
            "scarlet_rot",
            "sleep",
            "madness",
            "death",
        ],
        weapon_passives,
    )
    if exact_aow_compat:
        write_csv(
            output_dir / "aow_weapon_compat.csv",
            ["aow_id", "aow_name", "weapon_id", "weapon_name", "affinity", "weapon_type_name", "weapon_type_keys"],
            exact_aow_compat,
        )
    print(f"Wrote {len(weapon_passives)} weapon passive rows")
    if exact_aow_compat:
        print(f"Wrote {len(exact_aow_compat)} exact AoW compatibility rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
