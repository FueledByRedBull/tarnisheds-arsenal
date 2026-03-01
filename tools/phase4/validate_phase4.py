from __future__ import annotations

import csv
import math
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
                int(row["curve_id_str"]),
                int(row["curve_id_dex"]),
                int(row["curve_id_int"]),
                int(row["curve_id_fai"]),
                int(row["curve_id_arc"]),
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

    return issues


def main() -> int:
    project_root = Path(__file__).resolve().parents[2]
    data_dir = project_root / "data" / "phase1"
    if not data_dir.exists():
        print(f"ERROR: missing data dir {data_dir}")
        return 1

    issues = []
    issues.extend(validate_data_snapshot(data_dir))
    issues.extend(validate_runtime_ar(data_dir))

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
