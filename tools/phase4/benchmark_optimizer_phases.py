#!/usr/bin/env python3
"""Benchmark optimizer preparation, scoring, and materialization in release mode."""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "core" / "er_optimizer_core" / "Cargo.toml"
PREFIX = "PHASE_BENCH "
PHASE_KEYS = (
    "preparationMedianMs",
    "scoringMedianMs",
    "materializationMedianMs",
    "totalMedianMs",
)


def command_output(*command: str) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True, stderr=subprocess.PIPE).strip()


def compare_baseline(
    cases: list[dict[str, Any]], baseline_path: Path, threshold: float
) -> list[dict[str, Any]]:
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    previous = {case["name"]: case for case in baseline.get("cases", [])}
    regressions: list[dict[str, Any]] = []
    for case in cases:
        old = previous.get(case["name"])
        if not old:
            continue
        if old.get("results") is not None and old["results"] != case.get("results"):
            raise ValueError(f"{case['name']} changed ranked results; review correctness before accepting timings")
        comparison: dict[str, float] = {}
        for key in PHASE_KEYS:
            old_value = float(old.get(key, 0.0))
            if old_value <= 0.0:
                continue
            change = ((float(case[key]) - old_value) / old_value) * 100.0
            comparison[key] = change
            if change > threshold:
                regressions.append(
                    {
                        "case": case["name"],
                        "phase": key.removesuffix("MedianMs"),
                        "regressionPercent": change,
                    }
                )
        case["baselineChangesPercent"] = comparison
    return regressions


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

    metadata = {
        **metadata_record,
        "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "platform": platform.platform(),
        "processor": platform.processor(),
        "cpuCount": os.cpu_count(),
        "rustc": command_output("rustc", "--version"),
        "commit": command_output("git", "rev-parse", "HEAD"),
    }
    regressions = (
        compare_baseline(cases, args.baseline, args.max_regression_percent)
        if args.baseline
        else []
    )
    report = {
        "schemaVersion": 1,
        "metadata": metadata,
        "cases": cases,
        "regressions": regressions,
        "comparisonMode": "enforced" if args.fail_on_regression else "advisory",
    }
    encoded = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(f"{encoded}\n", encoding="utf-8")
    print(encoded)
    return 1 if regressions and args.fail_on_regression else 0


if __name__ == "__main__":
    raise SystemExit(main())
