from __future__ import annotations

import math
from pathlib import Path

from .models import ValidationIssue

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
