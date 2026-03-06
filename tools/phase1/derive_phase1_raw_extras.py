#!/usr/bin/env python3

from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

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
    "poison": ("poizonAttackPower", "diseaseAttackPower"),
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
    weapon_rows: list[dict[str, str]],
    sp_effect_rows: dict[int, dict[str, str]],
    weapon_csv_rows: list[dict[str, str]],
) -> list[dict[str, object]]:
    by_id = {int(row["weapon_id"]): row for row in weapon_csv_rows}
    rows_out: list[dict[str, object]] = []
    for weapon in weapon_rows:
        weapon_id = to_int(weapon, "id")
        if weapon_id % 100 != 0:
            continue
        csv_row = by_id.get(weapon_id)
        if csv_row is None:
            continue

        effect_ids: list[int] = []
        for field in WEAPON_EFFECT_FIELDS:
            effect_id = to_int(weapon, field, -1)
            if effect_id > 0 and effect_id not in effect_ids:
                effect_ids.append(effect_id)

        totals = {status: 0.0 for status in STATUS_FIELDS}
        for effect_id in effect_ids:
            effect = sp_effect_rows.get(effect_id, {})
            for status, fields in STATUS_FIELDS.items():
                for field in fields:
                    totals[status] += to_float(effect, field, 0.0)

        rows_out.append(
            {
                "weapon_id": csv_row["weapon_id"],
                "name": csv_row["name"],
                "affinity": csv_row["affinity"],
                "effect_ids": "|".join(str(effect_id) for effect_id in effect_ids),
                "bleed": _fmt(totals["bleed"]),
                "frost": _fmt(totals["frost"]),
                "poison": _fmt(totals["poison"]),
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


def main() -> int:
    args = parse_args()
    workdir = args.workdir
    phase1_dir = args.phase1
    output_dir = args.output

    weapon_xml = workdir / "EquipParamWeapon.param.xml"
    gem_xml = workdir / "EquipParamGem.param.xml"
    sp_effect_xml = workdir / "SpEffectParam.param.xml"
    if not weapon_xml.exists() or not sp_effect_xml.exists() or not gem_xml.exists():
        raise FileNotFoundError(f"Serialized XML files not found under {workdir}")

    weapon_rows = list(iter_param_rows(weapon_xml))
    gem_rows = list(iter_param_rows(gem_xml))
    sp_effect_rows = {to_int(row, "id"): row for row in iter_param_rows(sp_effect_xml)}
    weapon_csv_rows = read_csv(phase1_dir / "weapons.csv")

    weapon_passives = build_weapon_passives(weapon_rows, sp_effect_rows, weapon_csv_rows)
    exact_aow_compat = build_exact_aow_compat(weapon_csv_rows, canonical_gem_rows(gem_rows))

    write_csv(
        output_dir / "weapon_passives.csv",
        ["weapon_id", "name", "affinity", "effect_ids", "bleed", "frost", "poison", "sleep", "madness", "death"],
        weapon_passives,
    )
    write_csv(
        output_dir / "aow_weapon_compat.csv",
        ["aow_id", "aow_name", "weapon_id", "weapon_name", "affinity", "weapon_type_name", "weapon_type_keys"],
        exact_aow_compat,
    )
    print(f"Wrote {len(weapon_passives)} weapon passive rows")
    print(f"Wrote {len(exact_aow_compat)} exact AoW compatibility rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
