"""Compare the skill animation extractor with the Vanilla motion workbook.

Vanilla's own animation files sit in the game's encrypted archives, so this runs the
extractor on Vanilla's regulation through the mod's animations and hks. Where the mod left
an animation alone the two independent sources must agree; a difference is either the mod
reassigning a skill or an extractor gap.
"""

from __future__ import annotations

import argparse
import re
import shutil
import sys
import tempfile
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Mapping

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.extract_skill_animations import (  # noqa: E402
    NORMAL_MOVE_ANIMS,
    run_skill_animation_exports,
)
from tools.phase1.phase1_dump import paramdex_path, read_csv, unpack_regulation  # noqa: E402
from tools.phase1.profiles import discover_witchybnd, profile_definition  # noqa: E402

FIRST_NUMBER = re.compile(r"[0-9]+(?:\.[0-9]+)?")


@dataclass(frozen=True)
class AttackAgreement:
    skills: int
    exact_skills: int
    attacks: int
    found_attacks: int


def damaging_attacks(rows: Iterable[Mapping[str, str]]) -> dict[str, set[int]]:
    attacks: dict[str, set[int]] = defaultdict(set)
    for row in rows:
        if row["is_lacking_fp"] == "0" and row["is_damaging"] == "1":
            attacks[row["aow_name"]].add(int(row["atk_id"]))
    return attacks


def compare_attacks(book: Mapping[str, set[int]], extracted: Mapping[str, set[int]]) -> AttackAgreement:
    names = set(book) | set(extracted)
    return AttackAgreement(
        skills=len(names),
        exact_skills=sum(book.get(name, set()) == extracted.get(name, set()) for name in names),
        attacks=sum(len(attacks) for attacks in book.values()),
        found_attacks=sum(len(attacks & extracted.get(name, set())) for name, attacks in book.items()),
    )


def compare_first_hit_poise(
    book: Iterable[Mapping[str, str]], extracted: Iterable[Mapping[str, str]]
) -> dict[str, tuple[int, int]]:
    """(agreeing, total) weapons per move, comparing each move's first listed contact."""
    extracted_by_id = {row["weapon_id"]: row for row in extracted}
    agreeing: Counter[str] = Counter()
    totals: Counter[str] = Counter()
    for row in book:
        for column in NORMAL_MOVE_ANIMS:
            expected = FIRST_NUMBER.search(row[column])
            if expected is None:
                continue
            totals[column] += 1
            actual = FIRST_NUMBER.search(extracted_by_id[row["weapon_id"]][column])
            agreeing[column] += actual is not None and abs(float(actual[0]) - float(expected[0])) < 0.011
    return {column: (agreeing[column], totals[column]) for column in NORMAL_MOVE_ANIMS}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vanilla-regulation", type=Path, default=ROOT / "data/raw/Vanilla/regulation.bin")
    args = parser.parse_args()
    witchybnd = discover_witchybnd(ROOT).resolve()
    sources = profile_definition("convergence").skill_animation_sources
    if sources is None:
        raise ValueError("the Convergence profile names no skill animation sources")
    book_dir = ROOT / "data/phase1"
    with tempfile.TemporaryDirectory(prefix="skill-compare-") as temp:
        snapshot = Path(temp) / "snapshot"
        shutil.copytree(book_dir, snapshot, ignore=shutil.ignore_patterns("*.xlsx"))
        regulation = unpack_regulation(args.vanilla_regulation, witchybnd, Path(temp))
        run_skill_animation_exports(
            snapshot,
            regulation,
            paramdex_path(witchybnd, "Defs"),
            sources,
            witchybnd,
            Path(temp),
            check_regulation=False,
        )
        attacks = compare_attacks(
            damaging_attacks(read_csv(book_dir / "aow_attack_data.csv")),
            damaging_attacks(read_csv(snapshot / "aow_attack_data.csv")),
        )
        poise = compare_first_hit_poise(read_csv(book_dir / "weapons.csv"), read_csv(snapshot / "weapons.csv"))
    print(f"Ashes with identical damaging attacks: {attacks.exact_skills} of {attacks.skills}")
    print(f"Workbook attacks found: {attacks.found_attacks} of {attacks.attacks}")
    for column, (agreeing, total) in poise.items():
        print(f"{column}: {agreeing} of {total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
