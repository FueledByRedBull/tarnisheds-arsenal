"""Exercise the required CI check against failed, skipped and cancelled jobs."""
from __future__ import annotations

import itertools
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import textwrap
import unittest


@unittest.skipUnless(shutil.which("pwsh"), "PowerShell is required to exercise the CI gate")
class CiWorkflowTests(unittest.TestCase):
    def test_required_check_only_accepts_success_or_explicitly_unselected_jobs(self) -> None:
        workflow = (Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml").read_text(
            encoding="utf-8"
        )
        gate = workflow.split("\n  rust-and-data:\n", 1)[1].split(
            "\n  windows-debug-and-data:\n", 1
        )[0]
        self.assertIn("needs: [select-checks, windows-debug-and-data, windows-release, linux-core-and-data, desktop-frontend]", gate)
        self.assertIn("if: ${{ always() }}", gate)
        self.assertIn("DEBUG_RESULT: ${{ needs.windows-debug-and-data.result }}", gate)
        self.assertIn("RELEASE_RESULT: ${{ needs.windows-release.result }}", gate)
        for job, output in (("windows-debug-and-data", "rust"), ("windows-release", "rust"),
                            ("linux-core-and-data", "rust"), ("desktop-frontend", "frontend")):
            section = re.split(r"\n  [a-z]", workflow.split(f"\n  {job}:\n", 1)[1], maxsplit=1)[0]
            self.assertIn("needs: select-checks", section)
            self.assertIn("if: ${{ needs.select-checks.outputs." + output + " == 'true' }}", section)
        script = textwrap.dedent(gate.split("        run: |\n", 1)[1])
        cases = []
        for rust, frontend in (("true", "true"), ("false", "true"), ("false", "false")):
            expected = "success" if rust == "true" else "skipped"
            baseline = {"SCOPE_RESULT": "success", "RUST_SELECTED": rust,
                        "FRONTEND_SELECTED": frontend, "DEBUG_RESULT": expected,
                        "RELEASE_RESULT": expected, "LINUX_RESULT": expected,
                        "FRONTEND_RESULT": "success" if frontend == "true" else "skipped"}
            cases.append({"env": baseline, "pass": True})
            for debug, release in itertools.product(
                ("success", "failure", "cancelled", "skipped"), repeat=2
            ):
                cases.append({"env": {**baseline, "DEBUG_RESULT": debug, "RELEASE_RESULT": release},
                              "pass": debug == release == expected})
            for key in ("SCOPE_RESULT", "LINUX_RESULT", "FRONTEND_RESULT"):
                for value in ("success", "failure", "cancelled", "skipped"):
                    cases.append({"env": {**baseline, key: value}, "pass": value == baseline[key]})
            for key in ("RUST_SELECTED", "FRONTEND_SELECTED"):
                for value in ("", "TRUE", "invalid"):
                    cases.append({"env": {**baseline, key: value}, "pass": False})
        cases.append({"env": {**cases[0]["env"], "FRONTEND_SELECTED": "false"}, "pass": False})
        harness = '''
$gate = {
''' + script + '''
}
$cases = $env:GATE_CASES | ConvertFrom-Json
foreach ($case in $cases) {
    foreach ($entry in $case.env.PSObject.Properties) {
        [Environment]::SetEnvironmentVariable($entry.Name, [string]$entry.Value)
    }
    $passed = $true
    try { & $gate } catch { $passed = $false }
    if ($passed -ne $case.pass) { throw "Wrong gate outcome: $($case | ConvertTo-Json -Compress)" }
}
'''
        result = subprocess.run(
            ["pwsh", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", harness],
            env={**os.environ, "GATE_CASES": json.dumps(cases)},
            capture_output=True, text=True, check=False, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
