import csv
from pathlib import Path
import shutil
import tempfile
import unittest

from tools.phase1.extract_motion_workbook import (
    MOTION_WORKBOOK_NAME,
    WorkbookReader,
    build_aow_attack_data,
    build_native_skill_attack_data,
    extract_variant,
    find_matching_aow,
    load_weapon_workbook_data,
)


class MotionWorkbookTests(unittest.TestCase):
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
