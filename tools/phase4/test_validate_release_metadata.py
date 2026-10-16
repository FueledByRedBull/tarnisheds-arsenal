from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools.phase4.validate_release_metadata import release_notes_errors

URL = "https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/tag/"


class ReleaseNotesTests(unittest.TestCase):
    def errors(self, notes: str | None, index: str) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            notes_dir = root / "docs" / "release-notes"
            notes_dir.mkdir(parents=True)
            (notes_dir / "README.md").write_text(index, encoding="utf-8")
            if notes is not None:
                (notes_dir / "v0.17.2.md").write_text(notes, encoding="utf-8")
            return release_notes_errors(root, "v0.17.2")

    def test_complete_notes_pass(self) -> None:
        self.assertEqual(self.errors("# v0.17.2\n", f"- [v0.17.2]({URL}v0.17.2).\n"), [])

    def test_missing_or_empty_notes_fail(self) -> None:
        index = f"- [v0.17.2]({URL}v0.17.2)\n"
        for notes in (None, "", "  \n"):
            with self.subTest(notes=notes):
                self.assertIn("missing or empty release notes", self.errors(notes, index)[0])

    def test_longer_tag_does_not_satisfy_the_index(self) -> None:
        errors = self.errors("# Notes\n", f"- [v0.17.20]({URL}v0.17.20)\n- v0.17.2-beta\n")
        self.assertEqual(len(errors), 2)
        self.assertIn("does not mention v0.17.2", errors[0])
        self.assertIn("must link v0.17.2", errors[1])


if __name__ == "__main__":
    unittest.main()
