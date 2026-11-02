import unittest

from tools.phase1.compare_skill_animations import (
    compare_attacks,
    compare_first_hit_poise,
    compare_routes,
    damaging_attacks,
    route_hits,
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

    def test_routes_compare_hit_counts_and_fall_back_to_generic_rows(self) -> None:
        def row(sheet_row: str, attack: str, variant: str = "", damaging: str = "1") -> dict[str, str]:
            return {
                "aow_id": "1",
                "aow_name": "Ash",
                "sheet_row": sheet_row,
                "atk_id": attack,
                "variant_weapon_type": variant,
                "is_damaging": damaging,
            }

        def assign(sheet_row: str, route: str, hits: str = "1") -> dict[str, str]:
            return {"aow_id": "1", "sheet_row": sheet_row, "route_id": route, "hit_count": hits}

        book = route_hits(
            [row("1", "7"), row("2", "8", "Axe"), row("3", "9", damaging="0")],
            [assign("1", "full", "2"), assign("3", "full"), assign("2", "axe")],
        )
        self.assertEqual(book[("Ash", "")], [{7: 2}])
        same = route_hits([row("1", "7"), row("2", "7")], [assign("1", "a"), assign("2", "a")])
        self.assertEqual(compare_routes(book, same), (1, 2))
        fewer = route_hits([row("1", "7")], [assign("1", "a")])
        self.assertEqual(compare_routes(book, fewer), (0, 2))

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
