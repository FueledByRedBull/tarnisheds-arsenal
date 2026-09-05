from __future__ import annotations

import json
import tomllib
import unittest

from tools.phase4.prepare_release import ROOT, version_updates


class PrepareReleaseTests(unittest.TestCase):
    def test_bump_changes_only_local_package_versions(self) -> None:
        updates = version_updates(ROOT, "0.12.0")
        self.assertEqual(len(updates), 7)
        for path, text in updates.items():
            if path.suffix == ".json":
                before = json.loads(path.read_text(encoding="utf-8"))
                after = json.loads(text)
                self.assertEqual(after["version"], "0.12.0")
                after["version"] = before["version"]
                if path.name == "package-lock.json":
                    self.assertEqual(after["packages"][""]["version"], "0.12.0")
                    after["packages"][""]["version"] = before["packages"][""]["version"]
            else:
                before = tomllib.loads(path.read_text(encoding="utf-8"))
                after = tomllib.loads(text)
                old = before["package"] if path.suffix == ".lock" else [before["package"]]
                new = after["package"] if path.suffix == ".lock" else [after["package"]]
                for previous, updated in zip(old, new, strict=True):
                    if previous["name"] in ("er_optimizer_core", "tarnisheds-arsenal-desktop"):
                        self.assertEqual(updated["version"], "0.12.0")
                        updated["version"] = previous["version"]
            self.assertEqual(after, before, str(path))

    def test_rejects_invalid_or_uninstallable_versions(self) -> None:
        for version in ("v1.2.3", "1.2", "01.2.3", "256.0.0", "1.0.65536"):
            with self.assertRaises(ValueError):
                version_updates(ROOT, version)
