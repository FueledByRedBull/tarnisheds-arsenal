#!/usr/bin/env python3
"""Run release-mode Paths and Affinity Watch regression benchmarks."""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "apps" / "desktop" / "src-tauri" / "Cargo.toml"
PREFIX = "WORKFLOW_BENCH "


def command_output(*command: str) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True, stderr=subprocess.PIPE).strip()


def case_key(case: dict[str, Any]) -> str:
    if case["workflow"] == "upgrade_series":
        return (
            f"upgrade_series:{case.get('reinforcement', 'all')}:"
            f"{case.get('points', 'all')}"
        )
    lane = case.get("lanes", case.get("affinities", "all"))
    return f"{case['workflow']}:{case['horizon']}:{lane}"


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

    environment = os.environ.copy()
    environment["ER_BENCH_REPEATS"] = str(args.repeats)
    environment.setdefault("RAYON_NUM_THREADS", "1")
    result = subprocess.run(
        [
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
        ],
        cwd=ROOT,
        env=environment,
        capture_output=True,
        text=True,
    )
    combined = f"{result.stdout}\n{result.stderr}"
    if result.returncode != 0:
        print(combined, file=sys.stderr)
        return result.returncode
    cases = [
        json.loads(line.split(PREFIX, 1)[1])
        for line in combined.splitlines()
        if PREFIX in line
    ]
    if not cases:
        raise RuntimeError("benchmark command produced no workflow cases")

    manifest = json.loads((ROOT / "data" / "phase1" / "manifest.json").read_text(encoding="utf-8"))
    report: dict[str, Any] = {
        "metadata": {
            "profile": "release",
            "rayon_threads": environment["RAYON_NUM_THREADS"],
            "python": platform.python_version(),
            "platform": platform.platform(),
            "processor": platform.processor(),
            "rustc": command_output("rustc", "--version"),
            "commit": command_output("git", "rev-parse", "HEAD"),
            "dataset_id": manifest["id"],
            "dataset_version": manifest["datasetVersion"],
            "model_version": manifest["modelVersion"],
        },
        "cases": cases,
    }

    regressions: list[dict[str, Any]] = []
    if args.baseline:
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
        previous = {case_key(case): case for case in baseline["cases"]}
        for case in cases:
            key = case_key(case)
            if key not in previous:
                continue
            old_ms = float(previous[key]["median_ms"])
            change = ((float(case["median_ms"]) - old_ms) / max(old_ms, 1e-9)) * 100.0
            case["baseline_median_ms"] = old_ms
            case["regression_percent"] = change
            if change > args.max_regression_percent:
                regressions.append({"case": key, "regression_percent": change})
    report["regressions"] = regressions
    report["comparison_mode"] = "enforced" if args.fail_on_regression else "advisory"

    encoded = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(f"{encoded}\n", encoding="utf-8")
    print(encoded)
    return 1 if regressions and args.fail_on_regression else 0


if __name__ == "__main__":
    raise SystemExit(main())
