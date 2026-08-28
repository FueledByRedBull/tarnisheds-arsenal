from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools.phase1.phase1_dump import MAX_EFFECTIVE_STRENGTH, expand_calc_correct_curve, iter_param_rows


class Phase1DumpTests(unittest.TestCase):
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
