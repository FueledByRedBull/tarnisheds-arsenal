from __future__ import annotations

import argparse
import csv
import json
import sys
from collections import defaultdict
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.extract_motion_workbook import load_weapon_workbook_data  # noqa: E402
from tools.phase1.derive_phase1_extras import (  # noqa: E402
    build_aow_affinity_compat,
    derive_phase1_diagnostics,
)
from tools.phase1.profiles import profile_definition  # noqa: E402
from tools.phase1.snapshot_manifest import validate_snapshot_manifest  # noqa: E402
from tools.phase4.validation.aow_effect_graph import validate_aow_effect_graph  # noqa: E402
from tools.phase4.validation.models import ValidationIssue  # noqa: E402
from tools.phase4.convergence_reference import validate_reference  # noqa: E402


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


def validate_profile_snapshot(data_dir: Path, profile_id: str) -> list[ValidationIssue]:
    """Validate contracts shared by every profile without assuming Vanilla mechanics."""
    issues: list[ValidationIssue] = []
    profile = profile_definition(profile_id)
    try:
        manifest = validate_snapshot_manifest(data_dir, profile)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        return [ValidationIssue("error", f"invalid atomic snapshot manifest: {error}")]

    weapons = read_csv(data_dir / "weapons.csv")
    reinforce = read_csv(data_dir / "reinforce.csv")
    calc_correct = read_csv(data_dir / "calc_correct.csv")
    aows = read_csv(data_dir / "aow.csv")
    aow_attack_data = read_csv(data_dir / "aow_attack_data.csv")
    native_skill_attack_data = read_csv(data_dir / "native_skill_attack_data.csv")
    aow_route_assignments = read_csv(data_dir / "aow_route_assignments.csv")
    aow_weapon_compat = read_csv(data_dir / "aow_weapon_compat.csv")
    attack_element_correct = read_csv(data_dir / "attack_element_correct.csv")
    attack_element_correct_ext = read_csv(data_dir / "attack_element_correct_ext.csv")
    weapon_passives = read_csv(data_dir / "weapon_passives.csv")
    weapon_scaling_summary, aow_affinity_compat = derive_phase1_diagnostics(
        data_dir,
        extended_scaling_grades=profile.rules.extended_scaling_grades,
    )

    minimums = [
        ("weapons.csv", len(weapons), 100),
        ("reinforce.csv", len(reinforce), 100),
        ("calc_correct.csv", len(calc_correct), 1_000),
        ("aow.csv", len(aows), 50),
        ("attack_element_correct_ext.csv", len(attack_element_correct_ext), 100),
    ]
    for label, actual, minimum in minimums:
        if actual < minimum:
            issues.append(ValidationIssue("error", f"{label} row count too low: {actual} < {minimum}"))

    empty_names = [row for row in weapons if not row.get("name", "").strip()]
    npc_rows = [
        row
        for row in weapons
        if "[npc]" in row.get("name", "").casefold()
        or "(npc)" in row.get("name", "").casefold()
    ]
    duplicate_ids = len({row.get("weapon_id") for row in weapons}) != len(weapons)
    affinity_names = {row.get("affinity", "") for row in weapons}
    expected_affinities = set(profile.affinity_by_slot.values())
    if empty_names:
        issues.append(ValidationIssue("error", "weapons.csv contains unnamed configurations"))
    if npc_rows:
        issues.append(ValidationIssue("error", "weapons.csv contains NPC-only configurations"))
    if duplicate_ids:
        issues.append(ValidationIssue("error", "weapons.csv contains duplicate weapon configuration IDs"))
    if affinity_names != expected_affinities:
        issues.append(
            ValidationIssue(
                "error",
                f"affinity set mismatch: missing={sorted(expected_affinities - affinity_names)}, extra={sorted(affinity_names - expected_affinities)}",
            )
        )
    if manifest["capabilities"]["weaponPassives"] and len(weapon_passives) != len(weapons):
        issues.append(
            ValidationIssue(
                "error",
                f"weapon_passives.csv must align 1:1 with weapons.csv ({len(weapon_passives)} vs {len(weapons)})",
            )
        )
    if manifest["capabilities"]["aowCompatibility"] and not aow_weapon_compat:
        issues.append(ValidationIssue("error", "AoW compatibility is declared but no compatibility rows exist"))
    reinforce_caps = max_reinforce_levels(reinforce)
    weapon_caps = {
        reinforce_caps.get(int(row["reinforce_type"]), -1)
        for row in weapons
    }
    allowed_caps = {
        0,
        int(manifest["rules"]["standardMaxUpgrade"]),
        int(manifest["rules"]["somberMaxUpgrade"]),
    }
    if not weapon_caps.issubset(allowed_caps):
        issues.append(
            ValidationIssue(
                "error",
                f"weapon reinforcement caps {sorted(weapon_caps)} exceed profile rules {sorted(allowed_caps)}",
            )
        )
    upgradeable_weapon_caps = weapon_caps - {0}
    if not manifest["rules"]["separateUpgradeCaps"] and len(upgradeable_weapon_caps) != 1:
        issues.append(
            ValidationIssue(
                "error",
                "single-path reinforcement profile contains multiple player-weapon caps",
            )
        )
    if manifest["rules"]["zeroAttackElementUsesWeaponScaling"]:
        zero_aec = next(
            (row for row in attack_element_correct if row["attack_element_correct_id"] == "0"),
            None,
        )
        if zero_aec is None or any(value != "1" for key, value in zero_aec.items() if key != "attack_element_correct_id"):
            issues.append(
                ValidationIssue(
                    "error",
                    "profile requires weapon-scaling fallback but attack-element row 0 is not fully enabled",
                )
            )
    aow_names_by_id = {row["aow_id"]: row["name"] for row in aows}
    weapons_by_id = {row["weapon_id"]: row for row in weapons}
    invalid_compatibility = [
        row
        for row in aow_weapon_compat
        if aow_names_by_id.get(row.get("aow_id", "")) != row.get("aow_name")
        or weapons_by_id.get(row.get("weapon_id", ""), {}).get("name") != row.get("weapon_name")
        or weapons_by_id.get(row.get("weapon_id", ""), {}).get("affinity") != row.get("affinity")
    ]
    if invalid_compatibility:
        issues.append(
            ValidationIssue(
                "error",
                f"AoW compatibility contains stale profile names or IDs: {invalid_compatibility[0]}",
            )
        )
    scaling_ids = {row.get("weapon_id", "") for row in weapon_scaling_summary}
    if scaling_ids != set(weapons_by_id):
        issues.append(
            ValidationIssue(
                "error",
                "derived weapon scaling summary does not cover every weapon configuration",
            )
        )
    diagnostic_names = [
        row.get("name", "") for row in weapon_scaling_summary
    ] + [
        name
        for row in aow_affinity_compat
        for name in row.get("sample_weapon_names", "").split("|")
    ]
    if any("[npc]" in name.casefold() or "(npc)" in name.casefold() for name in diagnostic_names):
        issues.append(ValidationIssue("error", "derived diagnostics contain NPC-only weapon names"))
    source_kinds = {source.get("kind") for source in manifest["sources"]}
    if profile_id == "convergence":
        reference_path = ROOT / "data" / "reference" / "convergence-3.0.0.1-weapons.json"
        try:
            validate_reference(data_dir, reference_path)
        except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
            issues.append(
                ValidationIssue(
                    "error",
                    f"Convergence external-reference differential failed: {error}",
                )
            )
        if manifest["rules"]["statusBuildupScales"]:
            issues.append(
                ValidationIssue(
                    "error",
                    "Convergence weapon status must remain fixed across stats and upgrades",
                )
            )
        required_sources = {
            "regulation",
            "weaponNamesBase",
            "weaponNamesDlc01",
            "artsNamesBase",
            "artsNamesDlc01",
            "modVersion",
            "weaponAvailability",
        }
        if not required_sources.issubset(source_kinds):
            issues.append(
                ValidationIssue(
                    "error",
                    "Convergence provenance is missing base/DLC name tables or regulation/version sources",
                )
            )
        expected_variants = {
            "10200000": "Galvanic Culling Blade [Twinblade]",
        }
        for weapon_id, expected_name in expected_variants.items():
            if weapons_by_id.get(weapon_id, {}).get("name") != expected_name:
                issues.append(
                    ValidationIssue(
                        "error",
                        f"Convergence player variant {weapon_id} ({expected_name}) is missing",
                    )
                )
        if "10205000" in weapons_by_id:
            issues.append(
                ValidationIssue(
                    "error",
                    "internal Galvanic Culling Blade transformation form must not be searchable",
                )
            )
        affinities_by_name: dict[str, set[str]] = defaultdict(set)
        for weapon in weapons:
            affinities_by_name[weapon["name"]].add(weapon["affinity"])
        if affinities_by_name["Dueling Shield"] != expected_affinities:
            issues.append(
                ValidationIssue(
                    "error",
                    "Dueling Shield must retain every legal Convergence infusion",
                )
            )
        if affinities_by_name["Carian Thrusting Shield"] != {"Standard"}:
            issues.append(
                ValidationIssue(
                    "error",
                    "Carian Thrusting Shield must remain a fixed Standard configuration",
                )
            )
        galvanic_scaling = next(
            (
                row
                for row in weapon_scaling_summary
                if row["weapon_id"] == "10200000"
            ),
            None,
        )
        if galvanic_scaling is None or galvanic_scaling.get("usable_stats") != "STR|DEX|INT":
            issues.append(
                ValidationIssue(
                    "error",
                    "Galvanic Culling Blade must scale with STR, DEX, and INT",
                )
            )
    if not manifest["capabilities"]["aowDamage"] and (aow_attack_data or native_skill_attack_data):
        issues.append(
            ValidationIssue(
                "error",
                "AoW damage is declared unsupported but damage rows are present and could be consumed silently",
            )
        )
    if not manifest["capabilities"]["aowRoutes"] and aow_route_assignments:
        issues.append(
            ValidationIssue(
                "error",
                "AoW routes are declared unsupported but route rows are present and could be consumed silently",
            )
        )
    return issues


def scoped(profile_id: str, issues: Iterable[ValidationIssue]) -> list[ValidationIssue]:
    return [ValidationIssue(issue.level, f"[{profile_id}] {issue.message}") for issue in issues]




def validate_data_snapshot(data_dir: Path) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []

    try:
        validate_snapshot_manifest(data_dir)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        issues.append(ValidationIssue("error", f"invalid atomic snapshot manifest: {error}"))

    weapons = read_csv(data_dir / "weapons.csv")
    reinforce = read_csv(data_dir / "reinforce.csv")
    calc_correct = read_csv(data_dir / "calc_correct.csv")
    aows = read_csv(data_dir / "aow.csv")
    aow_attack_data = read_csv(data_dir / "aow_attack_data.csv")
    aow_effect_data = read_csv(data_dir / "aow_effect_data.csv")
    aow_effect_coverage = read_csv(data_dir / "aow_effect_coverage.csv")
    aow_effect_exclusions = read_csv(data_dir / "aow_effect_exclusions.csv")
    aow_damage_coverage = read_csv(data_dir / "aow_damage_coverage.csv")
    native_skill_attack_data = read_csv(data_dir / "native_skill_attack_data.csv")
    native_skill_damage_coverage = read_csv(data_dir / "native_skill_damage_coverage.csv")
    aow_route_assignments = read_csv(data_dir / "aow_route_assignments.csv")
    aow_route_exclusions = read_csv(data_dir / "aow_route_exclusions.csv")
    attack_element_correct_ext = read_csv(data_dir / "attack_element_correct_ext.csv")
    weapon_passives = read_csv(data_dir / "weapon_passives.csv")
    weapon_passive_overlays = read_csv(data_dir / "weapon_passive_overlays.csv")
    passive_effect_coverage = read_csv(data_dir / "passive_effect_coverage.csv")
    aow_weapon_compat = read_csv(data_dir / "aow_weapon_compat.csv")
    aow_affinity_compat = build_aow_affinity_compat(aow_weapon_compat)

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
    npc_rows = [row for row in weapons if "[NPC]" in row.get("name", "")]
    if npc_rows:
        issues.append(
            ValidationIssue(
                "error",
                f"weapons.csv contains NPC-only rows: {npc_rows[0].get('name', '<unknown>')}",
            )
        )
    disabled_nonstandard = [
        row
        for row in weapons
        if row.get("affinity") != "Standard" and row.get("disable_gem_attr", "0") != "0"
    ]
    if disabled_nonstandard:
        sample = disabled_nonstandard[0]
        issues.append(
            ValidationIssue(
                "error",
                (
                    "nonstandard affinity rows must not disable gem attributes; "
                    f"found {sample.get('name')} | {sample.get('affinity')}"
                ),
            )
        )

    workbook_path = data_dir / "ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx"
    if workbook_path.exists():
        workbook_weapon_ids = set(load_weapon_workbook_data(workbook_path))
        standard_weapon_ids = {
            int(row["weapon_id"]) for row in weapons if row.get("affinity") == "Standard"
        }
        if standard_weapon_ids != workbook_weapon_ids:
            issues.append(
                ValidationIssue(
                    "error",
                    (
                        "standard player weapon IDs do not match WeaponData: "
                        f"missing={sorted(workbook_weapon_ids - standard_weapon_ids)[:10]} "
                        f"extra={sorted(standard_weapon_ids - workbook_weapon_ids)[:10]}"
                    ),
                )
            )
    issues.extend(
        validate_aow_effect_graph(
            aows,
            aow_attack_data + native_skill_attack_data,
            aow_effect_data,
            aow_effect_coverage,
            aow_effect_exclusions,
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
        required_weapon_columns = {
            "disable_two_hand_bonus",
            "weapon_type_keys",
            "stamina_consumption_rate",
            "physical_attribute_primary",
            "physical_attribute_secondary",
        }
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

    source_attack_rows = {
        (int(row["aow_id"]), int(row["sheet_row"])): row
        for row in [*aow_attack_data, *native_skill_attack_data]
    }
    assigned_attack_keys = {
        (int(row["aow_id"]), int(row["sheet_row"])) for row in aow_route_assignments
    }
    excluded_attack_keys = {
        (int(row["aow_id"]), int(row["sheet_row"])) for row in aow_route_exclusions
    }
    supported_route_effect_keys = {
        (int(row["aow_id"]), int(row["sheet_row"]))
        for row in aow_effect_data
        if row.get("is_supported") == "1"
        and row.get("role")
        in {"per_hit_status", "per_hit_attack_power", "replacement_or_chained"}
    }
    missing_route_assignments = sorted(
        key
        for key, row in source_attack_rows.items()
        if row.get("is_lacking_fp") == "0"
        and (row.get("is_damaging") == "1" or key in supported_route_effect_keys)
        and key not in assigned_attack_keys
    )
    if missing_route_assignments:
        issues.append(
            ValidationIssue(
                "error",
                f"damaging/effect-bearing full-FP attack rows lack route assignments: {missing_route_assignments[:10]}",
            )
        )
    undocumented_lacking_fp = sorted(
        key
        for key, row in source_attack_rows.items()
        if row.get("is_lacking_fp") == "1" and key not in excluded_attack_keys
    )
    if undocumented_lacking_fp:
        issues.append(
            ValidationIssue(
                "error",
                f"lacking-FP attack rows lack documented exclusions: {undocumented_lacking_fp[:10]}",
            )
        )
    stale_route_references = sorted(
        key for key in assigned_attack_keys | excluded_attack_keys if key not in source_attack_rows
    )
    if stale_route_references:
        issues.append(
            ValidationIssue(
                "error",
                f"route data references missing source rows: {stale_route_references[:10]}",
            )
        )

    route_ids_by_skill: dict[str, set[str]] = defaultdict(set)
    for row in aow_route_assignments:
        route_ids_by_skill[row["aow_name"]].add(row["route_id"])
    expected_route_ids = {
        "Wild Strikes": {"r1", "r2"},
        "Ghostflame Call": {"r1", "r2"},
        "Barbaric Roar": {
            "1h_uncharged",
            "1h_charged",
            "2h_uncharged",
            "2h_charged",
        },
    }
    for skill_name, expected in expected_route_ids.items():
        if route_ids_by_skill.get(skill_name) != expected:
            issues.append(
                ValidationIssue(
                    "error",
                    (
                        f"{skill_name} route IDs changed: "
                        f"{sorted(route_ids_by_skill.get(skill_name, set()))}"
                    ),
                )
            )

    valid_physical_attributes = {
        "standard",
        "strike",
        "slash",
        "pierce",
        "adaptive_primary",
        "adaptive_secondary",
    }
    for file_name, rows in (
        ("weapons.csv", weapons),
        ("aow_attack_data.csv", aow_attack_data),
        ("native_skill_attack_data.csv", native_skill_attack_data),
    ):
        fields = (
            ("physical_attribute_primary", "physical_attribute_secondary")
            if file_name == "weapons.csv"
            else ("physical_attack_attribute",)
        )
        invalid = [
            (row.get("weapon_id") or row.get("sheet_row"), field, row.get(field))
            for row in rows
            for field in fields
            if row.get(field) not in valid_physical_attributes
        ]
        if invalid:
            issues.append(
                ValidationIssue(
                    "error",
                    f"{file_name} has invalid physical attack attributes: {invalid[:3]}",
                )
            )

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
            issues.append(ValidationIssue("error", f"derived AoW affinity compatibility references missing aow_id={aow_id}"))
            break
        if not row["name"].strip() or row["name"] != canonical["name"]:
            issues.append(ValidationIssue("error", f"derived AoW affinity compatibility has stale/placeholder name for aow_id={aow_id}"))
            break

    native_statuses = {row["status"] for row in native_skill_damage_coverage}
    unexpected_native_statuses = sorted(native_statuses.difference({"matched", "generic_aow"}))
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

    allowed_effect_coverage_statuses = {"resolved", "excluded_missing_source"}
    unexpected_effect_statuses = sorted(
        {
            row.get("status", "")
            for row in passive_effect_coverage
            if row.get("status", "") not in allowed_effect_coverage_statuses
        }
    )
    if unexpected_effect_statuses:
        issues.append(
            ValidationIssue(
                "error",
                "passive_effect_coverage.csv has unclassified statuses: "
                + ", ".join(unexpected_effect_statuses),
            )
        )
    undocumented_effect_exclusions = [
        row
        for row in passive_effect_coverage
        if row.get("status") != "resolved" and not row.get("reason", "").strip()
    ]
    if undocumented_effect_exclusions:
        issues.append(
            ValidationIssue(
                "error",
                "passive_effect_coverage.csv contains an exclusion without a reason",
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




def main() -> int:
    parser = argparse.ArgumentParser(description="Validate extracted data and runtime calculations.")
    parser.add_argument("--report", type=Path, help="Write a machine-readable validation report.")
    parser.add_argument("--diagnostic", action="store_true", help="Include profile versions and CSV row counts in the report and console output.")
    args = parser.parse_args()
    project_root = Path(__file__).resolve().parents[2]
    profile_dirs = {
        "vanilla": project_root / "data" / "phase1",
        "convergence": project_root / "data" / "profiles" / "convergence",
    }
    diagnostics = {profile_id: profile_diagnostics(data_dir) for profile_id, data_dir in profile_dirs.items()} if args.diagnostic else None
    issues: list[ValidationIssue] = []
    for profile_id, data_dir in profile_dirs.items():
        if not data_dir.exists():
            issues.append(ValidationIssue("error", f"[{profile_id}] missing data dir {data_dir}"))
            continue
        issues.extend(scoped(profile_id, validate_profile_snapshot(data_dir, profile_id)))

    vanilla_dir = profile_dirs["vanilla"]
    if vanilla_dir.exists():
        issues.extend(scoped("vanilla", validate_data_snapshot(vanilla_dir)))

    errors = [issue for issue in issues if issue.level == "error"]
    warnings = [issue for issue in issues if issue.level == "warning"]

    for issue in warnings:
        print(f"WARN: {issue.message}")
    for issue in errors:
        print(f"ERROR: {issue.message}")
    if diagnostics is not None:
        print(json.dumps({"diagnostics": diagnostics}, indent=2, sort_keys=True))

    if errors:
        write_validation_report(args.report, "failed", issues, list(profile_dirs), diagnostics)
        print(f"VALIDATION FAILED ({len(errors)} errors, {len(warnings)} warnings)")
        return 1

    write_validation_report(args.report, "passed", issues, list(profile_dirs), diagnostics)
    print(f"VALIDATION PASSED ({len(warnings)} warnings)")
    return 0


def write_validation_report(
    path: Path | None,
    status: str,
    issues: list[ValidationIssue],
    profiles: list[str] | None = None,
    diagnostics: dict[str, dict[str, object]] | None = None,
) -> None:
    if path is None:
        return
    payload = {
        "status": status,
        "errors": sum(issue.level == "error" for issue in issues),
        "warnings": sum(issue.level == "warning" for issue in issues),
        "profiles": profiles or [],
        "diagnostics": diagnostics,
        "issues": [
            {"level": issue.level, "message": issue.message}
            for issue in issues
        ],
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def profile_diagnostics(data_dir: Path) -> dict[str, object]:
    manifest_path = data_dir / "manifest.json"
    if not manifest_path.exists():
        return {"dataDir": str(data_dir), "missing": True}
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    row_counts = {}
    for filename in ("weapons.csv", "aow.csv", "aow_weapon_compat.csv", "calc_correct.csv", "reinforce.csv"):
        path = data_dir / filename
        if path.exists():
            with path.open("r", encoding="utf-8", newline="") as handle:
                row_counts[filename] = max(0, sum(1 for _ in csv.reader(handle)) - 1)
    return {
        "datasetVersion": manifest.get("datasetVersion"),
        "gameVersion": manifest.get("profile", {}).get("gameVersion"),
        "modelVersion": manifest.get("modelVersion"),
        "rowCounts": row_counts,
    }


if __name__ == "__main__":
    raise SystemExit(main())
