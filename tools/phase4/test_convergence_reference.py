from __future__ import annotations

import csv
import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase4 import convergence_reference
from tools.phase4.convergence_reference import (
    STATUS_KEYS,
    _canonical_hash,
    _ours_corrections,
    _ours_signature,
    build_reference,
    validate_reference,
)


def weapon_row(weapon_id: int, affinity: str) -> dict[str, str]:
    row = {
        "weapon_id": str(weapon_id),
        "affinity": affinity,
        "weapon_type_id": "1",
        "reinforce_type": "0",
        "disable_two_hand_bonus": "0",
    }
    for stat in ("str", "dex", "int", "fai", "arc"):
        row[f"req_{stat}"] = "0"
        row[f"{stat}_scaling"] = "0"
    for damage in ("physical", "magic", "fire", "lightning", "holy"):
        row[f"base_{damage}"] = "0"
        row[f"curve_id_{damage}"] = "0"
    for status in ("poison", "blood", "sleep", "madness"):
        row[f"curve_id_{status}"] = "28"
    row["base_physical"] = "10"
    row["attack_element_correct_id"] = "16000"
    return row


def write_csv(path: Path, rows: list[dict[str, str]]) -> None:
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=rows[0].keys())
        writer.writeheader()
        writer.writerows(rows)


class ConvergenceReferenceTests(unittest.TestCase):
    def test_affinity_label_mutation_is_rejected(self) -> None:
        rows = [weapon_row(1, "Standard"), weapon_row(2, "Heavy")]
        modeled = [[int(row["weapon_id"]), row["affinity"], _ours_signature(row)] for row in rows]
        passives = [
            {"weapon_id": row["weapon_id"], **{key: "0" for key in STATUS_KEYS}}
            for row in rows
        ]
        reference = {
            "weapons": [
                {"weaponId": 1, "affinity": "Standard"},
                {"weaponId": 2, "affinity": "Heavy"},
            ],
            "modeledWeaponDataSha256": _canonical_hash(modeled),
            "weaponStatusDataSha256": _canonical_hash(
                [[int(row["weapon_id"]), [0.0] * len(STATUS_KEYS)] for row in rows]
            ),
            "weaponCorrectionDataSha256": _canonical_hash(
                [
                    [int(row["weapon_id"]), _ours_corrections(row, passive)]
                    for row, passive in zip(rows, passives, strict=True)
                ]
            ),
        }
        with tempfile.TemporaryDirectory() as temporary:
            snapshot = Path(temporary)
            with (snapshot / "weapons.csv").open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=rows[0].keys())
                writer.writeheader()
                writer.writerows(rows)
            with (snapshot / "weapon_passives.csv").open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=passives[0].keys())
                writer.writeheader()
                writer.writerows(passives)
            reference_path = snapshot / "reference.json"
            reference_path.write_text(json.dumps(reference), encoding="utf-8")

            validate_reference(snapshot, reference_path)

            rows[0]["affinity"], rows[1]["affinity"] = rows[1]["affinity"], rows[0]["affinity"]
            with (snapshot / "weapons.csv").open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=rows[0].keys())
                writer.writeheader()
                writer.writerows(rows)
            with self.assertRaisesRegex(ValueError, "affinities differ"):
                validate_reference(snapshot, reference_path)

            # An omitted XML default replaced by a hard-coded fallback must not pass.
            rows[0]["affinity"], rows[1]["affinity"] = rows[1]["affinity"], rows[0]["affinity"]
            rows[0]["attack_element_correct_id"] = "0"
            write_csv(snapshot / "weapons.csv", rows)
            with self.assertRaisesRegex(ValueError, "correction-curve IDs differ"):
                validate_reference(snapshot, reference_path)

    def test_build_reference_matches_rows_by_affinity_and_signature(self) -> None:
        rows = [{**weapon_row(1, "Standard"), "name": "Blade"}, {**weapon_row(2, "Heavy"), "name": "Blade"}]
        source = {"weaponName": "Blade", "weaponType": 1, "reinforceTypeId": 0, "attack": [[0, 10]],
                  "attackElementCorrectId": 16000}
        reference = {"weapons": [{**source, "affinityId": 1}, {**source, "affinityId": -1}],
                     "statusSpEffectParams": {}}
        with tempfile.TemporaryDirectory() as temporary:
            snapshot = Path(temporary)
            with (snapshot / "weapons.csv").open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=rows[0].keys())
                writer.writeheader()
                writer.writerows(rows)
            write_csv(snapshot / "weapon_passives.csv", [
                {"weapon_id": row["weapon_id"], **dict.fromkeys(STATUS_KEYS, "0")} for row in rows
            ])
            with patch.object(convergence_reference, "_fetch_reference", return_value=(b"payload", reference)):
                built = build_reference(snapshot, "https://example.invalid/reference.js")
                self.assertEqual(built["weapons"], [{"weaponId": 1, "affinity": "Standard"},
                                                    {"weaponId": 2, "affinity": "Heavy"}])
                self.assertEqual(built["affinityCounts"], {"Heavy": 1, "Standard": 1})
                reference["weapons"].append({**source, "affinityId": 2})
                with self.assertRaisesRegex(ValueError, "unmatched 2:Blade"):
                    build_reference(snapshot, "https://example.invalid/reference.js")

    def test_candidate_slot_labels_do_not_decide_the_reference_affinity(self) -> None:
        # An unfiltered candidate labels a unique weapon by its ID slot; the reference
        # row says it is a fixed Standard weapon, and that is what gets recorded.
        rows = [{**weapon_row(7, "Heavy"), "name": "Unique Blade"}]
        source = {"weaponName": "Unique Blade", "weaponType": 1, "reinforceTypeId": 0,
                  "attack": [[0, 10]], "attackElementCorrectId": 16000, "affinityId": -1}
        with tempfile.TemporaryDirectory() as temporary:
            snapshot = Path(temporary)
            write_csv(snapshot / "weapons.csv", rows)
            write_csv(snapshot / "weapon_passives.csv", [
                {"weapon_id": "7", **dict.fromkeys(STATUS_KEYS, "0")}
            ])
            data = json.dumps({"weapons": [source], "statusSpEffectParams": {}}).encode()
            (snapshot / "reference.js").write_bytes(data)
            built = build_reference(snapshot, str(snapshot / "reference.js"))
        self.assertEqual(built["weapons"], [{"weaponId": 7, "affinity": "Standard"}])
        self.assertEqual(built["version"], convergence_reference.PROFILE.mod_version)
        self.assertEqual(built["sourceSha256"], hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    unittest.main()
