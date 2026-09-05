#!/usr/bin/env python3

from __future__ import annotations

import csv
from collections import defaultdict
from pathlib import Path

STAT_KEYS = ("str", "dex", "int", "fai", "arc")
DAMAGE_KEYS = ("physical", "magic", "fire", "lightning", "holy")


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def scale_letter(value: float, extended: bool = False) -> str:
    if value <= 0.0:
        return "-"
    if extended and value >= 2.25:
        return "S++"
    if extended and value >= 2.0:
        return "S+"
    if value >= 1.75:
        return "S"
    if value >= 1.4:
        return "A"
    if value >= 0.9:
        return "B"
    if value >= 0.6:
        return "C"
    if value >= 0.25:
        return "D"
    return "E"


def truthy(value: str) -> bool:
    return value.strip().lower() in {"1", "true", "yes"}


def effective_stat_labels(
    weapon_row: dict[str, str],
    aec_row: dict[str, str] | None,
) -> tuple[list[str], list[str]]:
    usable: list[str] = []
    dead: list[str] = []
    for stat in STAT_KEYS:
        scaling = float(weapon_row[f"{stat}_scaling"])
        contributes = False
        if scaling > 0.0 and aec_row is not None:
            for damage in DAMAGE_KEYS:
                if float(weapon_row[f"base_{damage}"]) <= 0.0:
                    continue
                if truthy(aec_row[f"{stat}_scales_{damage}"]):
                    contributes = True
                    break
        if contributes:
            usable.append(stat.upper())
        else:
            dead.append(stat.upper())
    return usable, dead


def build_weapon_scaling_summary(
    weapons: list[dict[str, str]],
    aec_rows: list[dict[str, str]],
    *,
    extended_scaling_grades: bool = False,
) -> list[dict[str, str]]:
    aec_map = {row["attack_element_correct_id"]: row for row in aec_rows}
    rows: list[dict[str, str]] = []
    for weapon in sorted(weapons, key=lambda row: (row["name"], row["affinity"], int(row["weapon_id"]))):
        aec_row = aec_map.get(weapon["attack_element_correct_id"])
        usable, dead = effective_stat_labels(weapon, aec_row)
        out: dict[str, str] = {
            "weapon_id": weapon["weapon_id"],
            "name": weapon["name"],
            "affinity": weapon["affinity"],
            "weapon_type_name": weapon["weapon_type_name"],
            "weapon_type_keys": weapon["weapon_type_keys"],
            "attack_element_correct_id": weapon["attack_element_correct_id"],
            "usable_stats": "|".join(usable),
            "dead_stats": "|".join(dead),
        }
        for stat in STAT_KEYS:
            scaling = float(weapon[f"{stat}_scaling"])
            out[f"{stat}_scaling"] = f"{scaling:.2f}"
            out[f"{stat}_grade"] = scale_letter(scaling, extended_scaling_grades)
            out[f"{stat}_effective"] = "1" if stat.upper() in usable else "0"
        rows.append(out)
    return rows


def build_aow_affinity_compat(
    weapons: list[dict[str, str]],
    aows: list[dict[str, str]],
) -> list[dict[str, str]]:
    grouped: dict[tuple[str, str, str], set[str]] = defaultdict(set)
    for ash in aows:
        affinities = set(ash["valid_affinities"].split("|"))
        weapon_types = set(ash["valid_weapon_types"].split("|"))
        for weapon in weapons:
            if (weapon["can_change_aow"] == "1"
                    and weapon["affinity"] in affinities
                    and weapon_types.intersection(weapon["weapon_type_keys"].split("|"))):
                grouped[(ash["aow_id"], ash["name"], weapon["affinity"])].add(weapon["name"])

    rows: list[dict[str, str]] = []
    for (aow_id, name, affinity), weapon_names in sorted(grouped.items(), key=lambda item: (item[0][1], item[0][2])):
        samples = sorted(weapon_names)[:5]
        rows.append(
            {
                "aow_id": aow_id,
                "name": name,
                "affinity": affinity,
                "weapon_count": str(len(weapon_names)),
                "sample_weapon_names": "|".join(samples),
            }
        )
    return rows


def derive_phase1_diagnostics(
    input_dir: Path,
    *,
    extended_scaling_grades: bool = False,
) -> tuple[list[dict[str, str]], list[dict[str, str]]]:
    weapons = read_csv(input_dir / "weapons.csv")
    aec_rows = read_csv(input_dir / "attack_element_correct.csv")
    aows = read_csv(input_dir / "aow.csv")

    return (
        build_weapon_scaling_summary(
            weapons,
            aec_rows,
            extended_scaling_grades=extended_scaling_grades,
        ),
        build_aow_affinity_compat(weapons, aows),
    )
