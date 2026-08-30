from __future__ import annotations

import unittest

from tools.phase1.phase1_dump import MAX_EFFECTIVE_STRENGTH
from tools.phase4.validate_phase4 import validate_used_calc_correct_curves


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


if __name__ == "__main__":
    unittest.main()
