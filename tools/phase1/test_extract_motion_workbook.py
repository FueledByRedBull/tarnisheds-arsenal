from pathlib import Path
import unittest

from tools.phase1.extract_motion_workbook import (
    MOTION_WORKBOOK_NAME,
    extract_variant,
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


if __name__ == "__main__":
    unittest.main()
