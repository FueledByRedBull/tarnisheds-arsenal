from __future__ import annotations

from .models import ValidationIssue

def validate_aow_effect_graph(
    aows: list[dict[str, str]],
    attack_rows: list[dict[str, str]],
    effects: list[dict[str, str]],
    coverage: list[dict[str, str]],
    exclusions: list[dict[str, str]],
) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    required_effect_columns = {
        "record_id",
        "aow_id",
        "sheet_row",
        "source_kind",
        "source_param_ids",
        "effect_id",
        "parent_effect_id",
        "link_kind",
        "role",
        "activation_action_id",
        "activation_timing",
        "is_canonical",
        "is_supported",
        "reason",
        "physical_attack_power",
        "magic_attack_power",
        "fire_attack_power",
        "lightning_attack_power",
        "holy_attack_power",
        "bleed_buildup",
        "frost_buildup",
        "poison_buildup",
        "scarlet_rot_buildup",
        "sleep_buildup",
        "madness_buildup",
        "death_buildup",
        "uses_status_correction",
        "uses_attack_correction",
    }
    if not effects:
        return [ValidationIssue("error", "aow_effect_data.csv is empty")]
    missing_columns = sorted(required_effect_columns.difference(effects[0]))
    if missing_columns:
        issues.append(
            ValidationIssue(
                "error",
                f"aow_effect_data.csv is missing columns: {', '.join(missing_columns)}",
            )
        )
        return issues

    aow_ids = {int(row["aow_id"]) for row in aows}
    attack_keys = {
        (int(row["aow_id"]), int(row["sheet_row"])) for row in attack_rows
    }
    record_ids = [int(row["record_id"]) for row in effects]
    if len(record_ids) != len(set(record_ids)):
        issues.append(ValidationIssue("error", "aow_effect_data.csv has duplicate record IDs"))
    invalid_references = [
        row
        for row in effects
        if (
            int(row["sheet_row"]) == 0
            and int(row["aow_id"]) not in aow_ids
        )
        or (
            int(row["sheet_row"]) != 0
            and (int(row["aow_id"]), int(row["sheet_row"])) not in attack_keys
        )
    ]
    if invalid_references:
        row = invalid_references[0]
        issues.append(
            ValidationIssue(
                "error",
                "AoW effect references an unknown skill/hit: "
                f"aow_id={row['aow_id']} sheet_row={row['sheet_row']}",
            )
        )

    attack_coverage_keys = {
        (int(row["aow_id"]), int(row["sheet_row"]), int(row["atk_id"]))
        for row in attack_rows
    }
    coverage_keys = {
        (int(row["aow_id"]), int(row["sheet_row"]), int(row["atk_id"]))
        for row in coverage
    }
    if attack_coverage_keys != coverage_keys:
        issues.append(
            ValidationIssue(
                "error",
                "aow_effect_coverage.csv does not cover every unique attack row exactly",
            )
        )

    unsupported_keys = {
        (
            int(row["aow_id"]),
            int(row["sheet_row"]),
            int(row["effect_id"]),
            row["role"],
            row["reason"],
        )
        for row in effects
        if row["is_supported"] == "0"
    }
    exclusion_keys = {
        (
            int(row["aow_id"]),
            int(row["sheet_row"]),
            int(row["effect_id"]),
            row["role"],
            row["reason"],
        )
        for row in exclusions
    }
    if unsupported_keys != exclusion_keys:
        issues.append(
            ValidationIssue(
                "error",
                "aow_effect_exclusions.csv must exactly match unsupported effect records",
            )
        )

    def find_effect(aow_id: int, sheet_row: int, effect_id: int) -> dict[str, str] | None:
        return next(
            (
                row
                for row in effects
                if int(row["aow_id"]) == aow_id
                and int(row["sheet_row"]) == sheet_row
                and int(row["effect_id"]) == effect_id
            ),
            None,
        )

    known_effects = (
        (227, 1485, 881, "frost_buildup", 60.0, "per_hit_status"),
        (228, 1440, 883, "poison_buildup", 60.0, "per_hit_status"),
        (501, 1498, 1800, "frost_buildup", 70.0, "per_hit_status"),
        (501, 1499, 1801, "frost_buildup", 110.0, "per_hit_status"),
        (4220, 1491, 20001091, "frost_buildup", 20.0, "per_hit_status"),
        (4220, 1494, 20001092, "frost_buildup", 80.0, "per_hit_status"),
    )
    for aow_id, sheet_row, effect_id, field, expected, role in known_effects:
        effect = find_effect(aow_id, sheet_row, effect_id)
        if effect is None or effect["role"] != role or float(effect[field]) != expected:
            issues.append(
                ValidationIssue(
                    "error",
                    "known AoW effect mapping is wrong: "
                    f"aow_id={aow_id} sheet_row={sheet_row} effect_id={effect_id}",
                )
            )

    poison_moth = find_effect(119, 1442, 1622)
    if (
        poison_moth is None
        or poison_moth["role"] != "replacement_or_chained"
        or poison_moth["is_supported"] != "0"
        or float(poison_moth["poison_buildup"]) != 250.0
    ):
        issues.append(
            ValidationIssue(
                "error",
                "Poison Moth Flight replacement effect must remain explicit and unsupported",
            )
        )

    canonical = [
        row
        for row in effects
        if row["sheet_row"] == "0" and row["is_canonical"] == "1"
    ]
    expected_persistent = {
        (201, "persistent_weapon_buff"),
        (214, "persistent_weapon_buff"),
        (217, "persistent_weapon_buff"),
        (227, "persistent_weapon_buff"),
        (227, "persistent_on_hit"),
        (228, "persistent_weapon_buff"),
        (228, "persistent_on_hit"),
        (606, "persistent_weapon_buff"),
        (606, "persistent_on_hit"),
        (4140, "persistent_weapon_buff"),
        (4170, "persistent_weapon_buff"),
    }
    canonical_roles = {(int(row["aow_id"]), row["role"]) for row in canonical}
    if not expected_persistent.issubset(canonical_roles):
        issues.append(
            ValidationIssue("error", "canonical persistent AoW effect mappings are incomplete")
        )
    seppuku_attack = next(
        (
            row
            for row in canonical
            if row["aow_id"] == "606" and row["role"] == "persistent_weapon_buff"
        ),
        None,
    )
    seppuku_bleed = next(
        (
            row
            for row in canonical
            if row["aow_id"] == "606" and row["role"] == "persistent_on_hit"
        ),
        None,
    )
    if (
        seppuku_attack is None
        or float(seppuku_attack["physical_attack_power"]) != 30.0
        or seppuku_bleed is None
        or float(seppuku_bleed["bleed_buildup"]) != 30.0
        or seppuku_bleed["uses_status_correction"] != "1"
    ):
        issues.append(ValidationIssue("error", "Seppuku persistent effect mapping is wrong"))
    return issues
