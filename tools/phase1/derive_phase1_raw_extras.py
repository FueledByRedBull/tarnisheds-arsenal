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
REINFORCE_OVERLAY_FIELD_TO_WEAPON_FIELD = {
    "spEffectId1": "spEffectBehaviorId0",
    "spEffectId2": "spEffectBehaviorId1",
    "spEffectId3": "spEffectBehaviorId2",
}

AFFINITY_ATTRS = {
    "Standard": ("configurableWepAttr00", 1),
    "Heavy": ("configurableWepAttr01", 1),
    "Keen": ("configurableWepAttr02", 1),
    "Quality": ("configurableWepAttr03", 1),
    "Fire": ("configurableWepAttr04", 1),
    "Flame Art": ("configurableWepAttr05", 1),
    "Lightning": ("configurableWepAttr06", 1),
    "Sacred": ("configurableWepAttr07", 1),
    "Magic": ("configurableWepAttr08", 1),
    "Cold": ("configurableWepAttr09", 1),
    "Poison": ("configurableWepAttr10", 1),
    "Blood": ("configurableWepAttr11", 0),
    "Occult": ("configurableWepAttr12", 0),
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
STATUS_CORRECTION_COLUMNS = {
    status_key: f"{status_key}_uses_status_correction" for status_key in STATUS_FIELDS
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
        canonical_name = raw_name.replace("Ash of War:", "", 1).strip()
        if not canonical_name:
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
def weapon_effect_ids_from_param_rows(
    weapon_param_rows: dict[int, dict[str, str]],
    weapon_csv_rows: list[dict[str, str]],
) -> dict[int, list[int]]:
    out: dict[int, list[int]] = {}
    for csv_row in weapon_csv_rows:
        weapon_id = int(csv_row["weapon_id"])
        weapon_row = weapon_param_rows.get(weapon_id)
        if weapon_row is None:
            continue
        effect_ids: list[int] = []
        for field_name in WEAPON_EFFECT_FIELDS:
            effect_id = to_int(weapon_row, field_name, 0)
            if effect_id > 0 and effect_id not in effect_ids:
                effect_ids.append(effect_id)
        out[weapon_id] = effect_ids
    return out


def aow_valid_for_weapon(
    gem_row: dict[str, str],
    weapon_row: dict[str, str],
) -> bool:
    if to_int(weapon_row, "disable_gem_attr", 0) != 0:
        return False

    affinity_attr = AFFINITY_ATTRS.get(weapon_row["affinity"])
    if affinity_attr is None:
        return False
    affinity_field, affinity_default = affinity_attr
    if to_int(gem_row, affinity_field, affinity_default) == 0:
        return False

    weapon_types = [value for value in weapon_row["weapon_type_keys"].split("|") if value]
    if not weapon_types:
        return False
    return any(to_int(gem_row, f"canMountWep_{weapon_type}", 0) != 0 for weapon_type in weapon_types)


def build_weapon_passives(
    sp_effect_rows: dict[int, dict[str, str]],
    weapon_csv_rows: list[dict[str, str]],
    weapon_effect_ids: dict[int, list[int]],
) -> list[dict[str, object]]:
    by_id = {int(row["weapon_id"]): row for row in weapon_csv_rows}
    rows_out: list[dict[str, object]] = []
    for weapon_id, csv_row in sorted(by_id.items()):
        csv_row = by_id.get(weapon_id)
        if csv_row is None:
            continue

        effect_ids = weapon_effect_ids.get(weapon_id, [])
        totals = {
            "bleed": 0.0,
            "frost": 0.0,
            "poison": 0.0,
            "scarlet_rot": 0.0,
            "sleep": 0.0,
            "madness": 0.0,
            "death": 0.0,
        }
        correction_flags: dict[str, bool | None] = {
            status_key: None for status_key in STATUS_FIELDS
        }
        for effect_id in effect_ids:
            effect = sp_effect_rows.get(effect_id)
            if effect is None:
                continue
            correction_value = _safe_optional_bool(effect, "isUseStatusAilmentAtkPowerCorrect")
            for status_key, source_fields in STATUS_FIELDS.items():
                amount = sum(_safe_to_float(effect, field_name, 0.0) for field_name in source_fields)
                if amount <= 0.0:
                    continue
                if status_key == "scarlet_rot":
                    totals[status_key] += amount
                else:
                    totals[status_key] = max(totals[status_key], amount)
                if correction_value is not None:
                    correction_flags[status_key] = correction_value
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
                **{
                    STATUS_CORRECTION_COLUMNS[status_key]: _fmt_optional_bool(correction_flags[status_key])
                    for status_key in STATUS_FIELDS
                },
            }
        )
    rows_out.sort(key=lambda row: (row["name"], row["affinity"], int(row["weapon_id"])))
    return rows_out


def build_weapon_passive_overlays(
    sp_effect_rows: dict[int, dict[str, str]],
    weapon_csv_rows: list[dict[str, str]],
    weapon_param_rows: dict[int, dict[str, str]],
    reinforce_rows: dict[int, dict[str, str]],
    max_level_by_type: dict[int, int],
) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []
    for csv_row in weapon_csv_rows:
        weapon_id = int(csv_row["weapon_id"])
        weapon_row = weapon_param_rows.get(weapon_id)
        if weapon_row is None:
            continue
        reinforce_type = int(csv_row["reinforce_type"])
        max_level = max_level_by_type.get(reinforce_type, 0)
        first_reinforce_row = reinforce_rows.get(reinforce_type + 1)
        if first_reinforce_row is None:
            continue

        active_fields = [
            (reinforce_field, weapon_field)
            for reinforce_field, weapon_field in REINFORCE_OVERLAY_FIELD_TO_WEAPON_FIELD.items()
            if to_int(first_reinforce_row, reinforce_field, 0) > 0 and to_int(weapon_row, weapon_field, 0) > 0
        ]
        if not active_fields:
            continue

        for level in range(max_level + 1):
            totals = {status_key: 0.0 for status_key in STATUS_FIELDS}
            correction_flags: dict[str, bool | None] = {status_key: None for status_key in STATUS_FIELDS}
            effect_ids: list[int] = []

            for _, weapon_field in active_fields:
                base_effect_id = to_int(weapon_row, weapon_field, 0)
                if base_effect_id <= 0:
                    continue
                effect_id = base_effect_id + level
                effect = sp_effect_rows.get(effect_id)
                if effect is None:
                    continue
                effect_ids.append(effect_id)
                correction_value = _safe_optional_bool(effect, "isUseStatusAilmentAtkPowerCorrect")
                for status_key, source_fields in STATUS_FIELDS.items():
                    amount = sum(_safe_to_float(effect, field_name, 0.0) for field_name in source_fields)
                    if amount <= 0.0:
                        continue
                    totals[status_key] += amount
                    if correction_value is not None:
                        correction_flags[status_key] = correction_value

            if not effect_ids:
                continue

            rows_out.append(
                {
                    "weapon_id": csv_row["weapon_id"],
                    "name": csv_row["name"],
                    "affinity": csv_row["affinity"],
                    "level": str(level),
                    "effect_ids": "|".join(str(effect_id) for effect_id in effect_ids),
                    **{status_key: _fmt(totals[status_key]) for status_key in STATUS_FIELDS},
                    **{
                        STATUS_CORRECTION_COLUMNS[status_key]: _fmt_optional_bool(correction_flags[status_key])
                        for status_key in STATUS_FIELDS
                    },
                }
            )
    rows_out.sort(key=lambda row: (row["name"], row["affinity"], int(row["weapon_id"]), int(row["level"])))
    return rows_out


def build_exact_aow_compat(
    weapon_csv_rows: list[dict[str, str]],
    gem_rows_by_aow_id: dict[int, dict[str, str]],
) -> list[dict[str, object]]:
    rows_out: list[dict[str, object]] = []
    for weapon in weapon_csv_rows:
        for aow_id, gem_row in gem_rows_by_aow_id.items():
            if not aow_valid_for_weapon(gem_row, weapon):
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


def _safe_optional_bool(row: dict[str, str], key: str) -> bool | None:
    raw = row.get(key)
    if raw is None:
        return None
    value = raw.strip()
    if not value:
        return None
    return value not in {"0", "0.0"}

def _fmt_optional_bool(value: bool | None) -> str:
    if value is None:
        return ""
    return "1" if value else "0"


def export_regulation_extras(
    *,
    weapon_csv_rows: list[dict[str, str]],
    reinforce_csv_rows: list[dict[str, str]],
    weapon_param_rows: dict[int, dict[str, str]],
    reinforce_param_rows: dict[int, dict[str, str]],
    gem_rows: list[dict[str, str]],
    sp_effect_rows: dict[int, dict[str, str]],
    output_dir: Path,
) -> None:
    max_level_by_type: dict[int, int] = {}
    for row in reinforce_csv_rows:
        reinforce_type = int(row["reinforce_type"])
        level = int(row["level"])
        max_level_by_type[reinforce_type] = max(max_level_by_type.get(reinforce_type, 0), level)

    weapon_effect_ids = weapon_effect_ids_from_param_rows(weapon_param_rows, weapon_csv_rows)

    weapon_passives = build_weapon_passives(
        sp_effect_rows,
        weapon_csv_rows,
        weapon_effect_ids,
    )
    weapon_passive_overlays = build_weapon_passive_overlays(
        sp_effect_rows,
        weapon_csv_rows,
        weapon_param_rows,
        reinforce_param_rows,
        max_level_by_type,
    )

    exact_aow_compat: list[dict[str, object]] = []
    if gem_rows:
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
            *STATUS_CORRECTION_COLUMNS.values(),
        ],
        weapon_passives,
    )
    write_csv(
        output_dir / "weapon_passive_overlays.csv",
        [
            "weapon_id",
            "name",
            "affinity",
            "level",
            "effect_ids",
            "bleed",
            "frost",
            "poison",
            "scarlet_rot",
            "sleep",
            "madness",
            "death",
            *STATUS_CORRECTION_COLUMNS.values(),
        ],
        weapon_passive_overlays,
    )
    if exact_aow_compat:
        write_csv(
            output_dir / "aow_weapon_compat.csv",
            ["aow_id", "aow_name", "weapon_id", "weapon_name", "affinity", "weapon_type_name", "weapon_type_keys"],
            exact_aow_compat,
        )
    print(f"Wrote {len(weapon_passives)} weapon passive rows")
    print(f"Wrote {len(weapon_passive_overlays)} weapon passive overlay rows")
    if exact_aow_compat:
        print(f"Wrote {len(exact_aow_compat)} exact AoW compatibility rows")


def main() -> int:
    args = parse_args()
    workdir = args.workdir
    phase1_dir = args.phase1
    output_dir = args.output

    weapon_xml = workdir / "EquipParamWeapon.param.xml"
    gem_xml = workdir / "EquipParamGem.param.xml"
    sp_effect_xml = workdir / "SpEffectParam.param.xml"
    if not sp_effect_xml.exists():
        raise FileNotFoundError(f"Missing required regulation export: {sp_effect_xml}")

    weapon_csv_rows = read_csv(phase1_dir / "weapons.csv")
    reinforce_csv_rows = read_csv(phase1_dir / "reinforce.csv")
    weapon_param_rows = (
        {to_int(row, "id"): row for row in iter_param_rows(weapon_xml)} if weapon_xml.exists() else {}
    )
    reinforce_param_rows = (
        {to_int(row, "id"): row for row in iter_param_rows(workdir / "ReinforceParamWeapon.param.xml")}
        if (workdir / "ReinforceParamWeapon.param.xml").exists()
        else {}
    )
    gem_rows = list(iter_param_rows(gem_xml)) if gem_xml.exists() else []
    sp_effect_rows = {to_int(row, "id"): row for row in iter_param_rows(sp_effect_xml)}

    export_regulation_extras(
        weapon_csv_rows=weapon_csv_rows,
        reinforce_csv_rows=reinforce_csv_rows,
        weapon_param_rows=weapon_param_rows,
        reinforce_param_rows=reinforce_param_rows,
        gem_rows=gem_rows,
        sp_effect_rows=sp_effect_rows,
        output_dir=output_dir,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
