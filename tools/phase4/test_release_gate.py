from __future__ import annotations

import contextlib
import io
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase4 import release_gate

SHA = "b" * 40
OTHER = "a" * 40


def annotated(commit: str) -> str:
    return f"{'c' * 40}\trefs/tags/v1.0.0\n{commit}\trefs/tags/v1.0.0^{{}}"


class ReleaseGateTests(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.output = Path(directory.name) / "output"
        self.env_file = Path(directory.name) / "env"
        environment = patch.dict(os.environ, {"GITHUB_OUTPUT": str(self.output), "GITHUB_ENV": str(self.env_file)})
        environment.start()
        self.addCleanup(environment.stop)
        quiet = contextlib.redirect_stdout(io.StringIO())
        self.stdout = quiet.__enter__()
        self.addCleanup(quiet.__exit__, None, None, None)

    def test_manual_publication_requires_the_default_branch(self) -> None:
        release_gate.require_default_branch("refs/heads/main", "main", "main")
        for ref in ("refs/heads/feature", "refs/tags/main", "refs/heads/main2"):
            with self.subTest(ref=ref), self.assertRaisesRegex(SystemExit, "default branch"):
                release_gate.require_default_branch(ref, "feature", "main")

    def test_ci_proof_requires_successful_default_branch_push(self) -> None:
        base = {"databaseId": 123, "status": "completed", "conclusion": "success",
                "headBranch": "main", "headSha": SHA, "event": "push",
                "url": "https://github.com/example/repo/actions/runs/123", "createdAt": "2026-09-22T00:00:00Z"}
        cases = [({}, True), ({"conclusion": "failure"}, False),
                 ({"conclusion": "cancelled"}, False), ({"status": "in_progress"}, False),
                 ({"headSha": OTHER}, False), ({"headBranch": "feature"}, False),
                 ({"event": "pull_request"}, False)]
        for change, success in cases:
            with self.subTest(change=change):
                self.output.unlink(missing_ok=True)

                def gh(*command: str, runs=({**base, **change},)) -> str:
                    self.assertEqual(command[:3], ("gh", "run", "list"))
                    for flag, value in (("--workflow", "ci.yml"), ("--event", "push"), ("--commit", SHA)):
                        self.assertEqual(command[command.index(flag) + 1], value)
                    return json.dumps(list(runs))

                with (patch.object(release_gate, "run", side_effect=gh),
                      patch.object(release_gate.time, "sleep", side_effect=RuntimeError("would wait"))):
                    if success:
                        release_gate.wait_for_ci(SHA, "main")
                        self.assertEqual(self.output.read_text(encoding="utf-8").strip(), f"verified-sha={SHA}")
                        self.assertIn(base["url"], self.stdout.getvalue())
                    else:
                        with self.assertRaises((SystemExit, RuntimeError)):
                            release_gate.wait_for_ci(SHA, "main")
                        self.assertFalse(self.output.exists())

    def test_ci_proof_uses_the_newest_matching_run_and_rejects_malformed_shas(self) -> None:
        older = {"databaseId": 1, "status": "completed", "conclusion": "success", "headBranch": "main",
                 "headSha": SHA, "event": "push", "url": "old", "createdAt": "2026-09-22T00:00:00Z"}
        newer = {**older, "databaseId": 2, "conclusion": "failure", "url": "new", "createdAt": "2026-09-22T01:00:00Z"}
        with patch.object(release_gate, "run", return_value=json.dumps([older, newer])):
            with self.assertRaisesRegex(SystemExit, "CI run 2 concluded failure: new"):
                release_gate.wait_for_ci(SHA, "main")
        for sha in ("", SHA.upper(), SHA[:-1]):
            with self.subTest(sha=sha), self.assertRaisesRegex(SystemExit, "Invalid source commit SHA"):
                release_gate.wait_for_ci(sha, "main")

    def metadata(self, ref: str, publish: bool, local_tag: str, tag_commit: str) -> None:
        def git(*command: str) -> str:
            return local_tag if command[1] == "tag" else tag_commit

        validator = subprocess.CompletedProcess([], 0)
        with (patch.object(release_gate, "run", side_effect=git),
              patch.object(release_gate.subprocess, "run", return_value=validator) as run):
            release_gate.release_metadata(ref, ref.rsplit("/", 1)[-1], SHA, publish)
        tag = self.env_file.read_text(encoding="utf-8").rsplit("RELEASE_TAG=", 1)[1].split("\n", 1)[0]
        self.assertEqual(run.call_args.args[0][1:], ["tools/phase4/validate_release_metadata.py", "--tag", tag])

    @property
    def version(self) -> str:
        return json.loads((release_gate.ROOT / "apps/desktop/src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"]

    def test_preview_allows_an_existing_tag_and_publication_rejects_a_moved_one(self) -> None:
        tag = f"v{self.version}"
        self.metadata("refs/heads/preview", False, tag, OTHER)
        written = self.env_file.read_text(encoding="utf-8")
        self.assertIn(f"RELEASE_VERSION={self.version}\nRELEASE_TAG={tag}\n", written)
        self.assertIn(f"ARTIFACT_VERSION={self.version}-preview-{SHA}", written)
        self.env_file.unlink()
        self.metadata(f"refs/tags/{tag}", True, tag, SHA)
        self.assertIn(f"ARTIFACT_VERSION={self.version}\n", self.env_file.read_text(encoding="utf-8"))
        self.metadata("refs/heads/main", True, "", "")
        with self.assertRaisesRegex(SystemExit, f"Release tag {tag} already points to {OTHER}"):
            self.metadata("refs/heads/main", True, tag, OTHER)

    def test_packaging_reuses_only_matching_ci_proof(self) -> None:
        cases = [
            (False, "refs/heads/main", SHA, ["--preview", "--skip-validation"]),
            (False, "refs/heads/feature", "", ["--preview"]),
            (True, "refs/heads/main", SHA, ["--skip-validation"]),
            (True, "refs/tags/v0.14.1", SHA, ["--skip-validation"]),
            (False, "refs/heads/main", "", None),
            (True, "refs/heads/main", "", None),
            (False, "refs/heads/feature", SHA, None),
            (False, "refs/tags/main", SHA, None),
            (False, "refs/heads/main", OTHER, None),
            (True, "refs/heads/main", "malformed", None),
        ]
        for publish, ref, proof, expected in cases:
            with self.subTest(publish=publish, ref=ref, proof=proof):
                arguments = lambda: release_gate.package_arguments(publish, "workflow_dispatch", ref, "main", SHA, proof)  # noqa: E731
                if expected is None:
                    self.assertRaises(SystemExit, arguments)
                else:
                    self.assertEqual(arguments(), expected)

    def tag_gate(self, event: str, remote: list[str], local_tag: str = "", local_commit: str = "") -> list[tuple[str, ...]]:
        """remote lists successive ls-remote outputs; an annotated tag also reports its peeled commit."""
        calls: list[tuple[str, ...]] = []

        def git(*command: str) -> str:
            calls.append(command)
            if command[1] == "ls-remote":
                self.assertEqual(command[2:], ("origin", "refs/tags/v1.0.0", "refs/tags/v1.0.0^{}"))
                return remote.pop(0)
            if command[1] == "tag" and command[2] == "--list":
                return local_tag
            if command[1] == "rev-list":
                return local_commit
            return ""

        with (patch.object(release_gate, "run", side_effect=git),
              patch.object(release_gate.subprocess, "run") as push):
            try:
                release_gate.release_tag("v1.0.0", SHA, event)
            finally:
                calls.extend(tuple(call.args[0]) for call in push.call_args_list)
        return calls

    def test_manual_publication_creates_pushes_and_verifies_the_tag(self) -> None:
        calls = self.tag_gate("workflow_dispatch", ["", annotated(SHA)])
        self.assertIn(("git", "tag", "-a", "v1.0.0", "-m", "Release v1.0.0", SHA), calls)
        self.assertIn(("git", "push", "origin", "v1.0.0"), calls)
        calls = self.tag_gate("workflow_dispatch", ["", annotated(SHA)], local_tag="v1.0.0", local_commit=SHA)
        self.assertFalse(any(call[1:3] == ("tag", "-a") for call in calls))
        self.assertIn(("git", "push", "origin", "v1.0.0"), calls)

    def test_tag_push_or_existing_tag_is_only_verified(self) -> None:
        for event in ("push", "workflow_dispatch"):
            with self.subTest(event=event):
                calls = self.tag_gate(event, [annotated(SHA), annotated(SHA)])
                self.assertFalse(any(call[1] in ("tag", "push", "config") for call in calls))
        self.tag_gate("push", [f"{SHA}\trefs/tags/v1.0.0"])

    def test_tag_at_another_commit_or_lost_race_fails(self) -> None:
        with self.assertRaisesRegex(SystemExit, f"Remote release tag v1.0.0 points to {OTHER}; expected {SHA}"):
            self.tag_gate("push", [annotated(OTHER)])
        with self.assertRaisesRegex(SystemExit, f"Local release tag v1.0.0 already points to {OTHER}"):
            self.tag_gate("workflow_dispatch", [""], local_tag="v1.0.0", local_commit=OTHER)
        with self.assertRaisesRegex(SystemExit, f"points to {OTHER}"):
            self.tag_gate("workflow_dispatch", ["", annotated(OTHER)])
        with self.assertRaisesRegex(SystemExit, "points to ; expected"):
            self.tag_gate("push", [""])

    def test_remote_tag_lookup_failure_is_reported(self) -> None:
        with (patch.object(release_gate, "run", side_effect=subprocess.CalledProcessError(2, "git")),
              self.assertRaisesRegex(SystemExit, "Unable to inspect remote release tag v1.0.0")):
            release_gate.remote_tag_commit("v1.0.0")


if __name__ == "__main__":
    unittest.main()
