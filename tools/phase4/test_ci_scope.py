from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from tools.phase4.ci_scope import changed_paths, main, select_jobs


class CiScopeTests(unittest.TestCase):
    def test_conservative_scope(self) -> None:
        cases = [
            (["README.md", "LICENSE", "docs/release-notes/v0.14.1.md"], (False, False)),
            (["docs/assets/logo.svg"], (False, False)),
            (["apps/desktop/src/App.tsx", "docs/performance.md"], (False, True)),
            (["apps/desktop/tests/dto-contract.spec.ts"], (False, True)),
            (["apps/desktop/vite.config.ts", "apps/desktop/scripts/run-e2e.mjs"], (False, True)),
            (["core/er_optimizer_core/src/math.rs"], (True, True)),
            (["apps/desktop/src-tauri/build.rs"], (True, True)),
            (["apps/desktop/src-tauri/icons/icon.ico"], (True, True)),
            (["apps/desktop/package-lock.json"], (True, True)),
            (["data/phase1/weapons.csv", "apps/desktop/src/App.tsx"], (True, True)),
            (["tools/phase4/ci_scope.py"], (True, True)),
            ([".github/workflows/ci.yml"], (True, True)),
            ([".node-version", "docs/releasing.md"], (True, True)),
            (["docs/new-script.py"], (True, True)),
            (["unknown/new-file", "README.md"], (True, True)),
            ([], (True, True)),
            ([f"docs/{i}.md" for i in range(350)] + ["core/new.rs"], (True, True)),
            (["docs/space and unicode-\u03b1.md", "docs/new\nline.md"], (False, False)),
        ]
        for paths, expected in cases:
            with self.subTest(paths=paths[-2:]):
                self.assertEqual(select_jobs("pull_request", paths), expected)
                for event in ("push", "workflow_dispatch", "unknown"):
                    self.assertEqual(select_jobs(event, paths), (True, True))

    def test_merge_diff_includes_deleted_and_renamed_source(self) -> None:
        scratch = Path(__file__).resolve().parents[2] / ".codex-tmp"
        scratch.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            root = Path(directory)

            def git(*args: str) -> bytes:
                return subprocess.check_output(["git", *args], cwd=root, stderr=subprocess.PIPE)

            git("init", "--initial-branch=main")
            git("config", "user.name", "CI fixture")
            git("config", "user.email", "ci-fixture@example.invalid")
            (root / "core").mkdir()
            (root / "core/old.rs").write_text("source\n", encoding="utf-8")
            (root / "core/deleted.rs").write_text("deleted\n", encoding="utf-8")
            git("add", ".")
            git("commit", "-m", "Base fixture")
            with self.assertRaisesRegex(RuntimeError, "merge commit"):
                changed_paths(root)
            git("switch", "-c", "change")
            (root / "docs").mkdir()
            git("mv", "core/old.rs", "docs/moved.md")
            git("rm", "core/deleted.rs")
            (root / "docs/space \u03b1.md").write_text("docs\n", encoding="utf-8")
            git("add", ".")
            git("commit", "-m", "Move and delete fixture")
            git("switch", "main")
            git("merge", "--no-ff", "change", "-m", "Merge fixture")
            paths = changed_paths(root)
            self.assertEqual(set(paths), {
                "core/old.rs", "core/deleted.rs", "docs/moved.md", "docs/space \u03b1.md",
            })
            self.assertEqual(select_jobs("pull_request", paths), (True, True))
            git("clone", "--depth", "2", root.as_uri(), "shallow")
            self.assertEqual(set(changed_paths(root / "shallow")), set(paths))

    def test_failed_diff_never_emits_skip_outputs(self) -> None:
        with (
            patch.dict(os.environ, {"GITHUB_EVENT_NAME": "pull_request"}),
            patch("tools.phase4.ci_scope.subprocess.check_output", side_effect=subprocess.CalledProcessError(128, "git")),
            self.assertRaises(subprocess.CalledProcessError),
        ):
            main()

    def test_nul_delimited_paths_preserve_newlines_and_unicode(self) -> None:
        names = "docs/space \u03b1.md\0docs/new\nline.md\0core/source.rs\0".encode()
        with patch("tools.phase4.ci_scope.subprocess.check_output", side_effect=[b"a b", names]):
            self.assertEqual(changed_paths(Path.cwd()), [
                "docs/space \u03b1.md", "docs/new\nline.md", "core/source.rs",
            ])


if __name__ == "__main__":
    unittest.main()
