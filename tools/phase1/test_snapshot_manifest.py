from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from typing import Any

from tools.phase1.profiles import profile_definition
from tools.phase1.snapshot_manifest import (
    RUNTIME_FILES,
    promote_snapshot,
    write_snapshot_manifest,
)


class SnapshotPromotionTests(unittest.TestCase):
    def _create_staging(self, root: Path) -> Path:
        staging = root / "staging"
        staging.mkdir()
        for file_name in RUNTIME_FILES:
            (staging / file_name).write_text(f"{file_name}\n", encoding="utf-8")
        (staging / "listed.csv").write_text("listed\n", encoding="utf-8")
        regulation = root / "regulation.bin"
        regulation.write_bytes(b"regulation")
        write_snapshot_manifest(
            staging,
            regulation,
            profile=profile_definition("convergence"),
            generated_at="2026-08-17",
        )
        return staging

    def _create_destination(self, root: Path) -> Path:
        destination = root / "destination"
        destination.mkdir()
        (destination / "stale.csv").write_text("stale\n", encoding="utf-8")
        (destination / "listed.csv").write_text("old\n", encoding="utf-8")
        (destination / "keep.txt").write_text("keep\n", encoding="utf-8")
        return destination

    @staticmethod
    def _files(directory: Path) -> dict[str, bytes]:
        return {
            path.relative_to(directory).as_posix(): path.read_bytes()
            for path in directory.rglob("*")
            if path.is_file()
        }

    @staticmethod
    def _write_manifest(staging: Path, manifest: dict[str, Any]) -> None:
        (staging / "manifest.json").write_text(
            json.dumps(manifest, indent=2) + "\n",
            encoding="utf-8",
        )

    def test_promotion_reconciles_csvs_and_preserves_non_csv_files(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            staging = self._create_staging(root)
            destination = self._create_destination(root)

            promote_snapshot(staging, destination)

            self.assertFalse((destination / "stale.csv").exists())
            self.assertEqual((destination / "listed.csv").read_text(), "listed\n")
            self.assertEqual((destination / "keep.txt").read_text(), "keep\n")
            self.assertEqual(
                {path.name for path in destination.glob("*.csv")},
                {path.name for path in staging.glob("*.csv")},
            )

    def test_malformed_file_record_is_rejected_before_destination_mutation(self) -> None:
        for invalid_path in (None, "nested/listed.csv", r"nested\listed.csv", "C:listed.csv"):
            with self.subTest(invalid_path=invalid_path), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                staging = self._create_staging(root)
                destination = self._create_destination(root)
                before = self._files(destination)
                manifest = json.loads((staging / "manifest.json").read_text(encoding="utf-8"))
                manifest["diagnosticFiles"][0]["path"] = invalid_path
                self._write_manifest(staging, manifest)

                with self.assertRaises(ValueError):
                    promote_snapshot(staging, destination)
                self.assertEqual(self._files(destination), before)

    def test_bundled_source_subdirectory_is_rejected_before_destination_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            staging = self._create_staging(root)
            destination = self._create_destination(root)
            before = self._files(destination)

            source = staging / "nested" / "workbook.xlsx"
            source.parent.mkdir()
            source.write_bytes(b"workbook")
            manifest = json.loads((staging / "manifest.json").read_text(encoding="utf-8"))
            manifest["sources"].append(
                {
                    "kind": "workbook",
                    "bundled": True,
                    "path": "nested/workbook.xlsx",
                    "size": source.stat().st_size,
                    "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                }
            )
            self._write_manifest(staging, manifest)

            with self.assertRaises(ValueError):
                promote_snapshot(staging, destination)
            self.assertEqual(self._files(destination), before)


if __name__ == "__main__":
    unittest.main()
