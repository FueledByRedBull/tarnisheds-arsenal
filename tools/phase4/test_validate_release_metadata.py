from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools.phase4.validate_release_metadata import (
    markdown_link_errors,
    release_notes_errors,
    toolchain_reference_errors,
)

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


PIN = "dtolnay/rust-toolchain@" + "a" * 40


class ToolchainReferenceTests(unittest.TestCase):
    def errors(self, workflow: str, readme: str) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / ".github" / "workflows").mkdir(parents=True)
            (root / ".github" / "workflows" / "ci.yml").write_text(workflow, encoding="utf-8")
            (root / "README.md").write_text(readme, encoding="utf-8")
            return toolchain_reference_errors(root, "1.97.0", "22.23.1", "3.12.10", "v0.17.2")

    def test_matching_references_pass(self) -> None:
        readme = "**Rust 1.97.0**, **Node.js 22.23.1**, **Python 3.12.10**. [New](docs/release-notes/v0.17.2.md)\n"
        workflow = f"uses: {PIN} # 1.97.0\nnode-version-file: .node-version\n"
        self.assertEqual(self.errors(workflow, readme), [])

    def test_drifted_references_fail(self) -> None:
        readme = "Rust 1.96.0, Node.js 22.23.1, Python 3.12.10. [New](docs/release-notes/v0.17.1.md)\n"
        workflow = f"uses: {PIN} # 1.96.0\npython-version: '3.12'\n"
        errors = self.errors(workflow, readme)
        self.assertEqual(len(errors), 4, errors)
        self.assertIn("installs Rust 1.96.0", errors[0])
        self.assertIn("literal version", errors[1])
        self.assertIn("pinned Rust 1.97.0", errors[2])
        self.assertIn("links v0.17.1 release notes", errors[3])


class MarkdownLinkTests(unittest.TestCase):
    def test_missing_files_and_headings_fail(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs").mkdir()
            (root / "docs" / "guide.md").write_text("# Guide\n## Convergence 3.0.0.1 beta\n## Setup\n## Setup\n", encoding="utf-8")
            (root / "README.md").write_text(
                "[ok](docs/guide.md#convergence-3001-beta) [again](docs/guide.md#setup-1) [web](https://x.test/a)\n"
                "[self](#readme) `[code](missing.md)`\n# README\n"
                "[gone](docs/missing.md) [heading](docs/guide.md#nowhere)\n",
                encoding="utf-8",
            )
            self.assertEqual(markdown_link_errors(root, ["README.md", "docs/guide.md"]), [
                "README.md links missing docs/missing.md",
                "README.md links missing heading docs/guide.md#nowhere",
            ])


if __name__ == "__main__":
    unittest.main()
