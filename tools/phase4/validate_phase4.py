from __future__ import annotations

import csv
import importlib.util
import math
import os
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


@dataclass(frozen=True)
class ValidationIssue:
    level: str
    message: str


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def load_app_module(project_root: Path):
    module_path = project_root / "ui" / "desktop" / "app.py"
    spec = importlib.util.spec_from_file_location("er_optimizer_ui", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load app module spec from {module_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def max_reinforce_levels(rows: Iterable[dict[str, str]]) -> dict[int, int]:
    out: dict[int, int] = {}
    for row in rows:
        reinforce_type = int(row["reinforce_type"])
        level = int(row["level"])
        out[reinforce_type] = max(out.get(reinforce_type, -1), level)
    return out


def validate_data_snapshot(data_dir: Path) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []

    weapons = read_csv(data_dir / "weapons.csv")
    reinforce = read_csv(data_dir / "reinforce.csv")
    calc_correct = read_csv(data_dir / "calc_correct.csv")
    aows = read_csv(data_dir / "aow.csv")
    aow_attack_data = read_csv(data_dir / "aow_attack_data.csv")
    aow_buffs = read_csv(data_dir / "aow_buffs.csv")
    aow_damage_coverage = read_csv(data_dir / "aow_damage_coverage.csv")
    native_skill_attack_data = read_csv(data_dir / "native_skill_attack_data.csv")
    native_skill_damage_coverage = read_csv(data_dir / "native_skill_damage_coverage.csv")
    attack_element_correct_ext = read_csv(data_dir / "attack_element_correct_ext.csv")
    weapon_passives = read_csv(data_dir / "weapon_passives.csv")
    weapon_passive_overlays = read_csv(data_dir / "weapon_passive_overlays.csv")
    aow_weapon_compat = read_csv(data_dir / "aow_weapon_compat.csv")
    aow_affinity_compat = read_csv(data_dir / "aow_affinity_compat.csv")

    if len(weapons) < 3000:
        issues.append(ValidationIssue("error", f"weapons.csv row count too low: {len(weapons)}"))
    if len(reinforce) < 800:
        issues.append(ValidationIssue("error", f"reinforce.csv row count too low: {len(reinforce)}"))
    if len(calc_correct) < 7000:
        issues.append(
            ValidationIssue("error", f"calc_correct.csv row count too low: {len(calc_correct)}")
        )
    if len(aows) < 100:
        issues.append(ValidationIssue("error", f"aow.csv row count too low: {len(aows)}"))
    if len(aow_attack_data) < 1000:
        issues.append(
            ValidationIssue("error", f"aow_attack_data.csv row count too low: {len(aow_attack_data)}")
        )
    if len(native_skill_attack_data) < 1000:
        issues.append(
            ValidationIssue(
                "error",
                f"native_skill_attack_data.csv row count too low: {len(native_skill_attack_data)}",
            )
        )
    if len(native_skill_damage_coverage) < 1000:
        issues.append(
            ValidationIssue(
                "error",
                (
                    "native_skill_damage_coverage.csv row count too low: "
                    f"{len(native_skill_damage_coverage)}"
                ),
            )
        )
    if len(weapon_passive_overlays) < 1000:
        issues.append(
            ValidationIssue(
                "error",
                (
                    "weapon_passive_overlays.csv row count too low: "
                    f"{len(weapon_passive_overlays)}"
                ),
            )
        )
    ashable_rows_with_native_skill = [
        row
        for row in weapons
        if row.get("disable_gem_attr", "1") == "0"
        and (row.get("native_skill_id", "").strip() or row.get("native_skill_name", "").strip())
    ]
    if ashable_rows_with_native_skill:
        sample = ashable_rows_with_native_skill[0]
        issues.append(
            ValidationIssue(
                "error",
                (
                    "ashable weapons must not carry native_skill metadata; "
                    f"found {sample.get('name', '<unknown>')} | {sample.get('affinity', '<unknown>')}"
                ),
            )
        )
    if len(aow_buffs) < 8:
        issues.append(
            ValidationIssue("error", f"aow_buffs.csv row count too low: {len(aow_buffs)}")
        )
    if aow_buffs:
        required_aow_buff_columns = {
            "bleed_uses_status_correction",
            "frost_uses_status_correction",
            "poison_uses_status_correction",
            "scarlet_rot_uses_status_correction",
            "sleep_uses_status_correction",
            "madness_uses_status_correction",
            "death_uses_status_correction",
        }
        missing_aow_buff_columns = sorted(required_aow_buff_columns.difference(aow_buffs[0].keys()))
        if missing_aow_buff_columns:
            issues.append(
                ValidationIssue(
                    "error",
                    f"aow_buffs.csv is missing columns: {', '.join(missing_aow_buff_columns)}",
                )
            )
    if len(aow_damage_coverage) != len(aows):
        issues.append(
            ValidationIssue(
                "error",
                (
                    "aow_damage_coverage.csv should align 1:1 with aow.csv "
                    f"({len(aow_damage_coverage)} vs {len(aows)})"
                ),
            )
        )
    if len(attack_element_correct_ext) < 150:
        issues.append(
            ValidationIssue(
                "error",
                f"attack_element_correct_ext.csv row count too low: {len(attack_element_correct_ext)}",
            )
        )
    if len(weapon_passives) != len(weapons):
        issues.append(
            ValidationIssue(
                "error",
                (
                    "weapon_passives.csv should align 1:1 with weapons.csv "
                    f"({len(weapon_passives)} vs {len(weapons)})"
                ),
            )
        )
    if len(weapon_passive_overlays) < 1000:
        issues.append(
            ValidationIssue(
                "error",
                f"weapon_passive_overlays.csv row count too low: {len(weapon_passive_overlays)}",
            )
        )
    if weapons:
        required_weapon_columns = {"disable_two_hand_bonus", "weapon_type_keys"}
        missing_weapon_columns = sorted(required_weapon_columns.difference(weapons[0].keys()))
        if missing_weapon_columns:
            issues.append(
                ValidationIssue(
                    "error",
                    f"weapons.csv is missing columns: {', '.join(missing_weapon_columns)}",
                )
            )
    required_passive_columns = {
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
        "bleed_uses_status_correction",
        "frost_uses_status_correction",
        "poison_uses_status_correction",
        "scarlet_rot_uses_status_correction",
        "sleep_uses_status_correction",
        "madness_uses_status_correction",
        "death_uses_status_correction",
    }
    if weapon_passives:
        missing_columns = sorted(required_passive_columns.difference(weapon_passives[0].keys()))
        if missing_columns:
            issues.append(
                ValidationIssue(
                    "error",
                    f"weapon_passives.csv is missing columns: {', '.join(missing_columns)}",
                )
            )
    if len(aow_weapon_compat) < 40000:
        issues.append(
            ValidationIssue(
                "error",
                f"aow_weapon_compat.csv row count too low: {len(aow_weapon_compat)}",
            )
        )

    aow_by_id = {int(row["aow_id"]): row for row in aows}
    weapon_by_id = {int(row["weapon_id"]): row for row in weapons}
    attack_element_ext_ids = {
        int(row["attack_element_correct_id"])
        for row in attack_element_correct_ext
    }
    for file_name, rows in (
        ("aow_attack_data.csv", aow_attack_data),
        ("native_skill_attack_data.csv", native_skill_attack_data),
    ):
        for row in rows:
            override_id = int(row.get("overwrite_attack_element_correct_id") or 0)
            if override_id > 0 and override_id not in attack_element_ext_ids:
                issues.append(
                    ValidationIssue(
                        "error",
                        (
                            f"{file_name} row {row.get('sheet_row')} references missing "
                            f"attack_element_correct_ext id {override_id}"
                        ),
                    )
                )
                break

    for row in aow_weapon_compat:
        aow_id = int(row["aow_id"])
        weapon_id = int(row["weapon_id"])
        canonical = aow_by_id.get(aow_id)
        if canonical is None:
            issues.append(ValidationIssue("error", f"aow_weapon_compat.csv references missing aow_id={aow_id}"))
            break
        if not row["aow_name"].strip() or row["aow_name"] != canonical["name"]:
            issues.append(ValidationIssue("error", f"aow_weapon_compat.csv has stale/placeholder name for aow_id={aow_id}"))
            break
        if weapon_id not in weapon_by_id:
            issues.append(ValidationIssue("error", f"aow_weapon_compat.csv references missing weapon_id={weapon_id}"))
            break

    for row in aow_affinity_compat:
        aow_id = int(row["aow_id"])
        canonical = aow_by_id.get(aow_id)
        if canonical is None:
            issues.append(ValidationIssue("error", f"aow_affinity_compat.csv references missing aow_id={aow_id}"))
            break
        if not row["name"].strip() or row["name"] != canonical["name"]:
            issues.append(ValidationIssue("error", f"aow_affinity_compat.csv has stale/placeholder name for aow_id={aow_id}"))
            break

    native_statuses = {row["status"] for row in native_skill_damage_coverage}
    unexpected_native_statuses = sorted(native_statuses.difference({"matched", "generic_aow", "unmatched_weapon"}))
    if unexpected_native_statuses:
        issues.append(
            ValidationIssue(
                "error",
                f"native_skill_damage_coverage.csv has unexpected statuses: {', '.join(unexpected_native_statuses)}",
            )
        )
    if not any(row["match_source"] == "weapon_name_fallback" for row in native_skill_damage_coverage):
        issues.append(
            ValidationIssue(
                "error",
                "native_skill_damage_coverage.csv did not exercise weapon_name_fallback extraction",
            )
        )
    if any(row["status"] == "ambiguous_weapon_name_fallback" for row in native_skill_damage_coverage):
        issues.append(
            ValidationIssue(
                "error",
                "native_skill_damage_coverage.csv has ambiguous weapon-name fallback rows",
            )
        )

    reinforce_max = max_reinforce_levels(reinforce)
    used_types: dict[int, list[int]] = defaultdict(list)
    zero_base_count = 0
    used_curve_ids: set[int] = set()
    for row in weapons:
        reinforce_type = int(row["reinforce_type"])
        used_types[reinforce_type].append(int(row["weapon_id"]))
        base_total = sum(
            float(row[field])
            for field in (
                "base_physical",
                "base_magic",
                "base_fire",
                "base_lightning",
                "base_holy",
            )
        )
        if base_total == 0.0:
            zero_base_count += 1
        used_curve_ids.update(
            {
                int(row["curve_id_physical"]),
                int(row["curve_id_magic"]),
                int(row["curve_id_fire"]),
                int(row["curve_id_lightning"]),
                int(row["curve_id_holy"]),
            }
        )

    if zero_base_count != 0:
        issues.append(ValidationIssue("error", f"weapons with zero base damage: {zero_base_count}"))

    missing_type_keys = [
        row["weapon_id"]
        for row in weapons
        if row.get("disable_gem_attr", "1") == "0" and not row.get("weapon_type_keys", "").strip()
    ]
    if missing_type_keys:
        issues.append(
            ValidationIssue(
                "error",
                f"ashable weapons missing weapon_type_keys: {missing_type_keys[:10]}",
            )
        )

    antspur_standard = next(
        (row for row in weapon_passives if row["name"] == "Antspur Rapier" and row["affinity"] == "Standard"),
        None,
    )
    if antspur_standard is None:
        issues.append(ValidationIssue("error", "Antspur Rapier Standard missing from weapon_passives.csv"))
    else:
        poison = float(antspur_standard["poison"])
        scarlet_rot = float(antspur_standard.get("scarlet_rot", "0") or 0.0)
        if poison != 0.0 or scarlet_rot <= 0.0:
            issues.append(
                ValidationIssue(
                    "error",
                    (
                        "Antspur Rapier Standard passive split is wrong: "
                        f"poison={poison} scarlet_rot={scarlet_rot}"
                    ),
                )
            )

    great_katana_blood_25 = next(
        (
            row
            for row in weapon_passive_overlays
            if row["name"] == "Great Katana" and row["affinity"] == "Blood" and row["level"] == "25"
        ),
        None,
    )
    if great_katana_blood_25 is None:
        issues.append(
            ValidationIssue("error", "Great Katana Blood +25 missing from weapon_passive_overlays.csv")
        )
    elif float(great_katana_blood_25["bleed"]) < 100.0:
        issues.append(
            ValidationIssue(
                "error",
                f"Great Katana Blood +25 overlay bleed is too low: {great_katana_blood_25['bleed']}",
            )
        )

    special_non_upgrade_types = {3000}
    for reinforce_type, weapon_ids in sorted(used_types.items()):
        max_level = reinforce_max.get(reinforce_type, -1)
        if max_level < 0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"missing reinforce_type={reinforce_type} referenced by {len(weapon_ids)} weapons",
                )
            )
            continue
        is_somber = any(int(row["is_somber"]) == 1 and int(row["reinforce_type"]) == reinforce_type for row in weapons)
        if reinforce_type in special_non_upgrade_types:
            continue
        if is_somber and max_level != 10:
            issues.append(
                ValidationIssue(
                    "error",
                    f"somber reinforce_type={reinforce_type} has max_level={max_level}, expected 10",
                )
            )
        if not is_somber and max_level < 25:
            issues.append(
                ValidationIssue(
                    "error",
                    f"standard reinforce_type={reinforce_type} has max_level={max_level}, expected >=25",
                )
            )

    curves: dict[int, dict[int, float]] = defaultdict(dict)
    for row in calc_correct:
        curves[int(row["curve_id"])][int(row["stat_value"])] = float(row["multiplier"])

    non_mono_used: list[int] = []
    for curve_id in sorted(used_curve_ids):
        series = [curves[curve_id].get(x, 0.0) for x in range(1, 100)]
        if any(series[i] > series[i + 1] + 1e-9 for i in range(98)):
            non_mono_used.append(curve_id)
    if non_mono_used:
        issues.append(
            ValidationIssue(
                "error",
                f"non-monotonic used curves detected: {non_mono_used[:10]}",
            )
        )

    lions_claw = next((row for row in aows if row["name"] == "Lion's Claw"), None)
    if lions_claw is None:
        issues.append(ValidationIssue("error", "Lion's Claw not found in aow.csv"))
    else:
        bleed = float(lions_claw["bleed_buildup_add"])
        if bleed != 0.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"Lion's Claw bleed_buildup_add is {bleed}, expected 0",
                )
            )

    seppuku_buff = next((row for row in aow_buffs if row["name"] == "Seppuku"), None)
    if seppuku_buff is None:
        issues.append(ValidationIssue("error", "Seppuku missing from aow_buffs.csv"))
    else:
        if float(seppuku_buff["physical_attack_power_add"]) < 30.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"Seppuku physical attack buff is wrong: {seppuku_buff['physical_attack_power_add']}",
                )
            )
        if float(seppuku_buff["scaling_bleed_buildup_add"]) < 30.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"Seppuku bleed buff is wrong: {seppuku_buff['scaling_bleed_buildup_add']}",
                )
            )
        if seppuku_buff.get("bleed_uses_status_correction") != "1":
            issues.append(
                ValidationIssue(
                    "error",
                    "Seppuku should mark bleed_uses_status_correction=1",
                )
            )

    star_fist_occult = next(
        (row for row in weapon_passives if row["name"] == "Star Fist" and row["affinity"] == "Occult"),
        None,
    )
    if star_fist_occult is None:
        issues.append(ValidationIssue("error", "Star Fist Occult missing from weapon_passives.csv"))
    elif star_fist_occult.get("bleed_uses_status_correction") not in {"", "1"}:
        issues.append(
            ValidationIssue(
                "error",
                "Star Fist Occult bleed_uses_status_correction should be blank or 1",
            )
        )

    coverage_by_name = {row["aow_name"]: row for row in aow_damage_coverage}
    for name, expected_status in (
        ("Glintstone Pebble", "direct_damage"),
        ("Carian Retaliation", "direct_damage"),
        ("Parry", "missing"),
        ("Bloodhound's Step", "missing"),
    ):
        row = coverage_by_name.get(name)
        if row is None:
            issues.append(ValidationIssue("error", f"{name} missing from aow_damage_coverage.csv"))
            continue
        if row["status"] != expected_status:
            issues.append(
                ValidationIssue(
                    "error",
                    f"{name} coverage status is {row['status']}, expected {expected_status}",
                )
            )
    impaling = coverage_by_name.get("Impaling Thrust")
    if impaling is None or int(impaling["unique_collision_rows"]) == 0:
        issues.append(
            ValidationIssue(
                "error",
                "Impaling Thrust should report unique_skill_collision_rows > 0",
            )
        )
    retaliation = coverage_by_name.get("Carian Retaliation")
    if retaliation is None or int(retaliation["parry_rows"]) == 0 or int(retaliation["bullet_rows"]) == 0:
        issues.append(
            ValidationIssue(
                "error",
                "Carian Retaliation should expose both parry and bullet rows in coverage",
            )
        )

    weapon_rows_by_name = defaultdict(list)
    for row in weapons:
        weapon_rows_by_name[row["name"]].append(row)
    for weapon_name in ("Iron Ball", "Starscourge Greatsword", "Rellana's Twin Blades"):
        rows = weapon_rows_by_name.get(weapon_name, [])
        if not rows or not all(row.get("disable_two_hand_bonus") == "1" for row in rows):
            issues.append(
                ValidationIssue(
                    "error",
                    f"{weapon_name} should disable the two-hand strength bonus",
                )
            )

    halo_rows = [row for row in native_skill_attack_data if row["weapon_name"] == "Halo Scythe"]
    if not halo_rows:
        issues.append(ValidationIssue("error", "Halo Scythe missing from native_skill_attack_data.csv"))

    return issues


def validate_runtime_ar(data_dir: Path) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    try:
        import er_optimizer_core as core
    except Exception as exc:
        issues.append(ValidationIssue("error", f"runtime AR checks skipped: {exc}"))
        return issues

    data = core.load_game_data(str(data_dir))

    # Exact AR checks are covered by Rust tests.
    # This validates Python binding runtime behavior.
    cases = [
        {
            "class_name": "Samurai",
            "character_level": 80,
            "vig": 20,
            "mnd": 15,
            "end": 15,
            "str_stat": 18,
            "dex": 35,
            "int_stat": 9,
            "fai": 8,
            "arc": 16,
            "weapon_name": "Uchigatana",
            "affinity": "Keen",
            "max_upgrade": 25,
        },
        {
            "class_name": "Vagabond",
            "character_level": 120,
            "vig": 40,
            "mnd": 10,
            "end": 30,
            "str_stat": 40,
            "dex": 40,
            "int_stat": 9,
            "fai": 9,
            "arc": 7,
            "weapon_name": "Lordsworn's Greatsword",
            "affinity": "Quality",
            "max_upgrade": 25,
        },
    ]

    for case in cases:
        class_base = {
            "Vagabond": (9, 88),
            "Warrior": (8, 87),
            "Hero": (7, 86),
            "Bandit": (5, 84),
            "Astrologer": (6, 85),
            "Prophet": (7, 86),
            "Samurai": (9, 88),
            "Prisoner": (9, 88),
            "Confessor": (10, 89),
            "Wretch": (1, 80),
        }
        base_level, base_total = class_base[case["class_name"]]
        current_sum = (
            case["vig"]
            + case["mnd"]
            + case["end"]
            + case["str_stat"]
            + case["dex"]
            + case["int_stat"]
            + case["fai"]
            + case["arc"]
        )
        exact_level = base_level + (current_sum - base_total)

        rows = core.optimize_builds(
            data=data,
            class_name=case["class_name"],
            character_level=exact_level,
            vig=case["vig"],
            mnd=case["mnd"],
            end=case["end"],
            str_stat=case["str_stat"],
            dex=case["dex"],
            int_stat=case["int_stat"],
            fai=case["fai"],
            arc=case["arc"],
            max_upgrade=case["max_upgrade"],
            fixed_upgrade=case["max_upgrade"],
            weapon_name=case["weapon_name"],
            affinity=case["affinity"],
            objective="max_ar",
            top_k=1,
            lock_str=case["str_stat"],
            lock_dex=case["dex"],
            lock_int=case["int_stat"],
            lock_fai=case["fai"],
            lock_arc=case["arc"],
            min_str=0,
            min_dex=0,
            min_int=0,
            min_fai=0,
            min_arc=0,
            somber_filter="all",
            weapon_type_key=None,
        )
        if not rows:
            issues.append(
                ValidationIssue(
                    "error",
                    (
                        "runtime optimize returned no rows for "
                        f"{case['weapon_name']} {case['affinity']} +{case['max_upgrade']}"
                    ),
                )
            )
            continue
        actual = float(rows[0].ar_total)
        if not math.isfinite(actual) or actual <= 0.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"runtime optimize produced invalid AR: {actual}",
                )
            )

    bleed_rows = core.optimize_builds(
        data=data,
        class_name="Samurai",
        character_level=61,
        vig=40,
        mnd=11,
        end=20,
        str_stat=12,
        dex=20,
        int_stat=9,
        fai=8,
        arc=20,
        max_upgrade=10,
        fixed_upgrade=10,
        weapon_name="Rivers of Blood",
        affinity="Standard",
        aow_name=None,
        objective="max_ar",
        top_k=1,
        somber_filter="all",
        weapon_type_key=None,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        lock_str=None,
        lock_dex=None,
        lock_int=None,
        lock_fai=None,
        lock_arc=None,
    )
    if not bleed_rows:
        issues.append(ValidationIssue("error", "runtime bleed case returned no rows"))
    else:
        bleed_value = float(bleed_rows[0].bleed_buildup)
        if bleed_value < 50.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"runtime bleed case ignored innate weapon bleed: {bleed_value}",
                )
            )

    open_max_ar_rows = core.optimize_builds(
        data=data,
        class_name="Samurai",
        character_level=46,
        vig=12,
        mnd=11,
        end=13,
        str_stat=12,
        dex=15,
        int_stat=9,
        fai=8,
        arc=45,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Uchigatana",
        affinity="Blood",
        aow_name=None,
        objective="max_ar",
        top_k=1,
        somber_filter="all",
        weapon_type_key=None,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        lock_str=12,
        lock_dex=15,
        lock_int=9,
        lock_fai=8,
        lock_arc=45,
    )
    locked_seppuku_rows = core.optimize_builds(
        data=data,
        class_name="Samurai",
        character_level=46,
        vig=12,
        mnd=11,
        end=13,
        str_stat=12,
        dex=15,
        int_stat=9,
        fai=8,
        arc=45,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Uchigatana",
        affinity="Blood",
        aow_name="Seppuku",
        objective="max_ar",
        top_k=1,
        somber_filter="all",
        weapon_type_key=None,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        lock_str=12,
        lock_dex=15,
        lock_int=9,
        lock_fai=8,
        lock_arc=45,
    )
    if not open_max_ar_rows or not locked_seppuku_rows:
        issues.append(ValidationIssue("error", "runtime open Max AR AoW regression case returned no rows"))
    else:
        if open_max_ar_rows[0].aow_name != "Seppuku":
            issues.append(
                ValidationIssue(
                    "error",
                    f"runtime open Max AR did not pick Seppuku for Blood Uchigatana: {open_max_ar_rows[0].aow_name}",
                )
            )
        if not math.isclose(
            float(open_max_ar_rows[0].score),
            float(locked_seppuku_rows[0].score),
            rel_tol=1e-9,
            abs_tol=1e-6,
        ):
            issues.append(
                ValidationIssue(
                    "error",
                    "runtime open Max AR no longer matches the best explicit buff AoW result",
                )
            )

    open_bleed_aow_rows = core.optimize_builds(
        data=data,
        class_name="Samurai",
        character_level=112,
        vig=40,
        mnd=11,
        end=20,
        str_stat=12,
        dex=15,
        int_stat=9,
        fai=8,
        arc=8,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Uchigatana",
        affinity="Keen",
        aow_name=None,
        objective="max_ar_plus_bleed",
        top_k=1,
        somber_filter="all",
        weapon_type_key=None,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        lock_str=18,
        lock_dex=40,
        lock_int=9,
        lock_fai=8,
        lock_arc=45,
    )
    explicit_best_bleed_score: float | None = None
    explicit_best_bleed_ar: float | None = None
    for aow_name in data.compatible_aow_names("Uchigatana", "Keen"):
        explicit_rows = core.optimize_builds(
            data=data,
            class_name="Samurai",
            character_level=112,
            vig=40,
            mnd=11,
            end=20,
            str_stat=12,
            dex=15,
            int_stat=9,
            fai=8,
            arc=8,
            max_upgrade=25,
            fixed_upgrade=25,
            weapon_name="Uchigatana",
            affinity="Keen",
            aow_name=aow_name,
            objective="max_ar_plus_bleed",
            top_k=1,
            somber_filter="all",
            weapon_type_key=None,
            min_str=0,
            min_dex=0,
            min_int=0,
            min_fai=0,
            min_arc=0,
            lock_str=18,
            lock_dex=40,
            lock_int=9,
            lock_fai=8,
            lock_arc=45,
        )
        if explicit_rows:
            score = float(explicit_rows[0].score)
            ar = float(explicit_rows[0].ar_total)
            if (
                explicit_best_bleed_score is None
                or score > explicit_best_bleed_score
                or (
                    math.isclose(score, explicit_best_bleed_score, rel_tol=1e-9, abs_tol=1e-6)
                    and ar > float(explicit_best_bleed_ar or 0.0)
                )
            ):
                explicit_best_bleed_score = score
                explicit_best_bleed_ar = ar
    if not open_bleed_aow_rows or explicit_best_bleed_score is None or explicit_best_bleed_ar is None:
        issues.append(
            ValidationIssue("error", "runtime open Max AR + Bleed AoW regression case returned no rows")
        )
    else:
        if not math.isclose(
            float(open_bleed_aow_rows[0].score),
            explicit_best_bleed_score,
            rel_tol=1e-9,
            abs_tol=1e-6,
        ):
            issues.append(
                ValidationIssue(
                    "error",
                    "runtime open Max AR + Bleed no longer matches the best explicit AoW result",
                )
            )
        if not math.isclose(
            float(open_bleed_aow_rows[0].ar_total),
            explicit_best_bleed_ar,
            rel_tol=1e-9,
            abs_tol=1e-6,
        ):
            issues.append(
                ValidationIssue(
                    "error",
                    "runtime open Max AR + Bleed no longer uses AR as the equal-bleed AoW tie-breaker",
                )
            )

    aow_rows = core.optimize_builds(
        data=data,
        class_name="Samurai",
        character_level=84,
        vig=40,
        mnd=11,
        end=20,
        str_stat=21,
        dex=15,
        int_stat=40,
        fai=8,
        arc=8,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Sword Lance",
        affinity="Magic",
        aow_name="Glintstone Pebble",
        objective="aow_first_hit",
        top_k=1,
        somber_filter="all",
        weapon_type_key=None,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
    )
    if not aow_rows:
        issues.append(ValidationIssue("error", "runtime AoW first-hit objective returned no rows"))
    else:
        if float(aow_rows[0].aow_first_hit_damage) <= 0.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"runtime AoW first-hit damage is non-positive: {aow_rows[0].aow_first_hit_damage}",
                )
            )
        if float(aow_rows[0].aow_full_sequence_damage) < float(aow_rows[0].aow_first_hit_damage):
            issues.append(
                ValidationIssue(
                    "error",
                    "runtime AoW full-sequence damage is below first-hit damage",
                )
            )

    iron_ball_one_hand = core.optimize_builds(
        data=data,
        class_name="Wretch",
        character_level=64,
        vig=10,
        mnd=10,
        end=10,
        str_stat=68,
        dex=15,
        int_stat=10,
        fai=10,
        arc=10,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Iron Ball",
        affinity="Heavy",
        objective="max_ar",
        top_k=1,
        lock_str=68,
        lock_dex=15,
        lock_int=10,
        lock_fai=10,
        lock_arc=10,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        two_handing=False,
        somber_filter="all",
        weapon_type_key=None,
    )
    iron_ball_two_hand = core.optimize_builds(
        data=data,
        class_name="Wretch",
        character_level=64,
        vig=10,
        mnd=10,
        end=10,
        str_stat=68,
        dex=15,
        int_stat=10,
        fai=10,
        arc=10,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Iron Ball",
        affinity="Heavy",
        objective="max_ar",
        top_k=1,
        lock_str=68,
        lock_dex=15,
        lock_int=10,
        lock_fai=10,
        lock_arc=10,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        two_handing=True,
        somber_filter="all",
        weapon_type_key=None,
    )
    if not iron_ball_one_hand or not iron_ball_two_hand:
        issues.append(ValidationIssue("error", "runtime Iron Ball case returned no rows"))
    else:
        one_hand = float(iron_ball_one_hand[0].ar_total)
        two_hand = float(iron_ball_two_hand[0].ar_total)
        if one_hand <= 0.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"Iron Ball 1H AR is invalid: {one_hand}",
                )
            )
        if abs(two_hand - one_hand) > 0.01:
            issues.append(
                ValidationIssue(
                    "error",
                    f"Iron Ball incorrectly gains two-hand AR: 1H={one_hand} 2H={two_hand}",
                )
            )

    star_fist_blood = core.optimize_builds(
        data=data,
        class_name="Wretch",
        character_level=61,
        vig=10,
        mnd=10,
        end=10,
        str_stat=12,
        dex=10,
        int_stat=10,
        fai=10,
        arc=68,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Star Fist",
        affinity="Blood",
        objective="max_ar",
        top_k=1,
        lock_str=12,
        lock_dex=10,
        lock_int=10,
        lock_fai=10,
        lock_arc=68,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        two_handing=False,
        somber_filter="all",
        weapon_type_key=None,
    )
    if not star_fist_blood:
        issues.append(ValidationIssue("error", "runtime Star Fist blood case returned no rows"))
    else:
        bleed = float(star_fist_blood[0].bleed_buildup)
        if bleed <= 75.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"Star Fist blood buildup did not scale at +25 ARC 68: {bleed}",
                )
            )

    antspur_occult = core.optimize_builds(
        data=data,
        class_name="Wretch",
        character_level=69,
        vig=10,
        mnd=10,
        end=10,
        str_stat=10,
        dex=20,
        int_stat=10,
        fai=10,
        arc=68,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Antspur Rapier",
        affinity="Occult",
        objective="max_ar",
        top_k=1,
        lock_str=10,
        lock_dex=20,
        lock_int=10,
        lock_fai=10,
        lock_arc=68,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        two_handing=False,
        somber_filter="all",
        weapon_type_key=None,
    )
    if not antspur_occult:
        issues.append(ValidationIssue("error", "runtime Antspur occult case returned no rows"))
    else:
        poison = float(getattr(antspur_occult[0], "poison_buildup", 0.0))
        scarlet_rot = float(getattr(antspur_occult[0], "scarlet_rot_buildup", 0.0))
        if poison > 0.0 or scarlet_rot <= 60.0:
            issues.append(
                ValidationIssue(
                    "error",
                    f"Antspur occult scarlet rot scaling is wrong: poison={poison} scarlet_rot={scarlet_rot}",
                )
            )

    base_cap_request = {
        "data": data,
        "class_name": "Samurai",
        "character_level": 9,
        "vig": 12,
        "mnd": 11,
        "end": 13,
        "str_stat": 12,
        "dex": 15,
        "int_stat": 9,
        "fai": 8,
        "arc": 8,
        "max_upgrade": 0,
        "fixed_upgrade": 0,
        "weapon_name": "Uchigatana",
        "affinity": "Keen",
        "aow_name": None,
        "objective": "max_ar",
        "top_k": 1,
        "somber_filter": "all",
        "weapon_type_key": None,
        "min_str": 0,
        "min_dex": 0,
        "min_int": 0,
        "min_fai": 0,
        "min_arc": 0,
        "lock_str": None,
        "lock_dex": None,
        "lock_int": None,
        "lock_fai": None,
        "lock_arc": None,
        "two_handing": False,
    }
    for label, expected, overrides in (
        ("current stat cap", "str must be <= 99", {"str_stat": 100}),
        ("minimum stat cap", "minimum combat stat 0 must be <= 99", {"min_str": 100}),
        ("locked stat cap", "locked combat stat 0 must be <= 99", {"lock_str": 100}),
    ):
        request = dict(base_cap_request)
        request.update(overrides)
        try:
            core.optimize_builds(**request)
        except Exception as exc:
            if expected not in str(exc):
                issues.append(
                    ValidationIssue(
                        "error",
                        f"runtime {label} surfaced wrong error: {exc}",
                    )
                )
        else:
            issues.append(ValidationIssue("error", f"runtime {label} did not reject invalid input"))

    uchi_base = core.optimize_builds(
        data=data,
        class_name="Samurai",
        character_level=46,
        vig=12,
        mnd=11,
        end=13,
        str_stat=12,
        dex=15,
        int_stat=9,
        fai=8,
        arc=45,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Uchigatana",
        affinity="Blood",
        aow_name="Double Slash",
        objective="max_ar",
        top_k=1,
        lock_str=12,
        lock_dex=15,
        lock_int=9,
        lock_fai=8,
        lock_arc=45,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        two_handing=False,
        somber_filter="all",
        weapon_type_key=None,
    )
    uchi_seppuku = core.optimize_builds(
        data=data,
        class_name="Samurai",
        character_level=46,
        vig=12,
        mnd=11,
        end=13,
        str_stat=12,
        dex=15,
        int_stat=9,
        fai=8,
        arc=45,
        max_upgrade=25,
        fixed_upgrade=25,
        weapon_name="Uchigatana",
        affinity="Blood",
        aow_name="Seppuku",
        objective="max_ar",
        top_k=1,
        lock_str=12,
        lock_dex=15,
        lock_int=9,
        lock_fai=8,
        lock_arc=45,
        min_str=0,
        min_dex=0,
        min_int=0,
        min_fai=0,
        min_arc=0,
        two_handing=False,
        somber_filter="all",
        weapon_type_key=None,
    )
    if not uchi_base or not uchi_seppuku:
        issues.append(ValidationIssue("error", "runtime Seppuku case returned no rows"))
    else:
        if float(uchi_seppuku[0].ar_total) < float(uchi_base[0].ar_total) + 29.9:
            issues.append(
                ValidationIssue(
                    "error",
                    (
                        "Seppuku AR buff was not applied: "
                        f"base={uchi_base[0].ar_total} buffed={uchi_seppuku[0].ar_total}"
                    ),
                )
            )
        if float(uchi_seppuku[0].bleed_buildup) <= float(uchi_base[0].bleed_buildup) + 30.0:
            issues.append(
                ValidationIssue(
                    "error",
                    (
                        "Seppuku bleed buff was not applied: "
                        f"base={uchi_base[0].bleed_buildup} buffed={uchi_seppuku[0].bleed_buildup}"
                    ),
                )
            )

    return issues


def validate_level_paths(project_root: Path) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    if not (project_root / "ui" / "desktop" / "app.py").exists():
        return issues
    try:
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        from PyQt6 import QtWidgets
    except Exception as exc:
        issues.append(ValidationIssue("warning", f"PyQt level path checks skipped: {exc}"))
        return issues

    try:
        app_module = load_app_module(project_root)
    except Exception as exc:
        issues.append(ValidationIssue("warning", f"PyQt level path checks skipped: {exc}"))
        return issues

    app = QtWidgets.QApplication.instance()
    created_app = app is None
    if app is None:
        app = QtWidgets.QApplication([])

    window = app_module.MainWindow()
    try:
        app_module.apply_dark_theme(app)
        if window.main_tabs.count() != 4:
            issues.append(ValidationIssue("error", "main workspace should expose 4 tabs"))
        else:
            if window.main_tabs.tabText(2) != "PATHS":
                issues.append(ValidationIssue("error", "third main tab should be PATHS"))
            if window.main_tabs.tabText(3) != "AFFINITY WATCH":
                issues.append(ValidationIssue("error", "fourth main tab should be AFFINITY WATCH"))
        window._set_combo_by_data(window.class_combo, "Samurai")
        window._on_class_changed()
        window.vig_spin.setValue(40)
        window.mnd_spin.setValue(11)
        window.end_spin.setValue(20)
        window.str_spin.setValue(12)
        window.dex_spin.setValue(15)
        window.int_spin.setValue(9)
        window.fai_spin.setValue(8)
        window.arc_spin.setValue(20)
        window.max_upgrade_spin.setValue(16)
        window.lock_upgrade_exact.setChecked(True)
        window._set_combo_by_data(window.objective_combo, "max_ar_plus_bleed")
        if "Seppuku" in window.data.compatible_aow_names_for_affinity("Cold"):
            issues.append(
                ValidationIssue(
                    "error",
                    "global affinity AoW filtering still allows Seppuku for Cold",
                )
            )
        if "Seppuku" in window.data.compatible_aow_names_for_affinity("Fire"):
            issues.append(
                ValidationIssue(
                    "error",
                    "global affinity AoW filtering still allows Seppuku for Fire",
                )
            )
        window._set_combo_by_data(window.weapon_combo, "Uchigatana")
        window._refresh_affinity_options()
        window._set_combo_by_data(window.affinity_combo, "Cold")
        window._refresh_aow_options()
        if window.aow_combo.findData("Seppuku") >= 0:
            issues.append(
                ValidationIssue(
                    "error",
                    "main AoW selector still offers Seppuku for Cold Uchigatana",
                )
            )
        window._set_combo_by_data(window.affinity_combo, "Fire")
        window._refresh_aow_options()
        if window.aow_combo.findData("Seppuku") >= 0:
            issues.append(
                ValidationIssue(
                    "error",
                    "main AoW selector still offers Seppuku for Fire Uchigatana",
                )
            )
        window._set_combo_by_data(window.compare_weapon_combo, "Uchigatana")
        window._refresh_compare_affinity_options()
        window._set_combo_by_data(window.compare_affinity_combo, "Cold")
        window._refresh_compare_aow_options()
        if window.compare_aow_combo.findData("Seppuku") >= 0:
            issues.append(
                ValidationIssue(
                    "error",
                    "compare AoW selector still offers Seppuku for Cold Uchigatana",
                )
            )
        window._set_combo_by_data(window.compare_affinity_combo, "Fire")
        window._refresh_compare_aow_options()
        if window.compare_aow_combo.findData("Seppuku") >= 0:
            issues.append(
                ValidationIssue(
                    "error",
                    "compare AoW selector still offers Seppuku for Fire Uchigatana",
                )
            )
        if "Seppuku" in window.data.compatible_aow_names("Uchigatana", "Cold"):
            issues.append(
                ValidationIssue(
                    "error",
                    "runtime AoW compatibility still allows Seppuku for Cold Uchigatana",
                )
            )
        if "Seppuku" in window.data.compatible_aow_names("Uchigatana", "Fire"):
            issues.append(
                ValidationIssue(
                    "error",
                    "runtime AoW compatibility still allows Seppuku for Fire Uchigatana",
                )
            )
        window._set_combo_by_data(window.affinity_combo, "Blood")
        window._refresh_aow_options()
        window._set_combo_by_data(window.compare_affinity_combo, "Occult")
        window._refresh_compare_aow_options()
        window._refresh_estimate()
        session = window._current_session()
        request = window.desktop_service.build_optimize_request(session)
        if request != window._build_request_kwargs(include_progress=False):
            issues.append(
                ValidationIssue(
                    "error",
                    "service build_optimize_request no longer matches the app request builder",
                )
            )

        selected = window._best_row_config("Uchigatana", "Blood", "Seppuku")
        compare = window._best_row_config("Uchigatana", "Occult", "Seppuku")
        if selected is None or compare is None:
            issues.append(
                ValidationIssue(
                    "error",
                    "failed to build deterministic level-path comparison fixtures",
                )
            )
            return issues

        session = window._current_session()
        selected_metric = selected.metric_for_objective(session.objective_id)
        if math.isclose(float(selected_metric), float(selected.ar_total), rel_tol=1e-9, abs_tol=1e-9):
            issues.append(
                ValidationIssue(
                    "error",
                    "bleed objective fixture no longer differs from raw AR; downstream metric validation lost coverage",
                )
            )
        upgrade_series = window.desktop_service.build_upgrade_series(session, selected, selected.upgrade)
        upgrade_metric = upgrade_series.get(selected.upgrade)
        if upgrade_metric is None or not math.isclose(
            float(upgrade_metric),
            float(selected_metric),
            rel_tol=1e-9,
            abs_tol=1e-6,
        ):
            issues.append(
                ValidationIssue(
                    "error",
                    "upgrade series no longer uses the selected objective metric for Max AR + Bleed",
                )
            )

        window.active_compare_selected = selected
        window.active_compare_target = compare
        levels_ahead = 5
        preview_configs = window._path_preview_configs()
        previews_first = [
            window.desktop_service.build_path_preview(session, config.solved, levels_ahead, config.title)
            for config in preview_configs
        ]
        previews_second = [
            window.desktop_service.build_path_preview(session, config.solved, levels_ahead, config.title)
            for config in preview_configs
        ]
        if len(previews_first) != 2 or len(previews_second) != 2:
            issues.append(ValidationIssue("error", "level path preview generation failed"))
            return issues

        first_signature = _path_signature(previews_first)
        second_signature = _path_signature(previews_second)
        if first_signature != second_signature:
            issues.append(
                ValidationIssue(
                    "error",
                    "level path preview is not stable across repeated runs",
                )
            )

        for preview in previews_first:
            target_row = window.desktop_service._path_target_build(session, preview.config, levels_ahead)
            if target_row is None:
                issues.append(
                    ValidationIssue(
                        "error",
                        f"missing path target row for {preview.config.title}",
                    )
                )
                continue

            if preview.config.title == "Selected":
                start_metric = preview.steps[0].metric
                if start_metric is None or not math.isclose(
                    float(start_metric),
                    float(selected_metric),
                    rel_tol=1e-9,
                    abs_tol=1e-6,
                ):
                    issues.append(
                        ValidationIssue(
                            "error",
                            "path preview start step no longer uses the bleed-aware objective metric",
                        )
                    )
                if preview.steps[0].score is None or not math.isclose(
                    float(preview.steps[0].score),
                    float(start_metric or 0.0),
                    rel_tol=1e-9,
                    abs_tol=1e-6,
                ):
                    issues.append(
                        ValidationIssue(
                            "error",
                            "path preview score and displayed metric diverged for Max AR + Bleed",
                        )
                    )

            target_state = window._combat_state_from_row(target_row)
            final_state = preview.steps[-1].stats
            if preview.steps[0].stats != preview.config.start_state:
                issues.append(
                    ValidationIssue(
                        "error",
                        f"path preview for {preview.config.title} does not start from its solved current-level build",
                    )
                )
            if final_state != target_state:
                issues.append(
                    ValidationIssue(
                        "error",
                        f"path preview for {preview.config.title} does not reach its exact target state",
                    )
                )

            for previous, current in zip(preview.steps, preview.steps[1:]):
                deltas = {
                    "str": current.stats.str_stat - previous.stats.str_stat,
                    "dex": current.stats.dex - previous.stats.dex,
                    "int": current.stats.int_stat - previous.stats.int_stat,
                    "fai": current.stats.fai - previous.stats.fai,
                    "arc": current.stats.arc - previous.stats.arc,
                }
                positive = [stat_key for stat_key, delta in deltas.items() if delta == 1]
                if len(positive) != 1 or any(delta not in (0, 1) for delta in deltas.values()):
                    issues.append(
                        ValidationIssue(
                            "error",
                            f"path preview for {preview.config.title} does not add exactly one combat stat per level",
                        )
                    )
                    break
                if current.added_stat != positive[0]:
                    issues.append(
                        ValidationIssue(
                            "error",
                            f"path preview for {preview.config.title} recorded the wrong added stat",
                        )
                    )
                    break
                if (
                    current.stats.str_stat > target_state.str_stat
                    or current.stats.dex > target_state.dex
                    or current.stats.int_stat > target_state.int_stat
                    or current.stats.fai > target_state.fai
                    or current.stats.arc > target_state.arc
                ):
                    issues.append(
                        ValidationIssue(
                            "error",
                            f"path preview for {preview.config.title} overshoots the solved target state",
                        )
                    )
                    break

        bleed_watch_payload = window.desktop_service.build_affinity_watch(session, selected, 2)
        expected_line_order = [
            line.affinity
            for line in sorted(
                bleed_watch_payload.lines,
                key=lambda line: (
                    float(line.end_metric if line.end_metric is not None else float("-inf")),
                    app_module.desktop_models.result_rank_key(line.final_build) if line.final_build is not None else tuple(),
                ),
                reverse=True,
            )
        ]
        actual_line_order = [line.affinity for line in bleed_watch_payload.lines]
        if actual_line_order != expected_line_order:
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher lines are not sorted by objective metric and deterministic rank",
                )
            )
        selected_line = next(
            (line for line in bleed_watch_payload.lines if line.affinity == selected.affinity),
            None,
        )
        if selected_line is None:
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher lost the selected affinity line for Max AR + Bleed",
                )
            )
        else:
            first_metric_point = next(
                (point for point in selected_line.points if point.solved is not None and point.metric is not None),
                None,
            )
            if first_metric_point is None or not math.isclose(
                float(first_metric_point.metric or 0.0),
                float(first_metric_point.solved.score if first_metric_point.solved is not None else 0.0),
                rel_tol=1e-9,
                abs_tol=1e-6,
            ):
                issues.append(
                    ValidationIssue(
                        "error",
                        "affinity watcher no longer uses the bleed-aware objective metric",
                    )
                )

        sword_rows = app_module.core.optimize_builds(
            data=window.data,
            class_name="Samurai",
            character_level=84,
            vig=40,
            mnd=11,
            end=20,
            str_stat=21,
            dex=15,
            int_stat=40,
            fai=8,
            arc=8,
            max_upgrade=25,
            fixed_upgrade=25,
            two_handing=False,
            weapon_name="Sword Lance",
            affinity="Magic",
            aow_name="Glintstone Pebble",
            objective="max_ar",
            top_k=10,
            weapon_type_key=None,
            somber_filter="all",
            min_str=0,
            min_dex=0,
            min_int=0,
            min_fai=0,
            min_arc=0,
            lock_str=None,
            lock_dex=None,
            lock_int=None,
            lock_fai=None,
            lock_arc=None,
        )
        if any(row.fai > 8 or row.arc > 8 for row in sword_rows):
            issues.append(
                ValidationIssue(
                    "error",
                    "Sword Lance Magic still wastes points in zero-scaling FAI/ARC",
                )
            )

        saved_results = list(window.current_results)
        saved_compare_error = window.compare_resolution_error
        window.current_results = [selected, compare]
        window.results_table.setRowCount(len(window.current_results))
        original_solve_build = window.desktop_service.solve_build
        try:
            def fail_solve_build(*args: object, **kwargs: object) -> object:
                raise RuntimeError("synthetic compare failure")

            window.desktop_service.solve_build = fail_solve_build
            try:
                window._rebuild_upgrade_table()
            except Exception as exc:
                issues.append(
                    ValidationIssue(
                        "error",
                        f"compare rebuild propagated a service failure instead of surfacing it: {exc}",
                    )
                )
            else:
                compare_body = window.compare_compare_panel["body"].text()
                if "synthetic compare failure" not in compare_body:
                    issues.append(
                        ValidationIssue(
                            "error",
                            "compare rebuild no longer surfaces synchronous service failures in the UI state",
                        )
                    )
                if window.compare_resolution_error != "synthetic compare failure":
                    issues.append(
                        ValidationIssue(
                            "error",
                            "compare rebuild did not retain the surfaced synchronous failure message",
                        )
                    )
        finally:
            window.desktop_service.solve_build = original_solve_build
            window.current_results = saved_results
            window.compare_resolution_error = saved_compare_error
            window._rebuild_upgrade_table()

        watcher_row = {
            "weapon_name": "Claymore",
            "affinity": "Fire",
            "aow_name": "Double Slash",
            "best_upgrade": 25,
            "str_stat": 20,
            "dex": 20,
            "int_stat": 9,
            "fai": 8,
            "arc": 8,
            "best_ar_total": 0.0,
            "score": 0.0,
            "bleed_buildup": 0.0,
            "bleed_buildup_add": 0.0,
            "frost_buildup": 0.0,
            "poison_buildup": 0.0,
            "scarlet_rot_buildup": 0.0,
            "aow_first_hit_damage": 0.0,
            "aow_full_sequence_damage": 0.0,
        }
        window.str_spin.setValue(20)
        window.dex_spin.setValue(20)
        window.int_spin.setValue(9)
        window.fai_spin.setValue(8)
        window.arc_spin.setValue(8)
        window._set_combo_by_data(window.objective_combo, "max_ar")
        window._refresh_estimate()
        affinity_lines_first, affinity_breaks_first = window._build_affinity_watch_data(watcher_row, 4)
        affinity_lines_second, affinity_breaks_second = window._build_affinity_watch_data(watcher_row, 4)
        if len(affinity_lines_first) < 2:
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher did not produce multiple legal affinity lines",
                )
            )
        if _affinity_watch_signature(affinity_lines_first) != _affinity_watch_signature(affinity_lines_second):
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher lines are not stable across repeated runs",
                )
            )
        if _breakpoint_signature(affinity_breaks_first) != _breakpoint_signature(affinity_breaks_second):
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher breakpoints are not stable across repeated runs",
                )
            )

        invalid_row = dict(watcher_row)
        invalid_row["affinity"] = "Invalid Affinity"
        invalid_lines, _ = window._build_affinity_watch_data(invalid_row, 2)
        if not invalid_lines:
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher failed to skip an invalid preferred affinity cleanly",
                )
            )

        window._set_combo_by_data(window.objective_combo, "aow_full_sequence")
        window._refresh_estimate()
        affinity_lines_aow, _ = window._build_affinity_watch_data(watcher_row, 4)
        if len(affinity_lines_aow) < 2:
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher AoW objective did not produce multiple legal affinity lines",
                )
            )
        max_ar_end = {
            line.affinity: line.end_metric
            for line in affinity_lines_first
        }
        aow_end = {
            line.affinity: line.end_metric
            for line in affinity_lines_aow
        }
        shared_affinities = sorted(set(max_ar_end).intersection(aow_end))
        if not shared_affinities:
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher objective comparison had no shared affinities",
                )
            )
        elif all(
            math.isclose(float(max_ar_end[affinity] or 0.0), float(aow_end[affinity] or 0.0), rel_tol=1e-9, abs_tol=1e-9)
            for affinity in shared_affinities
        ):
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher objective change did not affect any affinity metric",
                )
            )

        synthetic_lines = [
            app_module.desktop_models.AffinityWatchLine(
                affinity="Keen",
                points=(
                    app_module.desktop_models.AffinityWatchPoint(10, 100.0, _synthetic_build(app_module, "Keen", 100.0, 10)),
                    app_module.desktop_models.AffinityWatchPoint(11, 100.0, _synthetic_build(app_module, "Keen", 100.0, 11)),
                    app_module.desktop_models.AffinityWatchPoint(12, 101.0, _synthetic_build(app_module, "Keen", 101.0, 12)),
                ),
                start_metric=100.0,
                end_metric=101.0,
                final_build=_synthetic_build(app_module, "Keen", 101.0, 12),
            ),
            app_module.desktop_models.AffinityWatchLine(
                affinity="Heavy",
                points=(
                    app_module.desktop_models.AffinityWatchPoint(10, 99.0, _synthetic_build(app_module, "Heavy", 99.0, 10)),
                    app_module.desktop_models.AffinityWatchPoint(11, 103.0, _synthetic_build(app_module, "Heavy", 103.0, 11)),
                    app_module.desktop_models.AffinityWatchPoint(12, 104.0, _synthetic_build(app_module, "Heavy", 104.0, 12)),
                ),
                start_metric=99.0,
                end_metric=104.0,
                final_build=_synthetic_build(app_module, "Heavy", 104.0, 12),
            ),
            app_module.desktop_models.AffinityWatchLine(
                affinity="Magic",
                points=(
                    app_module.desktop_models.AffinityWatchPoint(10, None, None),
                    app_module.desktop_models.AffinityWatchPoint(11, 103.0, _synthetic_build(app_module, "Magic", 103.0, 11, weapon_id=999)),
                    app_module.desktop_models.AffinityWatchPoint(12, None, None),
                ),
                start_metric=None,
                end_metric=103.0,
                final_build=_synthetic_build(app_module, "Magic", 103.0, 11, weapon_id=999),
            ),
        ]
        synthetic_breaks = window.desktop_service.detect_affinity_breakpoints(synthetic_lines, [10, 11, 12], "max_ar")
        if _breakpoint_signature(synthetic_breaks) != [(11, "Keen", "Heavy")]:
            issues.append(
                ValidationIssue(
                    "error",
                    "affinity watcher breakpoint detection regressed for leader changes, ties, or missing rows",
                )
            )
        if len(window.current_results) >= 2:
            target_fingerprint = window.current_results[1].fingerprint
            window.results_table.selectRow(1)
            window._selected_result_index()
            window._start_search()
            while window.active_run_id is not None:
                QtWidgets.QApplication.processEvents()
            selected_idx = window._selected_result_index()
            if selected_idx is None or window.current_results[selected_idx].fingerprint != target_fingerprint:
                issues.append(
                    ValidationIssue(
                        "error",
                        "selected solved build was not preserved across a compatible rerun",
                    )
                )
    finally:
        window.close()
        if created_app:
            app.quit()

    return issues


def _path_signature(previews: list[Any]) -> list[tuple[str, tuple[tuple[object, int, int, int, int, int], ...]]]:
    signatures: list[tuple[str, tuple[tuple[object, int, int, int, int, int], ...]]] = []
    for preview in previews:
        signatures.append(
            (
                preview.config.title,
                tuple(
                    (
                        step.added_stat,
                        step.stats.str_stat,
                        step.stats.dex,
                        step.stats.int_stat,
                        step.stats.fai,
                        step.stats.arc,
                    )
                    for step in preview.steps
                ),
            )
        )
    return signatures


def _affinity_watch_signature(
    lines: list[Any],
) -> list[tuple[str, tuple[tuple[int, float | None, str | None], ...]]]:
    return [
        (
            line.affinity,
            tuple(
                (
                    point.level,
                    None if point.metric is None else round(float(point.metric), 6),
                    None if point.solved is None else point.solved.affinity,
                )
                for point in line.points
            ),
        )
        for line in lines
    ]


def _breakpoint_signature(breakpoints: list[Any]) -> list[tuple[int, str, str]]:
    return [
        (int(breakpoint.level), str(breakpoint.outgoing_affinity), str(breakpoint.incoming_affinity))
        for breakpoint in breakpoints
    ]


def _synthetic_build(app_module, affinity: str, score: float, upgrade: int, weapon_id: int = 1):
    return app_module.desktop_models.SolvedBuild(
        weapon_id=weapon_id,
        weapon_name="Synthetic",
        affinity=affinity,
        aow_name=None,
        upgrade=upgrade,
        str_stat=10,
        dex=10,
        int_stat=10,
        fai=10,
        arc=10,
        ar_total=score,
        ar_physical=score,
        ar_magic=0.0,
        ar_fire=0.0,
        ar_lightning=0.0,
        ar_holy=0.0,
        score=score,
        bleed_buildup=0.0,
        bleed_buildup_add=0.0,
        frost_buildup=0.0,
        poison_buildup=0.0,
        scarlet_rot_buildup=0.0,
        aow_first_hit_damage=score,
        aow_full_sequence_damage=score,
    )


def main() -> int:
    project_root = Path(__file__).resolve().parents[2]
    data_dir = project_root / "data" / "phase1"
    if not data_dir.exists():
        print(f"ERROR: missing data dir {data_dir}")
        return 1

    issues = []
    issues.extend(validate_data_snapshot(data_dir))
    issues.extend(validate_runtime_ar(data_dir))
    issues.extend(validate_level_paths(project_root))

    errors = [issue for issue in issues if issue.level == "error"]
    warnings = [issue for issue in issues if issue.level == "warning"]

    for issue in warnings:
        print(f"WARN: {issue.message}")
    for issue in errors:
        print(f"ERROR: {issue.message}")

    if errors:
        print(f"VALIDATION FAILED ({len(errors)} errors, {len(warnings)} warnings)")
        return 1

    print(f"VALIDATION PASSED ({len(warnings)} warnings)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
