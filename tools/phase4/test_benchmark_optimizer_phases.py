from __future__ import annotations

import json
import argparse
import os
import sys
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

from tools.phase4.benchmark_optimizer_phases import PHASE_KEYS, compare_baseline, main, thread_policy


class BenchmarkComparisonTests(unittest.TestCase):
    def test_thread_policy_accepts_only_default_or_positive_counts(self) -> None:
        for value in ("default", "1", "2", "4"):
            self.assertEqual(thread_policy(value), value)
        for value in ("0", "-1", "1.5", "auto", ""):
            with self.subTest(value=value), self.assertRaises(argparse.ArgumentTypeError):
                thread_policy(value)

    def test_cli_thread_policy_controls_the_benchmark_environment(self) -> None:
        for arguments, expected in (([], "8"), (["--threads=default"], None), (["--threads=2"], "2")):
            with (
                self.subTest(arguments=arguments),
                patch.object(sys, "argv", ["benchmark", *arguments]),
                patch.dict(os.environ, {"RAYON_NUM_THREADS": "8"}),
                patch("tools.phase4.benchmark_optimizer_phases.capture_build_metadata"),
                patch("tools.phase4.benchmark_optimizer_phases.subprocess.run", side_effect=RuntimeError("stop before benchmark")) as run,
                self.assertRaisesRegex(RuntimeError, "stop before benchmark"),
            ):
                main()
            self.assertEqual(run.call_args.kwargs["env"].get("RAYON_NUM_THREADS"), expected)

    def test_timing_improvements_cannot_hide_changed_ranked_results(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            baseline = Path(temporary) / "baseline.json"
            original = {"name": "open-ar", "results": "same rows", **dict.fromkeys(PHASE_KEYS, 100)}
            baseline.write_text(json.dumps({"cases": [original]}), encoding="utf-8")
            self.assertEqual(compare_baseline([{**original, "totalMedianMs": 50}], baseline, 20), [])
            slower = compare_baseline([{**original, "totalMedianMs": 150}], baseline, 20)
            self.assertEqual(slower[0]["regressionPercent"], 50)
            with self.assertRaisesRegex(ValueError, "changed ranked results"):
                compare_baseline([{**original, "results": "wrong rows", "totalMedianMs": 1}], baseline, 20)


if __name__ == "__main__":
    unittest.main()
