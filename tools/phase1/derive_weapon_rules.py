from __future__ import annotations

import csv
from pathlib import Path

PAIRED_TYPE_KEYS = {
    "Knuckle",
    "Claw",
    "BeastClaw",
    "HandToHand",
    "PerfumeBottle",
    "ReverseHandSword",
}

PAIRED_UNIQUE_NAMES = {
    "Smithscript Dagger",
    "Ornamental Straight Sword",
    "Rellana's Twin Blades",
    "Starscourge Greatsword",
    "Greatsword of Radahn (Lord)",
    "Greatsword of Radahn (Light)",
    "Falx",
    "Death Knight's Twin Axes",
    "Dancing Blade of Ranah",
    "Horned Warrior's Sword",
}


def build_weapon_rules(project_root: Path) -> Path:
    weapons_path = project_root / "data" / "phase1" / "weapons.csv"
    out_path = project_root / "data" / "phase1" / "weapon_rules.csv"

    rules: dict[str, tuple[str, str]] = {}
    with weapons_path.open("r", encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            weapon_name = row["name"]
            type_keys = {
                token.strip()
                for token in row["weapon_type_keys"].split("|")
                if token.strip()
            }
            if type_keys & PAIRED_TYPE_KEYS:
                rules[weapon_name] = ("1", "paired_family")
                continue
            if weapon_name in PAIRED_UNIQUE_NAMES:
                rules[weapon_name] = ("1", "paired_unique")

    with out_path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=["weapon_name", "disable_two_hand_bonus", "source"],
        )
        writer.writeheader()
        for weapon_name in sorted(rules):
            disable, source = rules[weapon_name]
            writer.writerow(
                {
                    "weapon_name": weapon_name,
                    "disable_two_hand_bonus": disable,
                    "source": source,
                }
            )
    return out_path


def main() -> None:
    project_root = Path(__file__).resolve().parents[2]
    out_path = build_weapon_rules(project_root)
    print(f"Wrote weapon rules to {out_path}")


if __name__ == "__main__":
    main()
