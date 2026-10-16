from __future__ import annotations

import tempfile
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase1 import phase1_dump
from tools.phase1.aow_effect_graph import OUTPUT_FIELDS, _classify_direct_effect
from tools.phase1.derive_phase1_raw_extras import build_passive_effect_coverage, build_weapon_passives
from tools.phase1.extract_motion_workbook import (
    AOW_ATTACK_FIELDS,
    NATIVE_SKILL_ATTACK_FIELDS,
    ROUTE_ASSIGNMENT_FIELDS,
)
from tools.phase1.phase1_dump import (
    MAX_EFFECTIVE_STRENGTH,
    WEAPON_MODEL_FIELDS,
    build_weapon_rows,
    build_aow_rows,
    canonical_gem_rows,
    expand_calc_correct_curve,
    iter_param_rows,
    read_csv,
)
from tools.phase1.profiles import PROFILES, discover_witchybnd

ROOT = Path(__file__).resolve().parents[2]


def weapon_param_row(**fields: str) -> dict[str, str]:
    """A player weapon PARAM row with every required model field present."""
    return {
        **dict.fromkeys(WEAPON_MODEL_FIELDS, "0"),
        "sortId": "1",
        "isNormalAttackType": "1",
        "staminaConsumptionRate": "1",
        **fields,
    }


def build_rows(rows: list[dict[str, str]], **overrides: object) -> list[dict[str, object]]:
    arguments: dict = {
        "name_map": {},
        "sword_art_name_map": {},
        "wep_type_name_map": {5: "Greatsword"},
        "weapon_type_keys_by_id": {5: ("SwordLarge",)},
        "affinity_by_slot": {0: "Standard", 1: "Heavy"},
        "max_level_by_type": {0: 25, 100: 25},
        "player_weapon_data": {},
        "use_workbook_weapon_metadata": False,
        "weapon_name_overrides": {},
        "somber_reinforce_types": frozenset(),
        **overrides,
    }
    return build_weapon_rows(rows, **arguments)


class Phase1DumpTests(unittest.TestCase):
    def test_numeric_fields_reject_lossy_or_nonfinite_values(self) -> None:
        for raw in ("1.9", "NaN", "Infinity", "-Infinity", "", "1.0000000000000000001"):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                phase1_dump.to_int({"value": raw}, "value")
        for raw in ("NaN", "Infinity", "-Infinity", "1e309", ""):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                phase1_dump.to_float({"value": raw}, "value")
        self.assertEqual(phase1_dump.to_int({"value": "9007199254740993"}, "value"), 9007199254740993)
        self.assertEqual(phase1_dump.to_int({"value": "1.0"}, "value"), 1)
        self.assertEqual(phase1_dump.to_int({}, "optional", 6), 6)
        self.assertEqual(phase1_dump.to_float({}, "optional", 1.0), 1.0)

    def test_required_model_fields_use_declared_defaults_only(self) -> None:
        fields = ("physicsAtkRate", "magicAtkRate", "fireAtkRate", "thunderAtkRate", "darkAtkRate",
                  "correctStrengthRate", "correctAgilityRate", "correctMagicRate",
                  "correctFaithRate", "correctLuckRate", "baseAtkRate")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "reinforce.xml"
            declarations = "".join(f'<field name="{name}" defaultValue="1" />' for name in fields)
            path.write_text(f'<param><fields>{declarations}</fields><rows><row id="0" /></rows></param>')
            rows = list(iter_param_rows(path, apply_defaults=True))
            output, _ = phase1_dump.build_reinforce_rows(rows)
            self.assertEqual(output[0]["physical_damage_mult"], 1.0)
            del rows[0]["physicsAtkRate"]
            with self.assertRaisesRegex(ValueError, "missing physicsAtkRate"):
                phase1_dump.build_reinforce_rows(rows)
            path.write_text('<param><rows><row /></rows></param>')
            with self.assertRaisesRegex(ValueError, "missing id"):
                list(iter_param_rows(path))
        with self.assertRaisesRegex(ValueError, "missing correctType_Physics"):
            phase1_dump.derive_damage_curve_ids({"id": "1"})
        with self.assertRaisesRegex(ValueError, "missing isStrengthCorrect_byPhysics"):
            phase1_dump.build_attack_element_rows([{"id": "1"}])

    def test_witchybnd_arguments_and_child_errors(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "path with spaces & an apostrophe's.py"
            marker = target.with_suffix(".ok")
            target.write_text(
                "from pathlib import Path\nPath(__file__).with_suffix('.ok').write_text('ok')\n",
                encoding="utf-8",
            )
            phase1_dump.run_witchybnd(Path(sys.executable), target)
            self.assertEqual(marker.read_text(), "ok")
            target.write_text("import sys\nsys.exit('child diagnostic')\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "child diagnostic"):
                phase1_dump.run_witchybnd(Path(sys.executable), target)

    def test_extraction_never_reuses_xml_from_another_source_or_tool(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workdir = root / "work"
            stale = workdir / "regulation-bin"
            stale.mkdir(parents=True)
            params = (phase1_dump.WEAPON_PARAM, phase1_dump.REINFORCE_PARAM,
                      phase1_dump.CALC_CORRECT_PARAM, phase1_dump.ATTACK_ELEMENT_PARAM,
                      phase1_dump.AOW_PARAM, phase1_dump.SPEFFECT_PARAM)
            for param in params:
                (stale / f"{param}.xml").write_text("stale", encoding="utf-8")
            source = root / "source.bin"
            tool = root / "WitchyBND.exe"

            def witchy(_tool: Path, target: Path) -> None:
                if target.name == "regulation.bin":
                    unpacked = target.parent / "regulation-bin"
                    unpacked.mkdir()
                    for param in params:
                        (unpacked / param).write_bytes(target.read_bytes())
                else:
                    Path(f"{target}.xml").write_text(
                        f'<param><row id="{target.read_text()}" /></param>', encoding="utf-8",
                    )

            with (patch.object(phase1_dump, "run_witchybnd", side_effect=witchy),
                  patch.object(phase1_dump, "load_weapon_name_map", return_value={}),
                  patch.object(phase1_dump, "load_param_name_map", return_value={}),
                  patch.object(phase1_dump, "load_wep_type_name_map", return_value={}),
                  patch.object(phase1_dump, "discover_gem_mount_fields", return_value=())):
                contexts = []
                for content in ("1", "2", "2"):
                    source.write_text(content, encoding="utf-8")
                    contexts.append(phase1_dump.load_regulation_context(source, tool, workdir))
                    self.assertEqual(contexts[-1].weapon_rows[0]["id"], content)
                self.assertEqual(len({context.workdir for context in contexts}), 3)
                self.assertEqual((stale / f"{params[0]}.xml").read_text(), "stale")

    def test_gem_defaults_are_profile_specific_and_overrides_win(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "gem.xml"
            path.write_text(
                "<param><fields>"
                '<field name="configurableWepAttr00" defaultValue="1" />'
                '<field name="configurableWepAttr11" defaultValue="1" />'
                '<field name="configurableWepAttr12" defaultValue="1" />'
                '<field name="gemMountType" defaultValue="2" />'
                '</fields><rows><row id="21400" name="Ash of War: Flaming Strike" '
                'sortId="354000" iconId="8342" swordArtsParamId="214" '
                'configurableWepAttr12="0" canMountWep_ThrustingShield="1" /></rows></param>',
                encoding="utf-8",
            )
            gems = list(iter_param_rows(path, apply_defaults=True))
            rows = build_aow_rows(gems, {}, {}, {0: "Standard", 11: "Night", 12: "Lava"})
            self.assertEqual(rows[0]["valid_affinities"], "Standard|Night")
            selected = list(iter_param_rows(path, default_fields=("gemMountType",)))[0]
            self.assertEqual(selected["gemMountType"], "2")
            self.assertNotIn("configurableWepAttr11", selected)

    def test_native_only_gems_are_not_transferable_ashes(self) -> None:
        gems = [
            {
                "id": str(skill),
                "name": "Ash of War: Native",
                "swordArtsParamId": str(skill),
                "sortId": "999999",
                "iconId": "0",
            }
            for skill in (117, 223, 303)
        ]
        real = {
            "id": "21400",
            "name": "Ash of War: Flaming Strike",
            "swordArtsParamId": "214",
            "sortId": "354000",
            "iconId": "8342",
        }
        self.assertEqual(canonical_gem_rows([*gems, real]), {214: real})
        self.assertEqual(build_aow_rows(gems, {}, {}, {0: "Standard"}), [])

    def test_versioned_name_recovers_an_unnamed_weapon_series(self) -> None:
        standard = weapon_param_row(
            id="64530000", sortId="1750000", originEquipWep="64530000", wepType="5",
            attackBasePhysics="115",
        )
        heavy = {**standard, "id": "64530100", "sortId": "1750001", "reinforceTypeId": "100",
                 "attackBasePhysics": "110"}

        rows = build_rows([standard, heavy], weapon_name_overrides={64_530_000: "Reverse-Bladed Sword"})

        self.assertEqual(
            [(row["weapon_id"], row["name"]) for row in rows],
            [
                (64_530_000, "Reverse-Bladed Sword"),
                (64_530_100, "Reverse-Bladed Sword"),
            ],
        )

    def test_profile_somber_types_set_upgrade_path(self) -> None:
        row = weapon_param_row(
            id="2090000", name="Unique Blade", originEquipWep="2090000", reinforceTypeId="2200",
            wepType="5", attackBasePhysics="100", disableGemAttr="1", throwAtkRate="10",
        )
        rows = build_rows(
            [row], max_level_by_type={2200: 15}, somber_reinforce_types=frozenset({2200}),
        )
        self.assertEqual(rows[0]["is_somber"], 1)
        self.assertEqual(rows[0]["critical_damage_percent"], 110)
        self.assertEqual(build_rows([row], max_level_by_type={2200: 15})[0]["is_somber"], 0)

    def test_weapon_model_fields_fail_closed(self) -> None:
        row = weapon_param_row(id="2090000", name="Blade", originEquipWep="2090000", wepType="5",
                               attackBasePhysics="100")
        self.assertEqual(len(build_rows([row])), 1)
        for field in ("weight", "properStrength", "correctStrength", "attackBaseMagic",
                      "originEquipWep", "correctType_Poison", "isBlowAttackType"):
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, f"missing {field}"):
                build_rows([{key: value for key, value in row.items() if key != field}])
        with self.assertRaisesRegex(ValueError, "unknown reinforce type 300"):
            build_rows([{**row, "reinforceTypeId": "300"}])
        with self.assertRaisesRegex(ValueError, "unknown wepType 9"):
            build_rows([{**row, "wepType": "9"}])
        with self.assertRaisesRegex(ValueError, "WeaponData has no row for Blade"):
            build_rows([row], use_workbook_weapon_metadata=True)

    def test_declared_weapon_defaults_are_materialized(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "weapon.xml"
            path.write_text(
                '<param><fields><field name="attackElementCorrectId" defaultValue="16000" />'
                '<field name="iconId" defaultValue="7" /></fields>'
                '<rows><row id="1" /></rows></param>',
                encoding="utf-8",
            )
            row = list(iter_param_rows(path, default_fields=WEAPON_MODEL_FIELDS))[0]
        self.assertEqual(row["attackElementCorrectId"], "16000")
        self.assertNotIn("iconId", row)

    def test_calc_correct_keeps_only_weapon_curves(self) -> None:
        def curve(curve_id: str) -> dict[str, str]:
            values = {"id": curve_id}
            for index, stage in enumerate((0, 20, 40, 60, 99)):
                values |= {f"stageMaxVal{index}": str(stage), f"stageMaxGrowVal{index}": str(stage),
                           f"adjPt_maxGrowVal{index}": "1"}
            return values

        rows = phase1_dump.build_calc_correct_rows([curve("1"), curve("2"), curve("3")], {3, 1})
        self.assertEqual(sorted({str(row["curve_id"]) for row in rows}), ["1", "3"])
        self.assertEqual(len(rows), 2 * (MAX_EFFECTIVE_STRENGTH + 1))
        with self.assertRaisesRegex(ValueError, r"missing CalcCorrectGraph curves: \[4\]"):
            phase1_dump.build_calc_correct_rows([curve("1")], {1, 4})

    def test_paramdex_name_and_type_maps_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            tool = Path(directory) / "WitchyBND.exe"
            with self.assertRaisesRegex(FileNotFoundError, "names file"):
                phase1_dump.load_param_name_map(tool, "SwordArtsParam")
            with self.assertRaisesRegex(FileNotFoundError, "meta file"):
                phase1_dump.load_wep_type_name_map(tool)
            names = phase1_dump.paramdex_path(tool, "Names", "SwordArtsParam.txt")
            names.parent.mkdir(parents=True)
            names.write_text("100 Lion's Claw\nbad line\n200 %null%\n", encoding="utf-8")
            self.assertEqual(phase1_dump.load_param_name_map(tool, "SwordArtsParam"), {100: "Lion's Claw"})
            names.write_text("200 %null%\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "no usable entries"):
                phase1_dump.load_param_name_map(tool, "SwordArtsParam")
            meta = phase1_dump.paramdex_path(tool, "Meta", "EquipParamWeapon.xml")
            meta.parent.mkdir(parents=True)
            meta.write_text(
                '<PARAMMETA><Enums><Enum Name="OTHER" type="u8"><Option Value="1" Name="No" /></Enum>'
                '<Enum Name="WEP_TYPE" type="u16"><Option Value="0" Name="None" />'
                '<Option Value="3" Name="Straight Sword"/></Enum></Enums></PARAMMETA>',
                encoding="utf-8",
            )
            self.assertEqual(phase1_dump.load_wep_type_name_map(tool), {0: "None", 3: "Straight Sword"})
            meta.write_text("<PARAMMETA><Enums /></PARAMMETA>", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "no WEP_TYPE"):
                phase1_dump.load_wep_type_name_map(tool)

    def test_gem_mount_fields_come_from_declared_fields(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "gem.xml"
            path.write_text(
                '<param><fields><field name="canMountWep_SwordNormal" />'
                '<field name="canMountWep_Katana" /><field name="sortId" /></fields></param>',
                encoding="utf-8",
            )
            self.assertEqual(phase1_dump.discover_gem_mount_fields(path), ("SwordNormal", "Katana"))
            path.write_text('<param><fields><field name="sortId" /></fields></param>', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "canMountWep_"):
                phase1_dump.discover_gem_mount_fields(path)

    def test_merged_fmg_names_reject_conflicts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base, dlc = Path(directory) / "base.xml", Path(directory) / "dlc.xml"
            base.write_text('<fmg><entries><text id="1">Dagger</text></entries></fmg>', encoding="utf-8")
            dlc.write_text('<fmg><entries><text id="1">Dagger</text><text id="2">Axe</text></entries></fmg>',
                           encoding="utf-8")
            self.assertEqual(phase1_dump.load_merged_fmg_name_map([base, dlc], "weapon"),
                             {1: "Dagger", 2: "Axe"})
            dlc.write_text('<fmg><entries><text id="1">Knife</text></entries></fmg>', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "conflicting weapon FMG name for id=1"):
                phase1_dump.load_merged_fmg_name_map([base, dlc], "weapon")

    def test_witchybnd_discovery_requires_one_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(FileNotFoundError, "found 0"):
                discover_witchybnd(root)
            for version in ("3.9.0", "3.10.0"):
                tool = root / f"data/raw/WitchyBND-{version}/WitchyBND.exe"
                tool.parent.mkdir(parents=True)
                tool.write_bytes(b"")
                if version == "3.9.0":
                    self.assertEqual(discover_witchybnd(root), tool)
            with self.assertRaisesRegex(FileNotFoundError, "found 2"):
                discover_witchybnd(root)

    def test_profile_regulation_pins_match_tracked_snapshots(self) -> None:
        for profile in PROFILES.values():
            manifest = (ROOT / profile.output_dir / "manifest.json").read_text(encoding="utf-8")
            with self.subTest(profile=profile.id):
                self.assertIn(f'"sha256": "{profile.regulation_sha256}"', manifest)

    def test_unsupported_aow_tables_get_headers_from_extractor_schemas(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            phase1_dump.initialize_unsupported_aow_runtime_files(output)
            for name, fields in (("aow_attack_data.csv", AOW_ATTACK_FIELDS),
                                 ("native_skill_attack_data.csv", NATIVE_SKILL_ATTACK_FIELDS),
                                 ("aow_route_assignments.csv", ROUTE_ASSIGNMENT_FIELDS),
                                 ("aow_effect_data.csv", OUTPUT_FIELDS)):
                with self.subTest(name=name):
                    self.assertEqual((output / name).read_bytes(), (",".join(fields) + "\n").encode())
                    self.assertEqual(read_csv(output / name), [])

    def test_weapon_passives_reject_malformed_status_values(self) -> None:
        weapon = {"weapon_id": "1", "name": "Antspur Rapier", "affinity": "Standard"}
        effects = {10: {"poizonAttackPower": "80", "diseaseAttackPower": "50",
                        "isUseStatusAilmentAtkPowerCorrect": "1"}}
        row = build_weapon_passives(effects, [weapon], {1: [10]})[0]
        self.assertEqual((row["poison"], row["scarlet_rot"], row["poison_uses_status_correction"]),
                         ("30", "50", "1"))
        for field, value in (("bloodAttackPower", "x"), ("isUseStatusAilmentAtkPowerCorrect", "yes")):
            with self.subTest(field=field), self.assertRaises(ValueError):
                build_weapon_passives({10: {field: value}}, [weapon], {1: [10]})
        with self.assertRaisesRegex(ValueError, "missing, unclassified SpEffect IDs: 11"):
            build_passive_effect_coverage(effects, [weapon], {1: [10, 11]})

    def test_effect_classification_keeps_unsupported_payloads_explicit(self) -> None:
        effect = {"replaceSpEffectId": 0, "cycleOccurrenceSpEffectId": 0, "atkOccurrenceSpEffectId": 0,
                  "effectTargetEnemy": True, "effectTargetAttacker": False,
                  "physicsAttackPower": 0.0, "magicAttackPower": 0.0, "fireAttackPower": 0.0,
                  "thunderAttackPower": 0.0, "darkAttackPower": 0.0, "bloodAttackPower": 0.0,
                  "freezeAttackPower": 0.0, "poizonAttackPower": 0.0, "diseaseAttackPower": 0.0,
                  "sleepAttackPower": 0.0, "madnessAttackPower": 0.0, "curseAttackPower": 0.0}
        for changes, source, role, supported in (
            ({}, "attack_param", "visual_or_non_gameplay", True),
            ({"bloodAttackPower": 55.0}, "attack_param", "per_hit_status", True),
            ({"fireAttackPower": 30.0}, "attack_param", "per_hit_attack_power", False),
            ({"replaceSpEffectId": 9}, "attack_param", "replacement_or_chained", False),
            ({"effectTargetEnemy": False}, "attack_param", "self_mechanic", True),
            ({"bloodAttackPower": 55.0}, "bullet_shooter", "self_buff", True),
        ):
            with self.subTest(role=role, source=source):
                self.assertEqual(_classify_direct_effect({**effect, **changes}, source)[:2], (role, supported))

    def test_param_rows_use_real_xml_parsing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "param.xml"
            path.write_text(
                '<root xmlns="urn:test"><row id="7" name="A &amp; B"\n value="3" /></root>',
                encoding="utf-8",
            )
            self.assertEqual(
                list(iter_param_rows(path)),
                [{"id": "7", "name": "A & B", "value": "3"}],
            )

    def test_curves_cover_reachable_two_handed_strength(self) -> None:
        curve = {"id": "1"}
        for index, value in enumerate((0, 20, 40, 60, 99)):
            curve[f"stageMaxVal{index}"] = str(value)
            curve[f"stageMaxGrowVal{index}"] = str(value)
            curve[f"adjPt_maxGrowVal{index}"] = "1"

        values = expand_calc_correct_curve(curve)

        self.assertEqual(len(values), MAX_EFFECTIVE_STRENGTH + 1)
        self.assertEqual(values[99], values[MAX_EFFECTIVE_STRENGTH])

    def test_invalid_curve_math_fails_closed(self) -> None:
        curve = {"id": "9"}
        for index, value in enumerate((0, 20, 10, 60, 99)):
            curve[f"stageMaxVal{index}"] = str(value)
            curve[f"stageMaxGrowVal{index}"] = str(value)
            curve[f"adjPt_maxGrowVal{index}"] = "1"

        with self.assertRaisesRegex(ValueError, "curve 9: stage bounds"):
            expand_calc_correct_curve(curve)

    def test_explicit_constant_curve_is_supported(self) -> None:
        curve = {"id": "200"}
        for index in range(5):
            curve[f"stageMaxVal{index}"] = "0"
            curve[f"stageMaxGrowVal{index}"] = "25"
            curve[f"adjPt_maxGrowVal{index}"] = "1"

        values = expand_calc_correct_curve(curve)

        self.assertEqual(values[0], 0.0)
        self.assertEqual(values[MAX_EFFECTIVE_STRENGTH], 0.25)


if __name__ == "__main__":
    unittest.main()
