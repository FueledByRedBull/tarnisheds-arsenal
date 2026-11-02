import unittest

from tools.phase1.compare_skill_animations import (
    compare_attacks,
    compare_first_hit_poise,
    damaging_attacks,
)
from tools.phase1.extract_skill_animations import NORMAL_MOVE_ANIMS


class CompareSkillAnimationsTests(unittest.TestCase):
    def test_counts_only_full_fp_damaging_rows(self) -> None:
        rows = [
            {"aow_name": "Ash", "atk_id": "1", "is_lacking_fp": "0", "is_damaging": "1"},
            {"aow_name": "Ash", "atk_id": "2", "is_lacking_fp": "1", "is_damaging": "1"},
            {"aow_name": "Ash", "atk_id": "3", "is_lacking_fp": "0", "is_damaging": "0"},
        ]
        self.assertEqual(damaging_attacks(rows), {"Ash": {1}})

    def test_reports_identical_skills_and_found_attacks(self) -> None:
        agreement = compare_attacks(
            {"Same": {1, 2}, "Partial": {3, 4}, "Workbook only": {5}},
            {"Same": {1, 2}, "Partial": {3}, "Extractor only": {6}},
        )
        self.assertEqual(agreement.skills, 4)
        self.assertEqual(agreement.exact_skills, 1)
        self.assertEqual(agreement.attacks, 5)
        self.assertEqual(agreement.found_attacks, 3)

    def test_poise_compares_the_first_contact_and_skips_unlisted_moves(self) -> None:
        empty = dict.fromkeys(NORMAL_MOVE_ANIMS, "")
        book = [
            empty | {"weapon_id": "1", "one_hand_light_poise": "3 + 3", "one_hand_heavy_poise": "(4 Projectile)"},
            empty | {"weapon_id": "2", "one_hand_light_poise": "5.0"},
        ]
        extracted = [
            empty | {"weapon_id": "1", "one_hand_light_poise": "3.0 + 7.0", "one_hand_heavy_poise": "(4.0 Bullet)"},
            empty | {"weapon_id": "2", "one_hand_light_poise": "6.0"},
        ]
        result = compare_first_hit_poise(book, extracted)
        self.assertEqual(result["one_hand_light_poise"], (1, 2))
        self.assertEqual(result["one_hand_heavy_poise"], (1, 1))
        self.assertEqual(result["two_hand_light_poise"], (0, 0))


if __name__ == "__main__":
    unittest.main()
