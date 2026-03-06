from __future__ import annotations

import csv
import importlib.util
import math
import os
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


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
    aow_damage_coverage = read_csv(data_dir / "aow_damage_coverage.csv")
    attack_element_correct_ext = read_csv(data_dir / "attack_element_correct_ext.csv")
    weapon_passives = read_csv(data_dir / "weapon_passives.csv")
    aow_weapon_compat = read_csv(data_dir / "aow_weapon_compat.csv")

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
    if len(aow_weapon_compat) < 40000:
        issues.append(
            ValidationIssue(
                "error",
                f"aow_weapon_compat.csv row count too low: {len(aow_weapon_compat)}",
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

    return issues


def validate_runtime_ar(data_dir: Path) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    try:
        import er_optimizer_core as core
    except Exception as exc:
        issues.append(ValidationIssue("warning", f"runtime AR checks skipped: {exc}"))
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
            "weapon_name": "Lordsworn's Quality Greatsword",
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
        objective="max_ar_plus_bleed",
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

    return issues


def validate_level_paths(project_root: Path) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    try:
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        from PyQt6 import QtWidgets
    except Exception as exc:
        issues.append(ValidationIssue("warning", f"level path checks skipped: {exc}"))
        return issues

    try:
        app_module = load_app_module(project_root)
    except Exception as exc:
        issues.append(ValidationIssue("warning", f"level path checks skipped: {exc}"))
        return issues

    app = QtWidgets.QApplication.instance()
    created_app = app is None
    if app is None:
        app = QtWidgets.QApplication([])

    window = app_module.MainWindow()
    try:
        app_module.apply_dark_theme(app)
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

        window.active_compare_selected = selected
        window.active_compare_target = compare
        levels_ahead = 5
        previews_first = window._build_level_path_previews(levels_ahead)
        previews_second = window._build_level_path_previews(levels_ahead)
        if previews_first is None or previews_second is None or len(previews_first) != 2:
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
            target_row = window._level_path_target_row(preview.config, levels_ahead)
            if target_row is None:
                issues.append(
                    ValidationIssue(
                        "error",
                        f"missing path target row for {preview.config.title}",
                    )
                )
                continue

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
    finally:
        window.close()
        if created_app:
            app.quit()

    return issues


def _path_signature(previews: list[object]) -> list[tuple[str, tuple[tuple[object, int, int, int, int, int], ...]]]:
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
