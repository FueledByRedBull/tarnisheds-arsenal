from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.phase1_dump import (  # noqa: E402
    STATUS_ATTACK_POWER_FIELDS as STATUS_FIELDS,
    object_to_int,
    to_float,
    to_int,
    write_csv,
)

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

STATUS_CORRECTION_COLUMNS = {
    status_key: f"{status_key}_uses_status_correction" for status_key in STATUS_FIELDS
}
KNOWN_ABSENT_PASSIVE_EFFECTS = {
    5_180_500: (
        "Referenced by Nightrider Glaive, but absent from the source SpEffectParam; "
        "there is no status payload to extract."
    ),
    5_220_300: (
        "Referenced by Raptor Talons, but absent from the source SpEffectParam; "
        "there is no status payload to extract."
    ),
    5_245_100: (
        "Referenced by Lamenting Visage, but absent from the source SpEffectParam; "
        "there is no status payload to extract."
    ),
}


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


def build_passive_effect_coverage(
    sp_effect_rows: dict[int, dict[str, str]],
    weapon_csv_rows: list[dict[str, str]],
    weapon_effect_ids: dict[int, list[int]],
) -> list[dict[str, object]]:
    weapons_by_id = {int(row["weapon_id"]): row for row in weapon_csv_rows}
    references: dict[int, list[dict[str, str]]] = {}
    for weapon_id, effect_ids in weapon_effect_ids.items():
        weapon = weapons_by_id.get(weapon_id)
        if weapon is None:
            continue
        for effect_id in effect_ids:
            references.setdefault(effect_id, []).append(weapon)

    unexpected_missing = sorted(
        effect_id
        for effect_id in references
        if effect_id not in sp_effect_rows and effect_id not in KNOWN_ABSENT_PASSIVE_EFFECTS
    )
    if unexpected_missing:
        raise ValueError(
            "weapon passives reference missing, unclassified SpEffect IDs: "
            + ", ".join(str(effect_id) for effect_id in unexpected_missing)
        )

    rows_out: list[dict[str, object]] = []
    for effect_id, weapons in sorted(references.items()):
        present = effect_id in sp_effect_rows
        rows_out.append(
            {
                "effect_id": effect_id,
                "status": "resolved" if present else "excluded_missing_source",
                "reason": "" if present else KNOWN_ABSENT_PASSIVE_EFFECTS[effect_id],
                "reference_count": len(weapons),
                "weapon_ids": "|".join(sorted({row["weapon_id"] for row in weapons})),
                "weapon_names": "|".join(sorted({row["name"] for row in weapons})),
            }
        )
    return rows_out


def build_weapon_passives(
    sp_effect_rows: dict[int, dict[str, str]],
    weapon_csv_rows: list[dict[str, str]],
    weapon_effect_ids: dict[int, list[int]],
) -> list[dict[str, object]]:
    by_id = {int(row["weapon_id"]): row for row in weapon_csv_rows}
    rows_out: list[dict[str, object]] = []
    for weapon_id, csv_row in sorted(by_id.items()):
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
            correction_value = _optional_bool(effect, "isUseStatusAilmentAtkPowerCorrect")
            for status_key, field_name in STATUS_FIELDS.items():
                amount = to_float(effect, field_name, 0.0)
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
    rows_out.sort(
        key=lambda row: (str(row["name"]), str(row["affinity"]), object_to_int(row["weapon_id"]))
    )
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
                correction_value = _optional_bool(effect, "isUseStatusAilmentAtkPowerCorrect")
                for status_key, field_name in STATUS_FIELDS.items():
                    amount = to_float(effect, field_name, 0.0)
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
    rows_out.sort(
        key=lambda row: (
            str(row["name"]),
            str(row["affinity"]),
            object_to_int(row["weapon_id"]),
            object_to_int(row["level"]),
        )
    )
    return rows_out


def _fmt(value: float) -> str:
    text = f"{value:.2f}".rstrip("0").rstrip(".")
    return text if text else "0"


def _optional_bool(row: dict[str, str], key: str) -> bool | None:
    return None if not row.get(key, "").strip() else to_int(row, key, None) != 0

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
    sp_effect_rows: dict[int, dict[str, str]],
    output_dir: Path,
) -> None:
    max_level_by_type: dict[int, int] = {}
    for row in reinforce_csv_rows:
        reinforce_type = int(row["reinforce_type"])
        level = int(row["level"])
        max_level_by_type[reinforce_type] = max(max_level_by_type.get(reinforce_type, 0), level)

    weapon_effect_ids = weapon_effect_ids_from_param_rows(weapon_param_rows, weapon_csv_rows)
    passive_effect_coverage = build_passive_effect_coverage(
        sp_effect_rows,
        weapon_csv_rows,
        weapon_effect_ids,
    )

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

    write_csv(
        output_dir / "passive_effect_coverage.csv",
        [
            "effect_id",
            "status",
            "reason",
            "reference_count",
            "weapon_ids",
            "weapon_names",
        ],
        passive_effect_coverage,
    )
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
    print(f"Wrote {len(weapon_passives)} weapon passive rows")
    print(f"Wrote {len(weapon_passive_overlays)} weapon passive overlay rows")

