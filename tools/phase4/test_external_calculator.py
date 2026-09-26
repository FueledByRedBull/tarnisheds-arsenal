from __future__ import annotations

import math
import unittest

from tools.phase4.validate_external_calculator import (
    ComparisonCase,
    _raw_transferable_gems,
    compare_cases,
    compare_exhaustive_result,
    compare_raw_compatibility,
    floor_status,
    normalize_name,
    sample_cases,
)


def catalog() -> list[dict[str, object]]:
    return [
        {
            "name": "Test Blade",
            "affinity": affinity,
            "maxUpgrade": 25,
            "requirements": {"str": 10, "dex": 10, "int": 10, "fai": 10, "arc": 10},
            "supported": True,
            "ashes": [{"name": "Quickstep", "unbuffed": True}],
        }
        for affinity in ("Standard", "Heavy", "Keen")
    ] + [
        {
            "name": "Test Spear",
            "affinity": affinity,
            "maxUpgrade": 25,
            "requirements": {"str": 10, "dex": 10, "int": 10, "fai": 10, "arc": 10},
            "supported": True,
            "ashes": [{"name": "Quickstep", "unbuffed": True}],
        }
        for affinity in ("Standard", "Heavy", "Keen")
    ]


def external() -> list[dict[str, object]]:
    return [
        {
            "weaponName": "Test Blade",
            "affinityId": affinity_id,
            "maxUpgrade": 25,
            "requirements": {"str": 10, "dex": 10, "int": 10, "fai": 10, "arc": 10},
        }
        for affinity_id in (0, 1, 2)
    ] + [
        {
            "weaponName": "Test Spear",
            "affinityId": affinity_id,
            "maxUpgrade": 25,
            "requirements": {"str": 10, "dex": 10, "int": 10, "fai": 10, "arc": 10},
        }
        for affinity_id in (0, 1, 2)
    ]


class ExternalCalculatorTests(unittest.TestCase):
    def test_raw_transferable_gems_excludes_native_and_menu_rows(self) -> None:
        rows = {
            1: {"sortId": 999999, "iconId": 1, "swordArtsParamId": 10},
            2: {"sortId": 1, "iconId": 0, "swordArtsParamId": 11},
            3: {"sortId": 1, "iconId": 1, "swordArtsParamId": -1},
            4: {"sortId": 1, "iconId": 1, "swordArtsParamId": 12},
        }
        self.assertEqual(set(_raw_transferable_gems(rows)), {12})

    def test_raw_compatibility_includes_negative_pairs(self) -> None:
        expectation = {
            "weapon_ids": {1, 2},
            "transferable_ash_ids": {10, 20},
            "expected_transfer": {(1, 10), (2, 20)},
            "expected_native": {(1, 101)},
            "expected_can_change": {1},
        }
        catalog = [
            {
                "id": 1,
                "canChangeAow": True,
                "ashes": [{"id": 10}],
                "nativeSkill": {"id": 101, "compatible": True},
            },
            {
                "id": 2,
                "canChangeAow": False,
                "ashes": [{"id": 20}],
                "nativeSkill": {"id": 102, "compatible": False},
            },
        ]
        report = compare_raw_compatibility(catalog, expectation)
        self.assertTrue(report["passed"])
        self.assertEqual(report["totalCandidatePairs"], 4)
        self.assertEqual(report["negativePairsRejected"], 2)

    def test_exhaustive_result_requires_profile_caps_and_all_four_poses(self) -> None:
        expectation = {
            "expected_transfer": {(1, 10)},
            "expected_native": {(1, 101)},
        }
        result = {
            "profile": "vanilla",
            "transferablePairs": 1,
            "nativePairs": 1,
            "transferEvaluations": 4,
            "nativeEvaluations": 4,
            "arChecks": 8,
            "unsupportedArEvaluations": 0,
            "unsupportedWeaponEvaluations": 0,
            "statusChecks": 8,
            "nonUnitWeaponInfluenceWeapons": 0,
            "fixedStats": [99, 99, 99, 99, 99],
            "upgradeCaps": {"standard": 25, "somber": 10, "separate": True},
            "routes": {
                "damageCapability": True,
                "capability": True,
                "mappedTransferEvaluations": 4,
                "unmappedTransferEvaluations": 0,
                "unsupportedTransferEvaluations": 0,
                "mappedNativeEvaluations": 4,
                "unmappedNativeEvaluations": 0,
                "unsupportedNativeEvaluations": 0,
                "materializedRoutes": 2,
                "materializedHits": 2,
                "unsupportedEffectEvaluations": 0,
                "routeErrorEvaluations": 0,
            },
        }
        report = compare_exhaustive_result("vanilla", result, expectation)
        self.assertTrue(report["passed"])
        self.assertEqual(report["evaluations"], 8)

    def test_name_normalization_handles_game_text(self) -> None:
        self.assertEqual(normalize_name("Malenia’s Épée"), normalize_name("malenia's epee"))

    def test_status_comparison_uses_integer_floor(self) -> None:
        self.assertEqual(floor_status(12.99), 12)
        self.assertEqual(floor_status(13.0), 13)

    def test_sampling_is_reproducible_and_covers_hands_phases(self) -> None:
        first = sample_cases(external(), catalog(), count=2, seed=7)
        second = sample_cases(external(), catalog(), count=2, seed=7)
        self.assertEqual(first, second)
        selected, cases = first
        self.assertEqual(len(selected), 2)
        self.assertEqual(len(cases), 12)
        self.assertEqual(cases[0].payload()["externalIndex"], cases[0].external_index)
        for weapon in selected:
            weapon_cases = [case for case in cases if case.weapon_name == weapon]
            self.assertEqual(len(weapon_cases), 6)
            upgrades = {case.upgrade for case in weapon_cases}
            self.assertIn(0, upgrades)
            self.assertIn(25, upgrades)
            self.assertTrue(any(0 < upgrade < 25 for upgrade in upgrades))
        self.assertEqual({case.two_handing for case in cases}, {False, True})
        self.assertTrue(all(case.aow_name == "Quickstep" for case in cases))

    def test_sampling_rejects_too_few_unique_names(self) -> None:
        with self.assertRaisesRegex(ValueError, "usable external weapon names"):
            sample_cases(external(), catalog(), count=3, seed=7)

    def test_sampling_always_includes_the_32_bloodfiend_bleed_boundaries(self) -> None:
        affinities = ("Heavy", "Keen", "Quality", "Fire", "Flame Art", "Lightning", "Sacred", "Magic")
        local = catalog() + [
            {"name": "Bloodfiend's Fork", "affinity": affinity, "maxUpgrade": 25}
            for affinity in affinities
        ]
        reference = external() + [
            {"weaponName": "Bloodfiend's Fork", "affinityId": index, "maxUpgrade": 25}
            for index in range(1, 9)
        ]
        selected, cases = sample_cases(reference, local, count=1, seed=7)
        boundaries = [case for case in cases if case.sample_weapon == 0]
        self.assertEqual(len(selected), 1)
        self.assertEqual(len(boundaries), 32)
        self.assertEqual([case.case_id for case in cases], list(range(1, 39)))
        expected = {
            (affinity, upgrade, arc, two_handing)
            for affinity, other_upgrade, other_arc in (
                ("Heavy", 14, 99), ("Keen", 23, 99), ("Quality", 7, 99), ("Fire", 1, 99),
                ("Flame Art", 7, 45), ("Lightning", 14, 99), ("Sacred", 23, 99), ("Magic", 7, 99),
            )
            for upgrade, arc in ((25, 50), (other_upgrade, other_arc))
            for two_handing in (False, True)
        }
        self.assertEqual(
            {(case.affinity, case.upgrade, case.stats["arc"], case.two_handing) for case in boundaries},
            expected,
        )
        self.assertTrue(all(case.aow_name is None for case in boundaries))
        self.assertTrue(all(case.weapon_name == "Bloodfiend's Fork" for case in boundaries))

    def test_comparison_rejects_incomplete_results(self) -> None:
        case = ComparisonCase(
            case_id=1,
            sample_weapon=1,
            external_index=0,
            weapon_name="Test Blade",
            affinity="Standard",
            aow_name=None,
            upgrade=0,
            stats={stat: 10 for stat in ("str", "dex", "int", "fai", "arc")},
            two_handing=False,
        )
        with self.assertRaisesRegex(ValueError, "result count mismatch"):
            compare_cases([case], [], [], tolerance=0.001)

    def test_comparison_rejects_missing_malformed_and_nan_numbers(self) -> None:
        case = ComparisonCase(
            case_id=1,
            sample_weapon=1,
            external_index=0,
            weapon_name="Test Blade",
            affinity="Standard",
            aow_name=None,
            upgrade=0,
            stats={stat: 10 for stat in ("str", "dex", "int", "fai", "arc")},
            two_handing=False,
        )
        local = {
            "weapon": "Test Blade",
            "affinity": "Standard",
            "upgrade": 0,
            "stats": [10, 10, 10, 10, 10],
            "arComponents": [1.0] * 5,
            "baseStatus": [0.0] * 7,
        }
        external = {"attackPower": [1.0] * 5 + [0.0] * 7, "ineffectiveAttributes": []}
        mutations = (
            ("missing AR", local, external | {"attackPower": [1.0] * 4 + [0.0] * 7}),
            ("malformed AR", local | {"arComponents": [1.0] * 4}, external),
            ("NaN AR", local | {"arComponents": [math.nan] * 5}, external),
            ("NaN status", local | {"baseStatus": [math.nan] * 7}, external),
        )
        for label, bad_local, bad_external in mutations:
            with self.subTest(label=label):
                checks, _, failures = compare_cases(
                    [case], [bad_local], [bad_external], tolerance=0.001
                )
                self.assertTrue(failures)
                self.assertGreater(
                    checks["attackRating"]["failed"] + checks["basePassiveStatus"]["failed"],
                    0,
                )

    def test_skill_check_reports_null_skill_as_not_applicable(self) -> None:
        case = ComparisonCase(
            case_id=1,
            sample_weapon=1,
            external_index=0,
            weapon_name="Test Blade",
            affinity="Standard",
            aow_name=None,
            upgrade=0,
            stats={stat: 10 for stat in ("str", "dex", "int", "fai", "arc")},
            two_handing=False,
        )
        local = {
            "weapon": "Test Blade",
            "affinity": "Standard",
            "upgrade": 0,
            "stats": [10, 10, 10, 10, 10],
            "arComponents": [1.0] * 5,
            "baseStatus": [0.0] * 7,
        }
        external = {"attackPower": [1.0] * 5 + [0.0] * 7, "ineffectiveAttributes": []}
        checks, _, failures = compare_cases([case], [local], [external], tolerance=0.001)
        self.assertEqual(failures, [])
        self.assertEqual(checks["skillIdentity"], {"checked": 0, "passed": 0, "failed": 0, "notApplicable": 1})


if __name__ == "__main__":
    unittest.main()
