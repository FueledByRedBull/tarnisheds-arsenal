from __future__ import annotations

import argparse
import json
import math
import re
import urllib.request
from urllib.parse import urljoin
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import er_optimizer_core as core


TCALC_DATA_URL = "https://eldenring.tclark.io/regulation-vanilla-v1.14.js?0"
TARNISHED_CALCULATOR_URL = "https://www.tarnished.dev/weapon-calculator"
STATS = ("str", "dex", "int", "fai", "arc")


@dataclass(frozen=True)
class ComparisonCase:
    label: str
    site_weapon: str
    weapon_name: str
    affinity: str
    aow_name: str
    upgrade: int
    stats: dict[str, int]
    two_handing: bool = False


CASES = (
    ComparisonCase(
        "Keen Uchigatana +25, 20 STR / 40 DEX",
        "Keen Uchigatana",
        "Uchigatana",
        "Keen",
        "Unsheathe",
        25,
        {"str": 20, "dex": 40, "int": 10, "fai": 10, "arc": 10},
    ),
    ComparisonCase(
        "Quality Lordsworn's Greatsword +25, 40 STR / 40 DEX",
        "Lordsworn's Quality Greatsword",
        "Lordsworn's Greatsword",
        "Quality",
        "Stamp (Upward Cut)",
        25,
        {"str": 40, "dex": 40, "int": 10, "fai": 10, "arc": 10},
    ),
    ComparisonCase(
        "Reduvia +10, 20 DEX / 45 ARC",
        "Reduvia",
        "Reduvia",
        "Standard",
        "Reduvia Blood Blade",
        10,
        {"str": 10, "dex": 20, "int": 10, "fai": 10, "arc": 45},
    ),
    ComparisonCase(
        "Fire Uchigatana +25 two-handed, 48 STR / 15 DEX",
        "Fire Uchigatana",
        "Uchigatana",
        "Fire",
        "Unsheathe",
        25,
        {"str": 48, "dex": 15, "int": 10, "fai": 10, "arc": 10},
        two_handing=True,
    ),
)


def fetch_text(url: str) -> str:
    request = urllib.request.Request(url, headers={"User-Agent": "Tarnisheds-Arsenal-validator"})
    with urllib.request.urlopen(request, timeout=30) as response:  # noqa: S310
        return response.read().decode("utf-8")


def fetch_site_data(url: str) -> dict[str, Any]:
    return json.loads(fetch_text(url))


def fetch_tarnished_keen_uchigatana(url: str) -> dict[str, Any]:
    page = fetch_text(url)
    script_paths = re.findall(r'src="([^"]+\.js[^"]*)"', page)
    marker = '{"name":"Keen Uchigatana"'
    for script_path in script_paths:
        script = fetch_text(urljoin(url, script_path))
        start = script.find(marker)
        if start < 0:
            continue
        end = script.find('},{"name":', start)
        if end < 0:
            raise ValueError("Tarnished.dev Keen Uchigatana record has no terminator")
        return json.loads(script[start : end + 1].replace("\\'", "'"))
    raise ValueError("Tarnished.dev calculator bundle has no Keen Uchigatana record")


def build_curve(points: list[dict[str, float]]) -> list[float]:
    values = [0.0] * 149
    for index in range(1, len(points)):
        lower = points[index - 1]
        upper = points[index]
        start = 1 if index == 1 else int(lower["maxVal"]) + 1
        end = 148 if index == len(points) - 1 else int(upper["maxVal"])
        for stat in range(start, end + 1):
            progress = max(
                0.0,
                min(
                    1.0,
                    (stat - lower["maxVal"]) / (upper["maxVal"] - lower["maxVal"]),
                ),
            )
            adjustment = lower["adjPt"]
            if adjustment > 0:
                progress **= adjustment
            elif adjustment < 0:
                progress = 1.0 - (1.0 - progress) ** -adjustment
            values[stat] = lower["maxGrowVal"] + (
                upper["maxGrowVal"] - lower["maxGrowVal"]
            ) * progress
    return values


def site_attack_rating(data: dict[str, Any], case: ComparisonCase) -> float:
    weapon = next(row for row in data["weapons"] if row["name"] == case.site_weapon)
    reinforce = data["reinforceTypes"][str(weapon["reinforceTypeId"])][case.upgrade]
    corrections = data["attackElementCorrects"][str(weapon["attackElementCorrectId"])]
    graph_ids = weapon.get("calcCorrectGraphIds") or {}
    base_scaling = dict(weapon["attributeScaling"])
    effective_stats = dict(case.stats)
    if case.two_handing:
        effective_stats["str"] = math.floor(effective_stats["str"] * 1.5)

    total = 0.0
    for damage_type, base_attack in weapon["attack"]:
        damage_key = str(damage_type)
        reinforced_attack = base_attack * reinforce["attack"].get(damage_key, 0.0)
        multiplier = 1.0
        graph_id = str(graph_ids.get(damage_key, 0))
        curve = build_curve(data["calcCorrectGraphs"][graph_id])
        for stat in STATS:
            if corrections.get(damage_key, {}).get(stat, False):
                scaling = base_scaling.get(stat, 0.0) * reinforce["attributeScaling"][stat]
                multiplier += curve[effective_stats[stat]] * scaling
        total += reinforced_attack * multiplier
    return total


def local_attack_rating(data: Any, case: ComparisonCase) -> float:
    level = 30 + sum(case.stats.values()) - 79
    rows = core.optimize_builds(
        data=data,
        class_name="Wretch",
        character_level=level,
        vig=10,
        mnd=10,
        end=10,
        str_stat=case.stats["str"],
        dex=case.stats["dex"],
        int_stat=case.stats["int"],
        fai=case.stats["fai"],
        arc=case.stats["arc"],
        max_upgrade=case.upgrade,
        fixed_upgrade=case.upgrade,
        two_handing=case.two_handing,
        weapon_name=case.weapon_name,
        affinity=case.affinity,
        aow_name=case.aow_name,
        objective="max_ar",
        top_k=1,
        somber_filter="all",
        lock_str=case.stats["str"],
        lock_dex=case.stats["dex"],
        lock_int=case.stats["int"],
        lock_fai=case.stats["fai"],
        lock_arc=case.stats["arc"],
    )
    if len(rows) != 1:
        raise RuntimeError(f"local optimizer returned {len(rows)} rows for {case.label}")
    return float(rows[0].ar_total)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compare local AR with the T. Clark calculator's independent static formula/data."
    )
    parser.add_argument("--url", default=TCALC_DATA_URL)
    parser.add_argument("--tarnished-url", default=TARNISHED_CALCULATOR_URL)
    parser.add_argument("--tolerance", type=float, default=0.05)
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[2]
    site_data = fetch_site_data(args.url)
    local_data = core.load_game_data(str(root / "data" / "phase1"))
    failures = 0
    print(f"source={args.url}")
    print("note=external snapshot identifies itself as App 1.14; cases avoid changed DLC data")
    for case in CASES:
        external = site_attack_rating(site_data, case)
        local = local_attack_rating(local_data, case)
        drift = local - external
        passed = abs(drift) <= args.tolerance and round(local) == round(external)
        failures += int(not passed)
        print(
            f"{'PASS' if passed else 'FAIL'} {case.label}: "
            f"external={external:.4f} local={local:.4f} drift={drift:+.4f} "
            f"display={round(external)}/{round(local)}"
        )

    tarnished = fetch_tarnished_keen_uchigatana(args.tarnished_url)
    tcalc_weapon = next(row for row in site_data["weapons"] if row["name"] == "Keen Uchigatana")
    reinforce = site_data["reinforceTypes"][str(tcalc_weapon["reinforceTypeId"])][25]
    scaling = dict(tcalc_weapon["attributeScaling"])
    expected = {
        "phys25": dict(tcalc_weapon["attack"])[0] * reinforce["attack"]["0"],
        "str25": scaling["str"] * reinforce["attributeScaling"]["str"],
        "dex25": scaling["dex"] * reinforce["attributeScaling"]["dex"],
        "attackelementcorrectId": tcalc_weapon["attackElementCorrectId"],
        "physical": int(tcalc_weapon["calcCorrectGraphIds"]["0"]),
    }
    parity = all(
        abs(float(tarnished[field]) - float(value)) <= 0.0001
        for field, value in expected.items()
    )
    failures += int(not parity)
    print(
        f"{'PASS' if parity else 'FAIL'} Tarnished.dev Keen Uchigatana +25 source parity: "
        f"base={tarnished['phys25']} str={tarnished['str25']} dex={tarnished['dex25']} "
        f"aec={tarnished['attackelementcorrectId']} graph={tarnished['physical']}"
    )
    return int(failures > 0)


if __name__ == "__main__":
    raise SystemExit(main())
