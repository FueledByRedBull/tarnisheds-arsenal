from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from tools.phase4.benchmark_optimizer_phases import compare_baseline


class BenchmarkComparisonTests(unittest.TestCase):
    def test_timing_improvements_cannot_hide_changed_ranked_results(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            baseline = Path(temporary) / "baseline.json"
            original = {"name": "open-ar", "results": "same rows", "totalMedianMs": 100}
            baseline.write_text(json.dumps({"cases": [original]}), encoding="utf-8")
            self.assertEqual(compare_baseline([{**original, "totalMedianMs": 50}], baseline, 20), [])
            slower = compare_baseline([{**original, "totalMedianMs": 150}], baseline, 20)
            self.assertEqual(slower[0]["regressionPercent"], 50)
            with self.assertRaisesRegex(ValueError, "changed ranked results"):
                compare_baseline([{**original, "results": "wrong rows", "totalMedianMs": 1}], baseline, 20)


if __name__ == "__main__":
    unittest.main()
