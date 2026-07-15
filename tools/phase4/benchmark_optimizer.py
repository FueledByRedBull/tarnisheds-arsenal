from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import sys
import time
from pathlib import Path
from typing import Any

import er_optimizer_core as core


def estimate_kwargs(kwargs: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in kwargs.items() if key not in {"top_k", "progress_every"}}


def run_case(
    data: Any,
    name: str,
    repeats: int,
    warmups: int,
    kwargs: dict[str, Any],
) -> dict[str, Any]:
    estimate_start = time.perf_counter()
    estimate = core.estimate_search_space(data=data, **estimate_kwargs(kwargs))
    estimate_elapsed = time.perf_counter() - estimate_start

    for _ in range(warmups):
        core.optimize_builds(data=data, **kwargs)
    timings: list[float] = []
    result_counts: list[int] = []
    for _ in range(repeats):
        start = time.perf_counter()
        rows = core.optimize_builds(data=data, **kwargs)
        timings.append(time.perf_counter() - start)
        result_counts.append(len(rows))

    if len(set(result_counts)) != 1:
        raise RuntimeError(f"{name} returned inconsistent row counts: {result_counts}")

    result = {
        "name": name,
        "objective": kwargs["objective"],
        "weaponCandidates": estimate.weapon_candidates,
        "combinations": estimate.combinations,
        "estimateMs": estimate_elapsed * 1000.0,
        "rows": result_counts[-1],
        "bestMs": min(timings) * 1000.0,
        "medianMs": statistics.median(timings) * 1000.0,
        "worstMs": max(timings) * 1000.0,
        "samplesMs": [timing * 1000.0 for timing in timings],
    }
    print(
        f"{name}: objective={result['objective']} weapons={result['weaponCandidates']} "
        f"combos={result['combinations']} estimate={result['estimateMs']:.1f}ms "
        f"rows={result['rows']} best={result['bestMs']:.1f}ms "
        f"median={result['medianMs']:.1f}ms worst={result['worstMs']:.1f}ms"
    )
    return result


def base_request() -> dict[str, Any]:
    return {
        "class_name": "Samurai",
        "character_level": 80,
        "vig": 40,
        "mnd": 11,
        "end": 20,
        "str_stat": 12,
        "dex": 15,
        "int_stat": 9,
        "fai": 8,
        "arc": 20,
        "max_upgrade": 25,
        "fixed_upgrade": None,
        "two_handing": False,
        "weapon_name": None,
        "affinity": None,
        "aow_name": None,
        "weapon_type_key": None,
        "somber_filter": "all",
        "objective": "max_ar",
        "top_k": 10,
        "min_str": 0,
        "min_dex": 0,
        "min_int": 0,
        "min_fai": 0,
        "min_arc": 0,
        "lock_str": None,
        "lock_dex": None,
        "lock_int": None,
        "lock_fai": None,
        "lock_arc": None,
    }


def benchmark_cases(quick: bool) -> list[tuple[str, dict[str, Any]]]:
    low_level = base_request()
    low_level.update(
        {
            "character_level": 46,
            "vig": 12,
            "mnd": 11,
            "end": 13,
            "arc": 8,
            "fixed_upgrade": 25,
            "top_k": 5,
        }
    )

    katana = dict(low_level)
    katana["weapon_type_key"] = "Katana"
    broad_ar = dict(low_level)
    broad_physical = dict(low_level)
    broad_physical["objective"] = "max_physical_ar"
    katana_bleed = dict(katana)
    katana_bleed["objective"] = "max_ar_plus_bleed"
    high_level_katana = base_request()
    high_level_katana.update(
        {
            "character_level": 150,
            "weapon_type_key": "Katana",
            "fixed_upgrade": 25,
            "top_k": 5,
        }
    )

    locked = base_request()
    locked.update(
        {
            "character_level": 46,
            "vig": 12,
            "mnd": 11,
            "end": 13,
            "weapon_name": "Uchigatana",
            "affinity": "Blood",
            "aow_name": "Seppuku",
            "fixed_upgrade": 25,
            "lock_str": 12,
            "lock_dex": 15,
            "lock_int": 9,
            "lock_fai": 8,
            "lock_arc": 45,
            "arc": 45,
            "top_k": 1,
        }
    )

    open_aow = base_request()
    open_aow.update(
        {
            "character_level": 112,
            "weapon_name": "Uchigatana",
            "affinity": "Keen",
            "fixed_upgrade": 25,
            "objective": "max_ar_plus_bleed",
            "lock_str": 18,
            "lock_dex": 40,
            "lock_int": 9,
            "lock_fai": 8,
            "lock_arc": 45,
            "arc": 8,
            "top_k": 5,
        }
    )

    first_hit = base_request()
    first_hit.update(
        {
            "character_level": 150,
            "weapon_name": "Sword Lance",
            "affinity": "Magic",
            "aow_name": "Glintstone Pebble",
            "fixed_upgrade": 25,
            "objective": "aow_first_hit",
            "str_stat": 21,
            "dex": 15,
            "int_stat": 20,
            "arc": 8,
            "top_k": 5,
        }
    )
    full_sequence = dict(first_hit)
    full_sequence["objective"] = "aow_full_sequence"

    cases = [
        ("katana open max_ar", katana),
        ("all weapons open max_ar", broad_ar),
        ("all weapons open max_physical_ar", broad_physical),
        ("katana open max_ar_plus_bleed", katana_bleed),
        ("high-level katana max_ar", high_level_katana),
        ("locked exact max_ar", locked),
        ("open aow max_ar_plus_bleed", open_aow),
        ("aow first hit", first_hit),
        ("aow full sequence", full_sequence),
    ]
    if quick:
        return [cases[index] for index in (0, 1, 3, 7, 8)]
    return cases


def compare_baseline(
    report: dict[str, Any], baseline_path: Path, max_regression_percent: float
) -> list[str]:
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    previous = {case["name"]: case for case in baseline.get("cases", [])}
    failures: list[str] = []
    for case in report["cases"]:
        old = previous.get(case["name"])
        if not old or not old.get("medianMs"):
            continue
        delta = (case["medianMs"] / old["medianMs"] - 1.0) * 100.0
        case["baselineMedianMs"] = old["medianMs"]
        case["regressionPercent"] = delta
        if delta > max_regression_percent:
            failures.append(
                f"{case['name']} regressed {delta:.1f}% (limit {max_regression_percent:.1f}%)"
            )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description="Benchmark representative optimizer searches.")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--max-regression-percent", type=float, default=20.0)
    parser.add_argument("--fail-on-regression", action="store_true")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be at least 1")
    if args.warmups < 0:
        parser.error("--warmups must be non-negative")
    if args.baseline and not args.baseline.is_file():
        parser.error(f"baseline does not exist: {args.baseline}")

    project_root = Path(__file__).resolve().parents[2]
    data_dir = project_root / "data" / "phase1"
    manifest = json.loads((data_dir / "manifest.json").read_text(encoding="utf-8"))
    load_started = time.perf_counter()
    data = core.load_game_data(str(data_dir))
    load_ms = (time.perf_counter() - load_started) * 1000.0
    metadata = {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "cpuCount": os.cpu_count(),
        "rayonThreads": os.environ.get("RAYON_NUM_THREADS", "default"),
        "profile": "release-python-extension",
        "repeats": args.repeats,
        "warmups": args.warmups,
        "datasetId": manifest["id"],
        "datasetVersion": manifest["datasetVersion"],
        "modelVersion": manifest["modelVersion"],
        "commit": os.environ.get("GITHUB_SHA", "local"),
        "dataLoadMs": load_ms,
    }
    print(
        " ".join(
            [
                f"python={metadata['python']}",
                f"platform={metadata['platform']}",
                f"cpu_count={metadata['cpuCount']}",
                f"rayon_threads={metadata['rayonThreads']}",
                f"dataset={metadata['datasetId']}",
                f"repeats={args.repeats}",
                f"warmups={args.warmups}",
                f"data_load={load_ms:.1f}ms",
            ]
        )
    )

    report = {
        "schemaVersion": 1,
        "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "metadata": metadata,
        "cases": [
            run_case(data, name, args.repeats, args.warmups, kwargs)
            for name, kwargs in benchmark_cases(args.quick)
        ],
        "comparisonMode": "enforced" if args.fail_on_regression else "advisory",
    }
    failures = (
        compare_baseline(report, args.baseline, args.max_regression_percent)
        if args.baseline
        else []
    )
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"report={args.output}")
    for failure in failures:
        print(f"REGRESSION: {failure}", file=sys.stderr)
    return 1 if failures and args.fail_on_regression else 0


if __name__ == "__main__":
    raise SystemExit(main())
