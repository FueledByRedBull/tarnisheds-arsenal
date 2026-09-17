import csv
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from tools.phase1.extract_motion_workbook import (
    MOTION_WORKBOOK_NAME,
    WorkbookReader,
    build_aow_attack_data,
    build_attack_row,
    build_native_skill_attack_data,
    extract_variant,
    find_matching_aow,
    load_weapon_workbook_data,
    load_throw_attack_ids,
    run_workbook_exports,
)


class MotionWorkbookTests(unittest.TestCase):
    def test_generated_effect_exclusions_are_unique(self) -> None:
        path = Path(__file__).resolve().parents[2] / 'data/phase1/aow_effect_exclusions.csv'
        with path.open(encoding='utf-8', newline='') as stream:
            rows = [tuple(row) for row in csv.reader(stream)]
        self.assertEqual(len(rows), len(set(rows)))

    def test_explicit_paramdex_directory_is_preserved(self) -> None:
        project_root = Path(__file__).resolve().parents[2]
        phase1_dir = project_root / 'data' / 'phase1'
        regulation_bin_dir = Path('regulation')
        paramdex_defs_dir = Path('defs')
        with (
            patch(
                'tools.phase1.extract_motion_workbook.load_bullet_attack_ids',
                return_value={1},
            ) as load_bullets,
            patch(
                'tools.phase1.extract_motion_workbook.load_throw_attack_ids',
                return_value={2},
            ) as load_throws,
            patch(
                'tools.phase1.extract_motion_workbook.build_aow_attack_data'
            ) as build_generic,
            patch(
                'tools.phase1.extract_motion_workbook.build_native_skill_attack_data'
            ) as build_native,
            patch('tools.phase1.extract_motion_workbook.build_aow_route_data'),
            patch('tools.phase1.extract_motion_workbook.build_attack_element_correct_ext'),
            patch('tools.phase1.extract_motion_workbook.read_sp_effect_sheet', return_value=[]),
            patch('tools.phase1.aow_effect_graph.build_aow_effect_graph'),
        ):
            run_workbook_exports(
                project_root,
                phase1_dir,
                regulation_bin_dir,
                paramdex_defs_dir,
            )

        load_bullets.assert_called_once_with(regulation_bin_dir, paramdex_defs_dir)
        load_throws.assert_called_once_with(regulation_bin_dir, paramdex_defs_dir)
        build_generic.assert_called_once_with(
            project_root,
            phase1_dir,
            bullet_attack_ids={1},
            throw_attack_ids={2},
        )
        build_native.assert_called_once_with(
            project_root,
            phase1_dir,
            bullet_attack_ids={1},
            throw_attack_ids={2},
        )

    @patch('tools.phase1.extract_motion_workbook.load_param_table')
    def test_throw_provenance_uses_raw_throw_flag(self, load_param_table_mock) -> None:
        load_param_table_mock.return_value = SimpleNamespace(
            rows={
                0: {'throwFlag': 2},
                1234: {'throwFlag': 1},
                1235: {'throwFlag': 2},
            }
        )

        self.assertEqual(
            load_throw_attack_ids(Path('regulation'), Path('defs')),
            {1235},
        )
        load_param_table_mock.assert_called_once_with(
            Path('regulation') / 'AtkParam_Pc.param',
            Path('defs') / 'AtkParam.xml',
            {'throwFlag'},
        )

    def test_bullet_provenance_uses_numeric_attack_id_for_fixed_damage(self) -> None:
        headers = [
            'Phys MV',
            'Magic MV',
            'Fire MV',
            'Ltng MV',
            'Holy MV',
            'AtkPhys',
            'AtkMag',
            'AtkFire',
            'AtkLtng',
            'AtkHoly',
            'isAddBaseAtk',
            'IsArrowAtk',
            'AtkId',
            'Unique Skill Weapon',
            'overwriteAttackElementCorrectId',
            'spEffectId0',
            'spEffectId1',
            'spEffectId2',
            'spEffectId3',
            'spEffectId4',
            'isDisableBothHandsAtkBonus',
            'PhysAtkAttribute',
            'Status MV',
            'Weapon Buff MV',
            'Poise Dmg MV',
            'StaminaCost',
        ]
        values_by_header = {header: '0' for header in headers}
        values_by_header.update(
            {
                'AtkMag': '270',
                'AtkId': '1234',
                'PhysAtkAttribute': 'standard',
            }
        )
        header_idx = {header: index for index, header in enumerate(headers)}
        values = [values_by_header[header] for header in headers]

        bullet_row, bullet_damaging, bullet_kind = build_attack_row(
            header_idx,
            values,
            2,
            1,
            'Opaque Skill',
            'Opaque numeric row',
            bullet_attack_ids={1234},
            throw_attack_ids={1234},
        )
        self.assertEqual(bullet_kind, 'direct')
        self.assertEqual(bullet_row['is_bullet_attack'], '1')
        self.assertEqual(bullet_row['is_throw_attack'], '1')
        self.assertTrue(bullet_damaging)

        values_by_header['AtkId'] = '1235'
        nonbullet_row, nonbullet_damaging, _ = build_attack_row(
            header_idx,
            [values_by_header[header] for header in headers],
            3,
            1,
            'Opaque Skill',
            'Opaque numeric row',
            bullet_attack_ids={1234},
            throw_attack_ids={1234},
        )
        self.assertEqual(nonbullet_row['is_bullet_attack'], '0')
        self.assertEqual(nonbullet_row['is_throw_attack'], '0')
        self.assertFalse(nonbullet_damaging)

    def test_117_weapon_moves_and_placeholder_prefix(self) -> None:
        weapons = load_weapon_workbook_data(Path("data/phase1") / MOTION_WORKBOOK_NAME)
        reverse_blade = next(weapon for weapon in weapons.values() if weapon.name == "Reverse-Bladed Sword")

        self.assertEqual(reverse_blade.move_count, 68)
        self.assertEqual(reverse_blade.base_poise, 5.0)
        self.assertEqual(reverse_blade.one_hand_light_poise, "5.0")
        self.assertEqual(reverse_blade.two_hand_light_poise, "3 + 3")
        self.assertEqual(extract_variant("[Placeholder] Muleta"), "")

    def test_generic_and_native_skill_outputs_separate_unique_weapon_rows(self) -> None:
        project_root = Path(__file__).resolve().parents[2]
        source_phase1 = project_root / "data" / "phase1"
        with (source_phase1 / "aow.csv").open(encoding="utf-8", newline="") as handle:
            aow_rows = list(csv.DictReader(handle))
        ordered_names = sorted((row["name"] for row in aow_rows), key=len, reverse=True)
        workbook = WorkbookReader(source_phase1 / MOTION_WORKBOOK_NAME)
        try:
            sheet = workbook.read_sheet("Ashes of War Attack Data")
            header_idx = {header: index for index, header in enumerate(sheet.headers)}
            unique_generic_sheet_rows = {
                str(row_idx)
                for row_idx, values in enumerate(sheet.rows, start=2)
                if values[header_idx["Unique Skill Weapon"]].strip()
                and find_matching_aow(values[header_idx["Name"]].strip(), ordered_names)
            }
        finally:
            workbook.close()

        with tempfile.TemporaryDirectory() as temp_dir:
            phase1_dir = Path(temp_dir)
            for filename in ("aow.csv", "weapons.csv"):
                shutil.copyfile(source_phase1 / filename, phase1_dir / filename)
            build_aow_attack_data(project_root, phase1_dir)
            build_native_skill_attack_data(project_root, phase1_dir)

            with (phase1_dir / "aow_attack_data.csv").open(encoding="utf-8", newline="") as handle:
                generic_rows = list(csv.DictReader(handle))
            with (phase1_dir / "aow_damage_coverage.csv").open(encoding="utf-8", newline="") as handle:
                coverage_rows = list(csv.DictReader(handle))
            with (phase1_dir / "native_skill_attack_data.csv").open(encoding="utf-8", newline="") as handle:
                native_rows = list(csv.DictReader(handle))

        generic_sheet_rows = {row["sheet_row"] for row in generic_rows}
        self.assertFalse(unique_generic_sheet_rows & generic_sheet_rows)
        self.assertEqual(
            sum(int(row["standard_rows"]) for row in coverage_rows),
            len(generic_rows),
        )
        self.assertEqual(
            sum(int(row["unique_collision_rows"]) for row in coverage_rows),
            len(unique_generic_sheet_rows),
        )
        self.assertEqual(
            int(next(row for row in coverage_rows if row["aow_name"] == "Spinning Weapon")["unique_collision_rows"]),
            8,
        )
        self.assertEqual(
            int(next(row for row in coverage_rows if row["aow_name"] == "Spinning Slash")["unique_collision_rows"]),
            8,
        )

        native_by_weapon = {}
        for row in native_rows:
            native_by_weapon.setdefault(row["weapon_name"], set()).add(row["raw_name"])
        self.assertEqual(
            native_by_weapon["Carian Regal Scepter"],
            {
                f"Spinning Weapon [{index}]"
                for index in range(1, 5)
            }
            | {
                f"Spinning Weapon [{index}] (Lacking FP)"
                for index in range(1, 5)
            },
        )
        self.assertEqual(
            native_by_weapon["Dragon Halberd"],
            {
                "Spinning Slash #1",
                "Spinning Slash #2 [1]",
                "Spinning Slash #2 [2]",
                "Spinning Slash #2 - Bullet",
                "Spinning Slash #2 - Bullet (Water AoE)",
                "Spinning Slash #1 (Lacking FP)",
                "Spinning Slash #2 [1] (Lacking FP)",
                "Spinning Slash #2 [2] (Lacking FP)",
            },
        )


if __name__ == "__main__":
    unittest.main()
