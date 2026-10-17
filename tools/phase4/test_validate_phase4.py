from __future__ import annotations

import json
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase1.extract_motion_workbook import MOTION_WORKBOOK_NAME
from tools.phase1.phase1_dump import MAX_EFFECTIVE_STRENGTH
from tools.phase1.profiles import profile_definition
from tools.phase4 import validate_phase4 as validation
from tools.phase4.validate_phase4 import (
    validate_aow_compatibility,
    validate_profile_source_provenance,
    validate_used_calc_correct_curves,
)


def curve_rows() -> list[dict[str, str]]:
    return [
        {
            "curve_id": "1",
            "stat_value": str(stat),
            "multiplier": str(float(stat)),
        }
        for stat in range(MAX_EFFECTIVE_STRENGTH + 1)
    ]


class ValidatePhase4Tests(unittest.TestCase):
    def test_vanilla_validation_reads_each_table_once(self) -> None:
        data_dir = validation.ROOT / "data/phase1"
        with patch.object(validation, "read_csv", wraps=validation.read_csv) as read:
            self.assertEqual(validation.validate_profile_snapshot(data_dir, "vanilla"), [])
        paths = [call.args[0] for call in read.call_args_list]
        self.assertEqual(len(paths), len(set(paths)))

    def test_vanilla_thresholds_and_unique_coverage_checks_remain_required(self) -> None:
        read_csv = validation.read_csv
        cases = [(name, count, f"{name} row count too low") for name, count in (
            ("weapons.csv", 2_999), ("reinforce.csv", 799),
            ("calc_correct.csv", 1_499), ("aow.csv", 99),
            ("attack_element_correct_ext.csv", 149),
            ("weapon_passive_overlays.csv", 999),
        )]
        cases.append(("aow_route_assignments.csv", 0, "lack route assignments"))
        for filename, count, message in cases:
            with self.subTest(filename=filename):
                def read(path: Path) -> list[dict[str, str]]:
                    rows = read_csv(path)
                    return rows[:count] if path.name == filename else rows

                with patch.object(validation, "read_csv", side_effect=read):
                    issues = validation.validate_profile_snapshot(validation.ROOT / "data/phase1", "vanilla")
                self.assertTrue(any(issue.level == "error" and message in issue.message for issue in issues))

    def test_invalid_manifest_stops_before_loading_tables(self) -> None:
        with patch.object(validation, "validate_snapshot_manifest", side_effect=ValueError("bad hash")), \
             patch.object(validation, "read_csv") as read:
            issues = validation.validate_profile_snapshot(validation.ROOT / "data/phase1", "vanilla")
        read.assert_not_called()
        self.assertEqual(issues[0].level, "error")
        self.assertIn("bad hash", issues[0].message)

    def test_routes_require_positive_hit_counts_and_finite_fixed_poise(self) -> None:
        read_csv = validation.read_csv
        for filename, field, invalid in (
            ("aow_route_assignments.csv", "hit_count", "0"),
            ("aow_route_assignments.csv", "hit_count", "1.5"),
            ("aow_route_assignments.csv", "hit_count", "65536"),
            ("aow_attack_data.csv", "poise_base", "nan"),
            ("aow_attack_data.csv", "poise_base", "-1"),
            ("aow_attack_data.csv", "poise_base", ""),
        ):
            with self.subTest(field=field, invalid=invalid):
                def read(path: Path) -> list[dict[str, str]]:
                    rows = read_csv(path)
                    if path.name == filename:
                        rows[0][field] = invalid
                    return rows

                with patch.object(validation, "read_csv", side_effect=read):
                    issues = validation.validate_profile_snapshot(validation.ROOT / "data/phase1", "vanilla")
                self.assertTrue(any(issue.level == "error" and field in issue.message for issue in issues))

    def test_vanilla_poise_must_match_weapondata(self) -> None:
        read_csv = validation.read_csv
        for field, invalid in (("base_poise", "0"), ("move_count", "0"), ("two_hand_heavy_poise", "")):
            with self.subTest(field=field):
                def read(path: Path) -> list[dict[str, str]]:
                    rows = read_csv(path)
                    if path.name == "weapons.csv":
                        target = next(row for row in rows if row["name"] == "Black Steel Greathammer")
                        target[field] = invalid
                    return rows

                with patch.object(validation, "read_csv", side_effect=read):
                    issues = validation.validate_profile_snapshot(validation.ROOT / "data/phase1", "vanilla")
                self.assertTrue(any("poise data differs from WeaponData" in issue.message for issue in issues))

    def test_shared_weapon_model_checks_cover_convergence(self) -> None:
        data_dir = validation.ROOT / "data/profiles/convergence"
        self.assertEqual(validation.validate_profile_snapshot(data_dir, "convergence"), [])
        read_csv = validation.read_csv
        used_curve = read_csv(data_dir / "weapons.csv")[0]["curve_id_physical"]
        for filename, field, invalid, message in (
            ("weapons.csv", "physical_attribute_primary", "blunt", "physical attack attributes"),
            ("weapons.csv", "reinforce_type", "99999", "missing reinforce types"),
            ("weapons.csv", "curve_id_poison", "99999", "used curve 99999"),
            ("calc_correct.csv", "multiplier", "-1", "non-monotonic"),
        ):
            with self.subTest(field=field):
                def read(path: Path) -> list[dict[str, str]]:
                    rows = read_csv(path)
                    if path.name == filename:
                        target = rows[0] if filename == "weapons.csv" else next(
                            row for row in rows
                            if row["curve_id"] == used_curve and row["stat_value"] == "100"
                        )
                        target[field] = invalid
                    return rows

                with patch.object(validation, "read_csv", side_effect=read):
                    issues = validation.validate_profile_snapshot(data_dir, "convergence")
                self.assertTrue(any(message in issue.message for issue in issues), issues)

        def zero_base(path: Path) -> list[dict[str, str]]:
            rows = read_csv(path)
            if path.name == "weapons.csv":
                rows[0].update({f"base_{damage}": "0" for damage in validation.DAMAGE_TYPES})
            return rows

        with patch.object(validation, "read_csv", side_effect=zero_base):
            issues = validation.validate_profile_snapshot(data_dir, "convergence")
        self.assertTrue(any("zero base damage" in issue.message for issue in issues))

    def test_tracked_weapon_reference_provenance_is_checked(self) -> None:
        root = Path(__file__).resolve().parents[2]
        manifest = json.loads(
            (root / "data/profiles/convergence/manifest.json").read_text(encoding="utf-8")
        )
        profile = profile_definition("convergence")
        self.assertEqual(validate_profile_source_provenance(manifest, profile), [])
        record = next(source for source in manifest["sources"] if source["kind"] == "weaponAvailability")
        record["sha256"] = "0" * 64
        issues = validate_profile_source_provenance(manifest, profile)
        self.assertIn("provenance", issues[0].message)
        manifest["sources"] = [
            source for source in manifest["sources"] if source["kind"] != "weaponAvailability"
        ]
        issues = validate_profile_source_provenance(manifest, profile)
        self.assertIn("provenance is missing", issues[0].message)

    def test_tracked_vanilla_workbook_provenance_is_checked(self) -> None:
        root = Path(__file__).resolve().parents[2]
        manifest = json.loads(
            (root / "data/phase1/manifest.json").read_text(encoding="utf-8")
        )
        profile = profile_definition("vanilla")
        self.assertEqual(validate_profile_source_provenance(manifest, profile), [])
        record = next(source for source in manifest["sources"] if source["kind"] == "workbook")
        self.assertEqual(record["path"], MOTION_WORKBOOK_NAME)

        record["sha256"] = "0" * 64
        issues = validate_profile_source_provenance(manifest, profile)
        self.assertTrue(any("tracked Vanilla workbook" in issue.message for issue in issues))

        manifest = json.loads(
            (root / "data/phase1/manifest.json").read_text(encoding="utf-8")
        )
        record = next(source for source in manifest["sources"] if source["kind"] == "workbook")
        record["path"] = "untrusted-workbook.xlsx"
        issues = validate_profile_source_provenance(manifest, profile)
        self.assertTrue(any("tracked Vanilla workbook" in issue.message for issue in issues))

    def test_compact_permissions_reject_missing_and_malformed_fields(self) -> None:
        weapons = [{"can_change_aow": "1"}, {"can_change_aow": "0"}]
        ashes = [{"valid_affinities": "Standard|Night", "valid_weapon_types": "ThrustingShield"}]
        self.assertEqual(validate_aow_compatibility(weapons, ashes), [])
        for invalid in ({}, {"can_change_aow": "2"}):
            self.assertIn("permissions", validate_aow_compatibility([invalid], ashes)[0].message)
        self.assertIn("permissions", validate_aow_compatibility(weapons, [{}])[0].message)
        for field in ("valid_affinities", "valid_weapon_types"):
            for value in ("Standard|", "Standard|Standard", " Standard"):
                with self.subTest(field=field, value=value):
                    self.assertIn("malformed", validate_aow_compatibility(weapons, [{**ashes[0], field: value}])[0].message)

    def test_compact_permissions_reject_unknown_profile_tokens(self) -> None:
        weapons = [{"can_change_aow": "1", "weapon_type_keys": "ThrustingShield"}]
        ashes = [{"valid_affinities": "Standard|Cold", "valid_weapon_types": "ThrustingShield"}]
        issues = validate_aow_compatibility(
            weapons,
            ashes,
            expected_affinities={"Standard", "Night"},
        )
        self.assertIn("unknown affinity", issues[0].message)


    def test_used_curve_must_cover_effective_strength_through_148(self) -> None:
        rows = curve_rows()
        rows.pop(120)

        issues = validate_used_calc_correct_curves(rows, {1})

        self.assertEqual(len(issues), 1)
        self.assertIn("missing stat values: [120]", issues[0].message)

    def test_used_curve_decrease_after_99_is_rejected(self) -> None:
        rows = curve_rows()
        rows[120]["multiplier"] = "118"

        issues = validate_used_calc_correct_curves(rows, {1})

        self.assertEqual(len(issues), 1)
        self.assertIn("non-monotonic used curves", issues[0].message)

    def test_fixed_status_requires_flat_status_curves_and_no_overlays(self) -> None:
        zero = [{"curve_id": "28", "stat_value": str(stat), "multiplier": "0"} for stat in range(149)]
        weapon = {"weapon_id": "1", "arc_scaling": "0.6", "curve_id_blood": "28",
                  "curve_id_poison": "28", "curve_id_sleep": "28", "curve_id_madness": "28"}
        passive = {"weapon_id": "1", "bleed": "68", "poison": "0", "sleep": "0", "madness": "0"}
        self.assertEqual(validation.validate_fixed_status_rule([weapon], [passive], zero, []), [])

        scaling = validation.validate_fixed_status_rule(
            [{**weapon, "curve_id_blood": "1"}], [passive], zero + curve_rows(), []
        )
        self.assertIn("arcane-scaling status weapons: ['1']", scaling[0].message)
        overlays = validation.validate_fixed_status_rule([weapon], [passive], zero, [{"weapon_id": "1"}])
        self.assertIn("upgrade status overlays", overlays[0].message)

    def test_class_catalog_rejects_levels_that_are_not_rune_levels(self) -> None:
        row = {"class_id": "3000", "name": "Retainer", "base_level": "9", "vig": "15", "mnd": "10",
               "end": "11", "str": "14", "dex": "13", "int": "9", "fai": "9", "arc": "7"}
        self.assertEqual(validation.validate_class_catalog([row]), [])
        for broken in ({**row, "base_level": "10"}, {**row, "arc": "0"}, {**row, "name": " "},
                       {**row, "name": "Custom stats"}):
            with self.subTest(broken=broken):
                self.assertEqual(len(validation.validate_class_catalog([broken])), 1)
        self.assertEqual(len(validation.validate_class_catalog([row, row])), 1)


if __name__ == "__main__":
    unittest.main()
