from __future__ import annotations

import contextlib
import copy
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase4 import benchmark_optimizer_phases as phases
from tools.phase4 import benchmark_workflows as workflows


class BenchmarkDriverComparisonTests(unittest.TestCase):
    BUILD_METADATA = {
        "source": {"commit": "test", "dirty": True, "fingerprint": "source-digest"},
        "compiler_variant_fingerprint": "compiler-digest",
        "compiler": {"release_profile": {"lto": "thin", "codegen-units": 1},
                     "rustc_version_verbose": "rustc test"},
    }

    def case(self, driver, name="a", timing=10.0, **extra):
        if driver is phases:
            return {"kind": "case", "name": name, **dict.fromkeys(phases.PHASE_KEYS, timing), **extra}
        return {"workflow": "affinity_watch", "horizon": name, "affinities": 1,
                "model_version": "exact-v1", "median_ms": timing, **extra}

    def run_driver(self, driver, current, baseline, *arguments, builds=None):
        with tempfile.TemporaryDirectory() as temporary:
            baseline_path = Path(temporary) / "baseline.json"
            baseline_path.write_text(json.dumps({"cases": baseline}), encoding="utf-8")
            records = ([{"kind": "metadata"}] if driver is phases else []) + current
            output = "\n".join(driver.PREFIX + json.dumps(record) for record in records)
            captured = io.StringIO()
            with (
                patch("sys.argv", ["benchmark", "--baseline", str(baseline_path), *arguments]),
                patch.object(driver.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, output, "")),
                patch.object(driver, "capture_build_metadata",
                             side_effect=builds or [self.BUILD_METADATA, self.BUILD_METADATA]),
                contextlib.redirect_stdout(captured),
            ):
                status = driver.main()
            return status, json.loads(captured.getvalue())

    def test_build_provenance_preserves_raw_samples_and_results(self):
        for driver in (phases, workflows):
            samples = [7.0, 3.0, 5.0]
            sample_key = "totalSamplesMs" if driver is phases else "samples_ms"
            case = self.case(driver, timing=5.0, results="complete ordered results")
            case[sample_key] = samples
            with self.subTest(driver=driver.__name__):
                status, report = self.run_driver(driver, [case], [case])
                self.assertEqual(status, 0)
                self.assertEqual(report["metadata"]["build"], self.BUILD_METADATA)
                self.assertEqual(report["cases"][0][sample_key], samples)
                self.assertEqual(report["cases"][0]["results"], case["results"])

    def test_source_or_compiler_change_during_run_discards_measurements(self):
        for driver in (phases, workflows):
            for field in ("source", "compiler_variant_fingerprint"):
                changed = copy.deepcopy(self.BUILD_METADATA)
                if field == "source":
                    changed["source"]["fingerprint"] = "changed-source"
                else:
                    changed[field] = "changed-compiler"
                with self.subTest(driver=driver.__name__, field=field):
                    with self.assertRaisesRegex(RuntimeError, "changed during benchmark"):
                        self.run_driver(driver, [self.case(driver)], [self.case(driver)],
                                        builds=[self.BUILD_METADATA, changed])

    def test_enforced_rejects_empty_disjoint_and_missing_baselines(self):
        for driver in (phases, workflows):
            a, b = self.case(driver), self.case(driver, "b")
            for current, previous in (([a], []), ([a], [b]), ([a, b], [a])):
                with self.subTest(driver=driver.__name__, previous=previous):
                    status, report = self.run_driver(driver, current, previous, "--fail-on-regression")
                    self.assertEqual(status, 1)
                    self.assertTrue(report["comparison"]["missing"])

    def test_advisory_reports_incomplete_coverage(self):
        for driver in (phases, workflows):
            with self.subTest(driver=driver.__name__):
                status, report = self.run_driver(driver, [self.case(driver)], [])
                self.assertEqual(status, 0)
                self.assertEqual(report["comparison"]["compared"], [])
                self.assertTrue(report["comparison"]["requested"])

    def test_full_run_rejects_missing_current_cases(self):
        for driver in (phases, workflows):
            a, b = self.case(driver), self.case(driver, "b")
            with self.subTest(driver=driver.__name__):
                status, report = self.run_driver(driver, [a], [a, b], "--fail-on-regression")
                self.assertEqual(status, 1)
                expected = ["a", "b"] if driver is phases else ["affinity_watch:a:1", "affinity_watch:b:1"]
                self.assertEqual(report["comparison"]["requested"], expected)
                self.assertEqual(report["comparison"]["requested_source"], "full_comparison_union")
                self.assertEqual(report["comparison"]["missing"], expected[1:])
                self.assertEqual(report["comparison"]["intentionally_skipped"], [])
                self.assertEqual(len(report["comparison"]["timing_only"]), 1)
                self.assertEqual(report["comparison"]["output_parity_verified"], [])

    def test_explicit_case_requires_only_requested_output(self):
        a, b = self.case(phases), self.case(phases, "b")
        status, report = self.run_driver(phases, [a], [a, b], "--case", "a", "--fail-on-regression")
        self.assertEqual(status, 0)
        self.assertEqual(report["comparison"]["requested"], ["a"])
        self.assertEqual(report["comparison"]["requested_source"], "explicit_case")
        self.assertEqual(report["comparison"]["intentionally_skipped"], ["b"])
        status, report = self.run_driver(phases, [b], [a, b], "--case", "a", "--fail-on-regression")
        self.assertEqual(status, 1)
        self.assertEqual(report["comparison"]["missing"], ["a"])

    def test_duplicate_keys_rejected_on_either_side(self):
        for driver in (phases, workflows):
            a = self.case(driver)
            for current, previous in (([a, a], [a]), ([a], [a, a])):
                with self.subTest(driver=driver.__name__, current=current):
                    with self.assertRaisesRegex(ValueError, "duplicate"):
                        self.run_driver(driver, current, previous)

    def test_nonfinite_nonpositive_timings_rejected_on_either_side(self):
        for driver in (phases, workflows):
            a = self.case(driver)
            for timing in (float("nan"), float("inf"), float("-inf"), 0, -1):
                bad = self.case(driver, timing=timing)
                for current, previous in (([bad], [a]), ([a], [bad])):
                    with self.subTest(driver=driver.__name__, timing=timing):
                        with self.assertRaisesRegex(ValueError, "finite positive"):
                            self.run_driver(driver, current, previous)

    def test_output_parity_and_mismatch(self):
        for driver in (phases, workflows):
            a = self.case(driver, results="complete rows")
            with self.subTest(driver=driver.__name__):
                status, report = self.run_driver(driver, [a], [a], "--fail-on-regression")
                self.assertEqual(status, 0)
                self.assertEqual(len(report["comparison"]["output_parity_verified"]), 1)
                self.assertEqual(report["comparison"]["timing_only"], [])
                for current in (self.case(driver, results="changed rows"), self.case(driver)):
                    with self.assertRaisesRegex(ValueError, "changed ranked results"):
                        self.run_driver(driver, [current], [a])

    def test_enforced_requires_baseline(self):
        for driver in (phases, workflows):
            with (
                self.subTest(driver=driver.__name__),
                patch("sys.argv", ["benchmark", "--fail-on-regression"]),
                patch.object(driver.subprocess, "run", side_effect=AssertionError("must reject before cargo")),
            ):
                with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as failure:
                    driver.main()
                self.assertEqual(failure.exception.code, 2)

    def test_empty_current_output_cannot_succeed(self):
        for driver in (phases, workflows):
            with self.subTest(driver=driver.__name__):
                with self.assertRaisesRegex(RuntimeError, "incomplete output|no workflow cases"):
                    self.run_driver(driver, [], [self.case(driver)], "--fail-on-regression")

    def test_timing_regression_is_enforced_and_advisory(self):
        for driver in (phases, workflows):
            old, new = self.case(driver), self.case(driver, timing=20)
            with self.subTest(driver=driver.__name__):
                status, report = self.run_driver(driver, [new], [old], "--fail-on-regression")
                self.assertEqual(status, 1)
                self.assertTrue(report["regressions"])
                status, _ = self.run_driver(driver, [new], [old])
                self.assertEqual(status, 0)

    def test_invalid_recorded_samples_cannot_be_accepted(self):
        for driver in (phases, workflows):
            key = "totalSamplesMs" if driver is phases else "samples_ms"
            for samples in ([float("nan")], [0], [-1], [], "10"):
                good, bad = self.case(driver), self.case(driver)
                bad[key] = samples
                for current, previous in (([bad], [good]), ([good], [bad])):
                    with self.subTest(driver=driver.__name__, samples=samples):
                        with self.assertRaisesRegex(ValueError, "finite positive"):
                            self.run_driver(driver, current, previous)

    def test_missing_required_median_cannot_be_accepted(self):
        for driver in (phases, workflows):
            key = "scoringMedianMs" if driver is phases else "median_ms"
            good, bad = self.case(driver), self.case(driver)
            del bad[key]
            for current, previous in (([bad], [good]), ([good], [bad])):
                with self.subTest(driver=driver.__name__):
                    with self.assertRaisesRegex(ValueError, "finite positive"):
                        self.run_driver(driver, current, previous)

    def paths_case(self, mode="no_respec"):
        return {
            "workflow": "paths", "mode": mode, "horizon": 5, "lanes": 1,
            "model_version": "exact-v1", "median_ms": 10.0,
            "timing_scope": "paths_only", "warmups": 1,
            "requests": [{"mode": mode, "levelsAhead": 5,
                          "solved": {"weaponName": "Uchigatana", "affinity": "Keen"}}],
            "results": [{"steps": [{"level": 51, "stats": {"dex": 25},
                                    "route": {"upgrade": 10, "affinity": "Keen"}}]}],
        }

    def test_paths_modes_are_distinct_cases_with_verified_outputs(self):
        cases = [self.paths_case(), self.paths_case("optimum_envelope")]
        status, report = self.run_driver(workflows, cases, cases, "--fail-on-regression")
        self.assertEqual(status, 0)
        self.assertEqual(report["comparison"]["compared"], [
            "paths:no_respec:5:1", "paths:optimum_envelope:5:1",
        ])
        self.assertEqual(report["comparison"]["output_parity_verified"], report["comparison"]["compared"])
        self.assertEqual(report["comparison"]["timing_only"], [])

    def test_paths_require_fingerprints_and_isolated_timing_contract_on_both_sides(self):
        good = self.paths_case()
        for field in ("mode", "requests", "results", "timing_scope", "warmups"):
            bad = copy.deepcopy(good)
            del bad[field]
            for current, previous in (([bad], [good]), ([good], [bad])):
                with self.subTest(field=field, current=current):
                    with self.assertRaisesRegex(ValueError, f"Paths.*{field}"):
                        self.run_driver(workflows, current, previous)

    def test_paths_reject_malformed_fingerprints(self):
        good = self.paths_case()
        for field, value in (
            ("mode", "unknown"), ("timing_scope", "search_plus_paths"), ("warmups", -1),
            ("warmups", True), ("warmups", 1.5), ("lanes", 0), ("lanes", True),
            ("requests", []), ("requests", "serialized string"), ("requests", [None]), ("requests", [{}]),
            ("results", []), ("results", "serialized string"), ("results", [None]), ("results", [{}]),
            ("results", [{"steps": []}, {"steps": []}]),
        ):
            bad = {**good, field: value}
            with self.subTest(field=field, value=value):
                with self.assertRaisesRegex(ValueError, f"Paths.*{field}"):
                    self.run_driver(workflows, [good], [bad])

    def test_paths_changed_step_allocation_or_route_rejected_despite_faster_timing(self):
        good = self.paths_case()
        for field, value in (("stats", {"dex": 26}), ("route", {"upgrade": 11, "affinity": "Keen"})):
            bad = copy.deepcopy(good)
            bad["median_ms"] = 1
            bad["results"][0]["steps"][0][field] = value
            with self.subTest(field=field):
                with self.assertRaisesRegex(ValueError, "changed ranked results"):
                    self.run_driver(workflows, [bad], [good])

    def test_paths_changed_request_rejected_before_timing_comparison(self):
        good = self.paths_case()
        bad = copy.deepcopy(good)
        bad["requests"][0]["solved"]["affinity"] = "Heavy"
        bad["median_ms"] = 1
        with self.assertRaisesRegex(ValueError, "changed normalized requests"):
            self.run_driver(workflows, [bad], [good])

    def test_paths_allow_explicit_zero_or_multiple_warmups(self):
        for warmups in (0, 2):
            case = {**self.paths_case(), "warmups": warmups}
            with self.subTest(warmups=warmups):
                status, _ = self.run_driver(workflows, [case], [case], "--fail-on-regression")
                self.assertEqual(status, 0)

    def test_paths_lane_order_is_part_of_request_and_result_parity(self):
        good = self.paths_case()
        good["lanes"] = 2
        good["requests"].append(copy.deepcopy(good["requests"][0]))
        good["requests"][1]["solved"]["weaponName"] = "Bloodhound's Fang"
        good["results"].append(copy.deepcopy(good["results"][0]))
        good["results"][1]["steps"][0]["stats"]["dex"] = 30
        for field, error in (("requests", "changed normalized requests"), ("results", "changed ranked results")):
            bad = copy.deepcopy(good)
            bad[field].reverse()
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, error):
                self.run_driver(workflows, [bad], [good])


if __name__ == "__main__":
    unittest.main()
