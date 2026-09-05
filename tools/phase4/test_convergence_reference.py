from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path

from tools.phase4.convergence_reference import (
    STATUS_KEYS,
    _canonical_hash,
    _ours_signature,
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
    row["base_physical"] = "10"
    return row


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


if __name__ == "__main__":
    unittest.main()
