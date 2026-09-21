#!/usr/bin/env python3
"""Benchmark optimizer preparation, scoring, and materialization in release mode."""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import subprocess
import time
from pathlib import Path
from typing import Any

if __package__:
    from .benchmark_build_metadata import capture_build_metadata
else:
    from benchmark_build_metadata import capture_build_metadata

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "core" / "er_optimizer_core" / "Cargo.toml"
PREFIX = "PHASE_BENCH "
PHASE_KEYS = (
    "preparationMedianMs",
    "scoringMedianMs",
    "materializationMedianMs",
    "totalMedianMs",
)


def compare_baseline(
    cases: list[dict[str, Any]], baseline_path: Path, threshold: float,
    *, requested_cases: list[str] | None = None, coverage: dict[str, Any] | None = None,
) -> list[dict[str, Any]]:
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    current = index_cases(cases)
    previous = index_cases(baseline["cases"])
    requested = current.keys() | previous.keys() if requested_cases is None else set(requested_cases)
    compared = requested & current.keys() & previous.keys()
    comparison: dict[str, Any] = {
        "requested": sorted(requested),
        "requested_source": "explicit_case" if requested_cases is not None else "full_comparison_union",
        "compared": sorted(compared),
        "missing": sorted(requested - compared),
        "intentionally_skipped": sorted((current.keys() | previous.keys()) - requested),
        "output_parity_verified": [],
        "timing_only": [],
    }
    regressions: list[dict[str, Any]] = []
    for name in sorted(compared):
        case, old = current[name], previous[name]
        if old.get("results") is not None and old["results"] != case.get("results"):
            raise ValueError(f"{case['name']} changed ranked results; review correctness before accepting timings")
        parity = "output_parity_verified" if old.get("results") is not None else "timing_only"
        comparison[parity].append(name)
        changes: dict[str, float] = {}
        for key in PHASE_KEYS:
            old_value = float(old[key])
            change = ((float(case[key]) - old_value) / old_value) * 100.0
            if not math.isfinite(change):
                raise ValueError(f"{name} has non-finite timing change for {key}")
            changes[key] = change
            if change > threshold:
                regressions.append(
                    {
                        "case": case["name"],
                        "phase": key.removesuffix("MedianMs"),
                        "regressionPercent": change,
                    }
                )
        case["baselineChangesPercent"] = changes
    if coverage is not None:
        coverage.update(comparison)
    return regressions


def index_cases(cases: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    indexed: dict[str, dict[str, Any]] = {}
    for case in cases:
        name = case["name"]
        if name in indexed:
            raise ValueError(f"duplicate benchmark case: {name}")
        for key in PHASE_KEYS:
            values = [case.get(key)]
            sample_key = key.replace("MedianMs", "SamplesMs")
            if sample_key in case:
                samples = case[sample_key]
                if not isinstance(samples, list) or not samples:
                    raise ValueError(f"{name} requires finite positive {sample_key}")
                values.extend(samples)
            for value in values:
                if (
                    isinstance(value, bool) or not isinstance(value, (int, float))
                    or not math.isfinite(value) or value <= 0
                ):
                    raise ValueError(f"{name} requires finite positive {key} and samples")
        indexed[name] = case
    return indexed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("vanilla", "convergence"), default="vanilla")
    parser.add_argument("--case", help="Run one named Rust benchmark case.")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--max-regression-percent", type=float, default=20.0)
    parser.add_argument(
        "--fail-on-regression",
        action="store_true",
        help="Opt into a non-zero exit; default baseline comparisons are advisory.",
    )
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be at least 1")
    if args.warmups < 0:
        parser.error("--warmups must be non-negative")
    if not math.isfinite(args.max_regression_percent) or args.max_regression_percent < 0:
        parser.error("--max-regression-percent must be finite and non-negative")
    if args.fail_on_regression and not args.baseline:
        parser.error("--fail-on-regression requires --baseline")
    if args.baseline and not args.baseline.is_file():
        parser.error(f"baseline does not exist: {args.baseline}")

    environment = os.environ.copy()
    environment.setdefault("RAYON_NUM_THREADS", "1")
    command = [
        "cargo",
        "run",
        "--locked",
        "--offline",
        "--release",
        "--manifest-path",
        str(MANIFEST),
        "--example",
        "benchmark_optimizer_phases",
        "--",
        f"--profile={args.profile}",
        f"--warmups={args.warmups}",
        f"--repeats={args.repeats}",
    ]
    if args.case:
        command.append(f"--case={args.case}")
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
        print(combined)
        return result.returncode
    finished_build = capture_build_metadata(ROOT, MANIFEST, environment, command)
    if (
        finished_build["source"]["fingerprint"] != build_metadata["source"]["fingerprint"]
        or finished_build["compiler_variant_fingerprint"] != build_metadata["compiler_variant_fingerprint"]
    ):
        raise RuntimeError("source or compiler configuration changed during benchmark; discard measurements")
    records = [
        json.loads(line.split(PREFIX, 1)[1])
        for line in combined.splitlines()
        if PREFIX in line
    ]
    metadata_record = next(
        (record for record in records if record.get("kind") == "metadata"), None
    )
    cases = [record for record in records if record.get("kind") == "case"]
    if metadata_record is None or not cases:
        raise RuntimeError("phase benchmark produced incomplete output")
    index_cases(cases)

    metadata = {
        **metadata_record,
        "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "platform": platform.platform(),
        "processor": platform.processor(),
        "cpuCount": os.cpu_count(),
        "rustc": build_metadata["compiler"]["rustc_version_verbose"].splitlines()[0],
        "commit": build_metadata["source"]["commit"],
        "build": build_metadata,
    }
    comparison: dict[str, Any] = {}
    regressions = (
        compare_baseline(
            cases, args.baseline, args.max_regression_percent,
            requested_cases=[args.case] if args.case else None, coverage=comparison,
        )
        if args.baseline
        else []
    )
    report = {
        "schemaVersion": 1,
        "metadata": metadata,
        "cases": cases,
        "regressions": regressions,
        "comparisonMode": "enforced" if args.fail_on_regression else "advisory",
        "comparison": comparison if args.baseline else None,
    }
    encoded = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(f"{encoded}\n", encoding="utf-8")
    print(encoded)
    incomplete = bool(args.baseline and (comparison["missing"] or not comparison["compared"]))
    return 1 if args.fail_on_regression and (regressions or incomplete) else 0


if __name__ == "__main__":
    raise SystemExit(main())
