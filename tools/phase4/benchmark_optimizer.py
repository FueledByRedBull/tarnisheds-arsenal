from __future__ import annotations

import statistics
import time
from pathlib import Path
from typing import Any

import er_optimizer_core as core


def run_case(data: Any, name: str, repeats: int, kwargs: dict[str, Any]) -> None:
    timings: list[float] = []
    result_counts: list[int] = []
    for _ in range(repeats):
        start = time.perf_counter()
        rows = core.optimize_builds(data=data, **kwargs)
        elapsed = time.perf_counter() - start
        timings.append(elapsed)
        result_counts.append(len(rows))

    best = min(timings)
    median = statistics.median(timings)
    worst = max(timings)
    print(
        f"{name}: rows={result_counts[-1]} "
        f"best={best * 1000.0:.1f}ms median={median * 1000.0:.1f}ms worst={worst * 1000.0:.1f}ms"
    )


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


def main() -> int:
    project_root = Path(__file__).resolve().parents[2]
    data = core.load_game_data(str(project_root / "data" / "phase1"))
    repeats = 3

    open_search = base_request()
    open_search.update(
        {
            "character_level": 46,
            "vig": 12,
            "mnd": 11,
            "end": 13,
            "arc": 45,
            "fixed_upgrade": 25,
            "weapon_name": None,
            "affinity": None,
            "weapon_type_key": "Katana",
            "top_k": 5,
        }
    )
    run_case(data, "katana open max_ar", repeats, open_search)

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
    run_case(data, "locked exact max_ar", repeats, locked)

    bleed = base_request()
    bleed.update(
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
    run_case(data, "open aow max_ar_plus_bleed", repeats, bleed)

    aow = base_request()
    aow.update(
        {
            "character_level": 84,
            "weapon_name": "Sword Lance",
            "affinity": "Magic",
            "aow_name": "Glintstone Pebble",
            "fixed_upgrade": 25,
            "objective": "aow_first_hit",
            "str_stat": 21,
            "dex": 15,
            "int_stat": 40,
            "arc": 8,
            "top_k": 5,
        }
    )
    run_case(data, "aow first hit", repeats, aow)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
