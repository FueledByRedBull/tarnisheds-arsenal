"""Exercise the required CI check against failed, skipped and cancelled jobs."""
from __future__ import annotations

import itertools
import os
from pathlib import Path
import shutil
import subprocess
import textwrap
import unittest


@unittest.skipUnless(shutil.which("pwsh"), "PowerShell is required to exercise the CI gate")
class CiWorkflowTests(unittest.TestCase):
    def test_required_check_only_accepts_both_windows_jobs_succeeding(self) -> None:
        workflow = (Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml").read_text(
            encoding="utf-8"
        )
        gate = workflow.split("\n  rust-and-data:\n", 1)[1].split(
            "\n  windows-debug-and-data:\n", 1
        )[0]
        self.assertIn("needs: [windows-debug-and-data, windows-release]", gate)
        self.assertIn("if: ${{ always() }}", gate)
        self.assertIn("DEBUG_RESULT: ${{ needs.windows-debug-and-data.result }}", gate)
        self.assertIn("RELEASE_RESULT: ${{ needs.windows-release.result }}", gate)
        script = textwrap.dedent(gate.split("        run: |\n", 1)[1])
        for debug, release in itertools.product(
            ("success", "failure", "cancelled", "skipped"), repeat=2
        ):
            with self.subTest(debug=debug, release=release):
                result = subprocess.run(
                    ["pwsh", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
                    env={**os.environ, "DEBUG_RESULT": debug, "RELEASE_RESULT": release},
                    capture_output=True,
                    text=True,
                    check=False,
                    timeout=30,
                )
                self.assertEqual(
                    result.returncode == 0,
                    debug == release == "success",
                    result.stderr,
                )


if __name__ == "__main__":
    unittest.main()
