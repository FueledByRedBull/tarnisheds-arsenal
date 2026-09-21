#!/usr/bin/env python3
"""Run release-mode Paths and Affinity Watch regression benchmarks."""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import subprocess
import sys
from pathlib import Path
from typing import Any

if __package__:
    from .benchmark_build_metadata import capture_build_metadata
else:
    from benchmark_build_metadata import capture_build_metadata

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "apps" / "desktop" / "src-tauri" / "Cargo.toml"
PREFIX = "WORKFLOW_BENCH "


def case_key(case: dict[str, Any]) -> str:
    if case["workflow"] == "paths":
        mode = case.get("mode")
        if mode not in ("no_respec", "optimum_envelope"):
            raise ValueError("Paths benchmark requires mode; regenerate historical Paths reports")
        return f"paths:{mode}:{case['horizon']}:{case.get('lanes')}"
    if case["workflow"] == "upgrade_series":
        return (
            f"upgrade_series:{case.get('reinforcement', 'all')}:"
            f"{case.get('points', 'all')}"
        )
    lane = case.get("lanes", case.get("affinities", "all"))
    return f"{case['workflow']}:{case['horizon']}:{lane}"


def index_cases(cases: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    indexed: dict[str, dict[str, Any]] = {}
    for case in cases:
        key = case_key(case)
        if key in indexed:
            raise ValueError(f"duplicate benchmark case: {key}")
        if case["workflow"] == "paths":
            if case.get("timing_scope") != "paths_only":
                raise ValueError(f"Paths {key} requires timing_scope=paths_only")
            warmups = case.get("warmups")
            if isinstance(warmups, bool) or not isinstance(warmups, int) or warmups < 0:
                raise ValueError(f"Paths {key} requires non-negative integer warmups")
            lanes = case.get("lanes")
            if isinstance(lanes, bool) or not isinstance(lanes, int) or lanes < 1:
                raise ValueError(f"Paths {key} requires positive integer lanes")
            for field in ("requests", "results"):
                records = case.get(field)
                if (
                    not isinstance(records, list) or len(records) != lanes
                    or any(not isinstance(record, dict) or not record for record in records)
                ):
                    raise ValueError(f"Paths {key} requires complete {field} objects for every lane")
        values = [case.get("median_ms")]
        values.extend(case[field] for field in ("best_ms", "worst_ms") if field in case)
        if "samples_ms" in case:
            samples = case["samples_ms"]
            if not isinstance(samples, list) or not samples:
                raise ValueError(f"{key} requires finite positive samples_ms")
            values.extend(samples)
        for value in values:
            if (
                isinstance(value, bool) or not isinstance(value, (int, float))
                or not math.isfinite(value) or value <= 0
            ):
                raise ValueError(f"{key} requires finite positive timings")
        indexed[key] = case
    return indexed


def compare_baseline(
    cases: list[dict[str, Any]], baseline_path: Path, threshold: float,
    *, coverage: dict[str, Any],
) -> list[dict[str, Any]]:
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    current, previous = index_cases(cases), index_cases(baseline["cases"])
    requested = current.keys() | previous.keys()
    compared = current.keys() & previous.keys()
    coverage.update({
        "requested": sorted(requested),
        "requested_source": "full_comparison_union",
        "compared": sorted(compared),
        "missing": sorted(requested - compared),
        "intentionally_skipped": [],
        "output_parity_verified": [],
        "timing_only": [],
    })
    regressions: list[dict[str, Any]] = []
    for key in sorted(compared):
        case, old = current[key], previous[key]
        if case["workflow"] == "paths" and case["requests"] != old["requests"]:
            raise ValueError(f"Paths {key} changed normalized requests; timings are not comparable")
        if old.get("results") is not None and old["results"] != case.get("results"):
            raise ValueError(f"{key} changed ranked results; review correctness before accepting timings")
        parity = "output_parity_verified" if old.get("results") is not None else "timing_only"
        coverage[parity].append(key)
        old_ms = float(old["median_ms"])
        change = ((float(case["median_ms"]) - old_ms) / old_ms) * 100.0
        if not math.isfinite(change):
            raise ValueError(f"{key} has non-finite timing change")
        case["baseline_median_ms"] = old_ms
        case["regression_percent"] = change
        if change > threshold:
            regressions.append({"case": key, "regression_percent": change})
    return regressions


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--max-regression-percent", type=float, default=20.0)
    parser.add_argument("--fail-on-regression", action="store_true")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be at least 1")
    if not math.isfinite(args.max_regression_percent) or args.max_regression_percent < 0:
        parser.error("--max-regression-percent must be finite and non-negative")
    if args.fail_on_regression and not args.baseline:
        parser.error("--fail-on-regression requires --baseline")
    if args.baseline and not args.baseline.is_file():
        parser.error(f"baseline does not exist: {args.baseline}")

    environment = os.environ.copy()
    environment["ER_BENCH_REPEATS"] = str(args.repeats)
    environment.setdefault("RAYON_NUM_THREADS", "1")
    command = [
        "cargo",
        "test",
        "--locked",
        "--release",
        "--manifest-path",
        str(MANIFEST),
        "workflow_benchmark",
        "--",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    build_metadata = capture_build_metadata(ROOT, MANIFEST, environment, command)
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        capture_output=True,
        text=True,
    )
    combined = f"{result.stdout}\n{result.stderr}"
    if result.returncode != 0:
        print(combined, file=sys.stderr)
        return result.returncode
    finished_build = capture_build_metadata(ROOT, MANIFEST, environment, command)
    if (
        finished_build["source"]["fingerprint"] != build_metadata["source"]["fingerprint"]
        or finished_build["compiler_variant_fingerprint"] != build_metadata["compiler_variant_fingerprint"]
    ):
        raise RuntimeError("source or compiler configuration changed during benchmark; discard measurements")
    cases = [
        json.loads(line.split(PREFIX, 1)[1])
        for line in combined.splitlines()
        if PREFIX in line
    ]
    if not cases:
        raise RuntimeError("benchmark command produced no workflow cases")
    index_cases(cases)

    model_versions = {case["model_version"] for case in cases}
    if len(model_versions) != 1:
        raise RuntimeError("workflow cases disagree on runtime model identity")

    manifest = json.loads((ROOT / "data" / "phase1" / "manifest.json").read_text(encoding="utf-8"))
    report: dict[str, Any] = {
        "metadata": {
            "profile": "release",
            "rayon_threads": environment["RAYON_NUM_THREADS"],
            "python": platform.python_version(),
            "platform": platform.platform(),
            "processor": platform.processor(),
            "rustc": build_metadata["compiler"]["rustc_version_verbose"].splitlines()[0],
            "commit": build_metadata["source"]["commit"],
            "build": build_metadata,
            "dataset_id": manifest["id"],
            "dataset_version": manifest["datasetVersion"],
            "model_version": next(iter(model_versions)),
        },
        "cases": cases,
    }

    comparison: dict[str, Any] = {}
    regressions = (
        compare_baseline(cases, args.baseline, args.max_regression_percent, coverage=comparison)
        if args.baseline else []
    )
    report["regressions"] = regressions
    report["comparison_mode"] = "enforced" if args.fail_on_regression else "advisory"
    report["comparison"] = comparison if args.baseline else None

    encoded = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(f"{encoded}\n", encoding="utf-8")
    print(encoded)
    incomplete = bool(args.baseline and (comparison["missing"] or not comparison["compared"]))
    return 1 if args.fail_on_regression and (regressions or incomplete) else 0


if __name__ == "__main__":
    raise SystemExit(main())
