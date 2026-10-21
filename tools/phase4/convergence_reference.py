from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import sys
import unicodedata
import urllib.request
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.profiles import CONVERGENCE_AFFINITIES, profile_definition  # noqa: E402
from tools.phase4.validate_external_calculator import REFERENCE_COMMIT  # noqa: E402


PROFILE = profile_definition("convergence")
# T. Clark's calculator publishes no data past Convergence 3.0.0.1, so its own data build
# (src/buildData.ts at REFERENCE_COMMIT) is run on the mod files and the output is tracked.
REFERENCE_GENERATOR = f"ThomasJClark/elden-ring-weapon-calculator@{REFERENCE_COMMIT} src/buildData.ts"
DEFAULT_SOURCE = Path(f"data/reference/tclark/regulation-convergence-v{PROFILE.mod_version}.js")
STAT_KEYS = ("str", "dex", "int", "fai", "arc")
DAMAGE_KEYS = ("physical", "magic", "fire", "lightning", "holy")
STATUS_INDEX_TO_KEY = {
    "5": "poison",
    "6": "scarlet_rot",
    "7": "bleed",
    "8": "frost",
    "9": "sleep",
    "10": "madness",
    "11": "death",
}
STATUS_KEYS = ("bleed", "frost", "poison", "scarlet_rot", "sleep", "madness", "death")
# Attack-power indices of the four status types with a weapon correction curve. The
# reference omits a curve equal to its default (0 for damage, 6 for status) and lists
# spell-tool curves the weapon AR does not use, so only curves of attack types the
# weapon actually has are compared; None marks an unused curve.
STATUS_CURVES = (("5", "poison"), ("7", "blood"), ("9", "sleep"), ("10", "madness"))


def _number(value: Any) -> float:
    return round(float(value or 0), 6)


def _ours_signature(row: dict[str, str]) -> tuple[Any, ...]:
    return (
        int(row["weapon_type_id"]),
        tuple(int(row[f"req_{stat}"]) for stat in STAT_KEYS),
        tuple(_number(row[f"base_{damage}"]) for damage in DAMAGE_KEYS),
        tuple(_number(row[f"{stat}_scaling"]) for stat in STAT_KEYS),
        int(row["reinforce_type"]),
        row["disable_two_hand_bonus"] == "1",
    )


def _reference_signature(row: dict[str, Any]) -> tuple[Any, ...]:
    requirements = row.get("requirements", {})
    attacks = {int(kind): _number(value) for kind, value in row.get("attack", [])}
    scaling = {str(stat): _number(value) for stat, value in row.get("attributeScaling", [])}
    return (
        int(row["weaponType"]),
        tuple(int(requirements.get(stat, 0)) for stat in STAT_KEYS),
        tuple(attacks.get(index, 0.0) for index in range(len(DAMAGE_KEYS))),
        tuple(scaling.get(stat, 0.0) for stat in STAT_KEYS),
        int(row["reinforceTypeId"]),
        bool(row.get("paired", False)),
    )


def _used_curves(row: dict[str, str], passive: dict[str, str]) -> tuple[list[bool], bool]:
    has_status = any(_number(passive[key]) > 0 for key in STATUS_KEYS)
    return [_number(row[f"base_{damage}"]) > 0 for damage in DAMAGE_KEYS], has_status


def _ours_corrections(row: dict[str, str], passive: dict[str, str]) -> list[Any]:
    damage_used, status_used = _used_curves(row, passive)
    return [
        int(row["attack_element_correct_id"]),
        [
            int(row[f"curve_id_{damage}"]) if used else None
            for damage, used in zip(DAMAGE_KEYS, damage_used, strict=True)
        ],
        [int(row[f"curve_id_{status}"]) if status_used else None for _, status in STATUS_CURVES],
    ]


def _reference_corrections(
    row: dict[str, Any], ours: dict[str, str], passive: dict[str, str]
) -> list[Any]:
    graphs = row.get("calcCorrectGraphIds") or {}
    damage_used, status_used = _used_curves(ours, passive)
    return [
        int(row["attackElementCorrectId"]),
        [
            int(graphs.get(str(index), 0)) if used else None
            for index, used in enumerate(damage_used)
        ],
        [int(graphs.get(index, 6)) if status_used else None for index, _ in STATUS_CURVES],
    ]


def _name_key(value: str) -> str:
    value = re.sub(r"\s+\[[^\]]+\]$", "", value)
    value = re.sub(r"^NEW\s+-\s+", "", value)
    value = unicodedata.normalize("NFKD", value).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "", value.lower())


def _load_csv(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def _fetch_reference(source: str) -> tuple[bytes, dict[str, Any]]:
    if source.startswith("https://"):
        request = urllib.request.Request(
            source,
            headers={"Accept": "application/json", "User-Agent": "Tarnisheds-Arsenal-Validator/0.9"},
        )
        with urllib.request.urlopen(request, timeout=30) as response:  # noqa: S310 - HTTPS only
            payload = response.read()
    else:
        payload = Path(source).read_bytes()
    return payload, json.loads(payload)


def _canonical_hash(value: Any) -> str:
    payload = json.dumps(value, separators=(",", ":"), sort_keys=True).encode()
    return hashlib.sha256(payload).hexdigest()


def _reference_status(row: dict[str, Any], reference: dict[str, Any]) -> list[float]:
    values = {key: 0.0 for key in STATUS_KEYS}
    for effect_id in row.get("statusSpEffectParamIds", []):
        effect = reference["statusSpEffectParams"].get(str(effect_id), {})
        for status_index, amount in effect.items():
            status_key = STATUS_INDEX_TO_KEY.get(status_index)
            if status_key is not None:
                values[status_key] += _number(amount)
    return [values[key] for key in STATUS_KEYS]


def build_reference(snapshot: Path, source: str) -> dict[str, Any]:
    payload, reference = _fetch_reference(source)
    ours = _load_csv(snapshot / "weapons.csv")
    passives = {row["weapon_id"]: row for row in _load_csv(snapshot / "weapon_passives.csv")}
    # Our affinity label only breaks ties: a reference candidate labels rows by slot, and
    # the reference decides which rows exist and what affinity each one is.
    by_signature: dict[tuple[Any, ...], list[dict[str, str]]] = defaultdict(list)
    for row in ours:
        by_signature[_ours_signature(row)].append(row)

    matched_weapons: list[dict[str, Any]] = []
    unmatched: list[str] = []
    ambiguous: list[str] = []
    affinity_counts: Counter[str] = Counter()
    used_ids: set[int] = set()
    modeled_rows: list[list[Any]] = []
    status_rows: list[list[Any]] = []
    correction_rows: list[list[Any]] = []
    reference_rows = sorted(reference["weapons"], key=lambda item: int(item["affinityId"]) != -1)
    for row in reference_rows:
        affinity_id = int(row["affinityId"])
        affinity = "Standard" if affinity_id < 0 else CONVERGENCE_AFFINITIES[affinity_id]
        candidates = [
            candidate
            for candidate in by_signature[_reference_signature(row)]
            if int(candidate["weapon_id"]) not in used_ids
        ]
        same_affinity = [candidate for candidate in candidates if candidate["affinity"] == affinity]
        if same_affinity:
            candidates = same_affinity
        named_candidates = [
            candidate for candidate in candidates if _name_key(candidate["name"]) == _name_key(row["weaponName"])
        ]
        if len(named_candidates) == 1:
            candidates = named_candidates

        if not candidates:
            if row["weaponName"] != "Unarmed":
                unmatched.append(f"{affinity_id}:{row['weaponName']}")
            continue
        if len(candidates) > 1:
            ambiguous.append(
                f"{affinity_id}:{row['weaponName']} => "
                + ", ".join(candidate["weapon_id"] for candidate in candidates)
            )
            continue
        matched_id = int(candidates[0]["weapon_id"])
        matched_weapons.append({"weaponId": matched_id, "affinity": affinity})
        modeled_rows.append([matched_id, affinity, _reference_signature(row)])
        status_rows.append([matched_id, _reference_status(row, reference)])
        correction_rows.append(
            [matched_id, _reference_corrections(row, candidates[0], passives[candidates[0]["weapon_id"]])]
        )
        used_ids.add(matched_id)
        affinity_counts[affinity] += 1

    if unmatched or ambiguous:
        detail = "\n".join([*(f"unmatched {item}" for item in unmatched), *(f"ambiguous {item}" for item in ambiguous)])
        raise ValueError(f"reference matching failed:\n{detail}")

    matched_weapons.sort(key=lambda item: item["weaponId"])
    matched_ids = [item["weaponId"] for item in matched_weapons]
    modeled_rows.sort(key=lambda item: item[0])
    status_rows.sort(key=lambda item: item[0])
    correction_rows.sort(key=lambda item: item[0])
    canonical_ids = "\n".join(str(value) for value in matched_ids).encode()
    return {
        "profile": "convergence",
        "version": PROFILE.mod_version,
        "source": source,
        "generator": REFERENCE_GENERATOR,
        "sourceSha256": hashlib.sha256(payload).hexdigest(),
        "sourceWeaponCount": len(reference["weapons"]),
        "excludedNonItemRows": ["Unarmed"],
        "matchedWeaponCount": len(matched_ids),
        "matchedWeaponIdsSha256": hashlib.sha256(canonical_ids).hexdigest(),
        "modeledWeaponDataSha256": _canonical_hash(modeled_rows),
        "weaponStatusDataSha256": _canonical_hash(status_rows),
        "weaponCorrectionDataSha256": _canonical_hash(correction_rows),
        "affinityCounts": dict(sorted(affinity_counts.items())),
        "weapons": matched_weapons,
    }


def validate_reference(snapshot: Path, reference_path: Path) -> None:
    reference = json.loads(reference_path.read_text(encoding="utf-8"))
    actual_rows = _load_csv(snapshot / "weapons.csv")
    actual_ids = sorted(int(row["weapon_id"]) for row in actual_rows)
    expected_ids = sorted(int(row["weaponId"]) for row in reference["weapons"])
    if actual_ids != expected_ids:
        missing = sorted(set(expected_ids) - set(actual_ids))
        extra = sorted(set(actual_ids) - set(expected_ids))
        raise ValueError(
            f"Convergence weapon availability differs from reference: missing={missing[:20]}, extra={extra[:20]}"
        )
    affinity_by_id = {
        int(row["weaponId"]): str(row["affinity"])
        for row in reference["weapons"]
    }
    affinity_mismatches = [
        (int(row["weapon_id"]), row["affinity"], affinity_by_id[int(row["weapon_id"])])
        for row in actual_rows
        if row["affinity"] != affinity_by_id[int(row["weapon_id"])]
    ]
    if affinity_mismatches:
        raise ValueError(
            "Convergence weapon affinities differ from the external reference: "
            f"{affinity_mismatches[:10]}"
        )
    modeled_rows = [
        [int(row["weapon_id"]), row["affinity"], _ours_signature(row)]
        for row in actual_rows
    ]
    modeled_rows.sort(key=lambda item: item[0])
    if _canonical_hash(modeled_rows) != reference["modeledWeaponDataSha256"]:
        raise ValueError("Convergence modeled weapon data differs from the external reference")

    passive_by_id = {
        int(row["weapon_id"]): row
        for row in _load_csv(snapshot / "weapon_passives.csv")
    }
    status_rows = [
        [weapon_id, [_number(passive_by_id[weapon_id][key]) for key in STATUS_KEYS]]
        for weapon_id in expected_ids
    ]
    if _canonical_hash(status_rows) != reference["weaponStatusDataSha256"]:
        raise ValueError("Convergence weapon status data differs from the external reference")

    correction_rows = sorted(
        [int(row["weapon_id"]), _ours_corrections(row, passive_by_id[int(row["weapon_id"])])]
        for row in actual_rows
    )
    if _canonical_hash(correction_rows) != reference["weaponCorrectionDataSha256"]:
        raise ValueError(
            "Convergence attack-element or correction-curve IDs differ from the external reference"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description="Build or validate the Convergence weapon reference.")
    parser.add_argument("--snapshot", type=Path, default=PROFILE.output_dir)
    parser.add_argument("--reference", type=Path, default=PROFILE.weapon_reference_path)
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument(
        "--source",
        default=DEFAULT_SOURCE.as_posix(),
        help="T. Clark regulation data (repository path or HTTPS URL) the reference is built from",
    )
    args = parser.parse_args()

    if args.refresh:
        reference = build_reference(args.snapshot, args.source)
        args.reference.parent.mkdir(parents=True, exist_ok=True)
        args.reference.write_text(json.dumps(reference, indent=2) + "\n", encoding="utf-8", newline="\n")
        print(f"Wrote {reference['matchedWeaponCount']} weapon IDs to {args.reference}")
    else:
        validate_reference(args.snapshot, args.reference)
        print(f"Validated Convergence weapon availability against {args.reference}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
