from __future__ import annotations

import tempfile
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase1 import phase1_dump

from tools.phase1.phase1_dump import (
    MAX_EFFECTIVE_STRENGTH,
    build_weapon_rows,
    build_aow_rows,
    canonical_gem_rows,
    expand_calc_correct_curve,
    iter_param_rows,
)


class Phase1DumpTests(unittest.TestCase):
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
                  patch.object(phase1_dump, "load_wep_type_name_map", return_value={})):
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

    def test_mounting_permission_is_independent_of_infusion(self) -> None:
        from tools.phase1.derive_phase1_extras import build_aow_affinity_compat

        weapon = {"name": "Shortbow", "affinity": "Standard", "weapon_type_keys": "BowSmall",
                  "can_change_aow": "1", "disable_gem_attr": "1"}
        ash = {"aow_id": "1", "name": "Barrage", "valid_affinities": "Standard",
               "valid_weapon_types": "BowSmall"}
        rows = build_aow_affinity_compat([weapon], [ash])
        self.assertEqual(rows[0]["sample_weapon_names"], "Shortbow")
        self.assertEqual(rows[0]["weapon_count"], "1")
        for invalid in ({**weapon, "can_change_aow": "0", "disable_gem_attr": "0"},
                        {**weapon, "affinity": "Night"}, {**weapon, "weapon_type_keys": "Bow"}):
            self.assertEqual(build_aow_affinity_compat([invalid], [ash]), [])

    def test_versioned_name_recovers_an_unnamed_weapon_series(self) -> None:
        standard = {
            "id": "64530000",
            "sortId": "1750000",
            "originEquipWep": "64530000",
            "reinforceTypeId": "0",
            "wepType": "92",
            "attackBasePhysics": "115",
        }
        heavy = {
            **standard,
            "id": "64530100",
            "sortId": "1750001",
            "reinforceTypeId": "100",
            "attackBasePhysics": "110",
        }

        rows = build_weapon_rows(
            [standard, heavy],
            {64_530_000: "Reverse-Bladed Sword"},
            {},
            {92: "Reverse-hand Blade"},
            {92: ("ReverseHandSword",)},
            {0: "Standard", 1: "Heavy"},
            {0: 25, 100: 25},
            {},
            use_workbook_weapon_metadata=False,
            allow_param_weapon_names=True,
            weapon_name_overrides={64_530_000: "Reverse-Bladed Sword"},
        )

        self.assertEqual(
            [(row["weapon_id"], row["name"]) for row in rows],
            [
                (64_530_000, "Reverse-Bladed Sword"),
                (64_530_100, "Reverse-Bladed Sword"),
            ],
        )

    def test_profile_somber_types_override_upgrade_cap_heuristic(self) -> None:
        row = {
            "id": "2090000",
            "name": "Unique Blade",
            "sortId": "1",
            "originEquipWep": "2090000",
            "reinforceTypeId": "2200",
            "wepType": "5",
            "attackBasePhysics": "100",
            "disableGemAttr": "1",
        }
        rows = build_weapon_rows(
            [row],
            {2090000: "Unique Blade"},
            {},
            {5: "Greatsword"},
            {5: ("SwordLarge",)},
            {0: "Standard"},
            {2200: 15},
            {},
            use_workbook_weapon_metadata=False,
            allow_param_weapon_names=True,
            weapon_name_overrides={},
            somber_reinforce_types=frozenset({2200}),
        )
        self.assertEqual(rows[0]["is_somber"], 1)

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
