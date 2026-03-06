#!/usr/bin/env python3

from __future__ import annotations

import argparse
import csv
from collections import defaultdict
from pathlib import Path

STAT_KEYS = ("str", "dex", "int", "fai", "arc")
DAMAGE_KEYS = ("physical", "magic", "fire", "lightning", "holy")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Derive extra Phase 1 CSVs from the shipped snapshot.")
    parser.add_argument(
        "--input",
        type=Path,
        default=Path("data") / "phase1",
        help="Input directory containing the base Phase 1 CSVs",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("data") / "phase1",
        help="Output directory for derived CSVs",
    )
    return parser.parse_args()


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def write_csv(path: Path, fieldnames: list[str], rows: list[dict[str, object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow({key: row.get(key, "") for key in fieldnames})


def scale_letter(value: float) -> str:
    if value <= 0.0:
        return "-"
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
) -> list[dict[str, object]]:
    aec_map = {row["attack_element_correct_id"]: row for row in aec_rows}
    rows: list[dict[str, object]] = []
    for weapon in sorted(weapons, key=lambda row: (row["name"], row["affinity"], int(row["weapon_id"]))):
        aec_row = aec_map.get(weapon["attack_element_correct_id"])
        usable, dead = effective_stat_labels(weapon, aec_row)
        out: dict[str, object] = {
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
            out[f"{stat}_grade"] = scale_letter(scaling)
            out[f"{stat}_effective"] = "1" if stat.upper() in usable else "0"
        rows.append(out)
    return rows


def build_aow_affinity_compat(
    exact_compat_rows: list[dict[str, str]],
) -> list[dict[str, object]]:
    grouped: dict[tuple[str, str, str], set[str]] = defaultdict(set)
    for row in exact_compat_rows:
        key = (row["aow_id"], row["aow_name"], row["affinity"])
        grouped[key].add(row["weapon_name"])

    rows: list[dict[str, object]] = []
    for (aow_id, name, affinity), weapon_names in sorted(grouped.items(), key=lambda item: (item[0][1], item[0][2])):
        samples = sorted(weapon_names)[:5]
        rows.append(
            {
                "aow_id": aow_id,
                "name": name,
                "affinity": affinity,
                "weapon_count": len(weapon_names),
                "sample_weapon_names": "|".join(samples),
            }
        )
    return rows


def main() -> int:
    args = parse_args()
    input_dir = args.input
    output_dir = args.output

    weapons = read_csv(input_dir / "weapons.csv")
    aec_rows = read_csv(input_dir / "attack_element_correct.csv")
    exact_compat_rows = read_csv(input_dir / "aow_weapon_compat.csv")

    weapon_scaling_rows = build_weapon_scaling_summary(weapons, aec_rows)
    aow_affinity_rows = build_aow_affinity_compat(exact_compat_rows)

    write_csv(
        output_dir / "weapon_scaling_summary.csv",
        [
            "weapon_id",
            "name",
            "affinity",
            "weapon_type_name",
            "weapon_type_keys",
            "attack_element_correct_id",
            "str_scaling",
            "str_grade",
            "str_effective",
            "dex_scaling",
            "dex_grade",
            "dex_effective",
            "int_scaling",
            "int_grade",
            "int_effective",
            "fai_scaling",
            "fai_grade",
            "fai_effective",
            "arc_scaling",
            "arc_grade",
            "arc_effective",
            "usable_stats",
            "dead_stats",
        ],
        weapon_scaling_rows,
    )
    write_csv(
        output_dir / "aow_affinity_compat.csv",
        ["aow_id", "name", "affinity", "weapon_count", "sample_weapon_names"],
        aow_affinity_rows,
    )
    print(f"Wrote {len(weapon_scaling_rows)} weapon scaling rows")
    print(f"Wrote {len(aow_affinity_rows)} AoW affinity rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
