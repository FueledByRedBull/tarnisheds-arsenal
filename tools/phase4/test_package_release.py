from __future__ import annotations

import hashlib
import contextlib
import io
import os
import shutil
import subprocess
import tempfile
import textwrap
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

from tools.phase4 import package_release


def workflow_script(name: str) -> str:
    workflow = (Path(__file__).resolve().parents[2] / ".github/workflows/release-package.yml").read_text(encoding="utf-8")
    step = workflow.split(f"      - name: {name}\n", 1)[1].split("\n      - name:", 1)[0]
    lines = []
    for line in step.split("        run: |\n", 1)[1].splitlines():
        if line and not line.startswith("          "):
            break
        lines.append(line)
    return textwrap.dedent("\n".join(lines))


class PackageReleaseTests(unittest.TestCase):
    def test_workflow_wires_ci_proof_to_only_default_branch_previews(self) -> None:
        workflow = (Path(__file__).resolve().parents[2] / ".github/workflows/release-package.yml").read_text(encoding="utf-8")
        self.assertIn("verified-sha: ${{ steps.verify.outputs.verified-sha }}", workflow)
        self.assertIn("VERIFIED_CI_SHA: ${{ needs.verify-ci.outputs.verified-sha }}", workflow)
        self.assertIn("if: startsWith(github.ref, 'refs/tags/') || inputs.publish == true || (github.event_name == 'workflow_dispatch' && github.ref == format('refs/heads/{0}', github.event.repository.default_branch))", workflow)
        self.assertIn("    needs: verify-ci", workflow)

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 is required")
    def test_workflow_preview_allows_an_existing_tag_and_runs_source_validation(self) -> None:
        root = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory() as directory:
            env = os.environ.copy()
            env.update({"GITHUB_ENV": str(Path(directory) / "env"), "GITHUB_SHA": "b" * 40,
                        "GITHUB_REF": "refs/heads/preview", "GITHUB_REF_NAME": "preview"})
            # Simulate an existing tag at a different commit; execute the workflow's actual scripts.
            prelude = '''
function git {
  $global:LASTEXITCODE = 0
  if ($args[0] -eq "tag") { "v0.12.0" } else { "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }
}
function python { $global:LASTEXITCODE = 0; Write-Output ($args -join " ") }
'''
            # Use the repository's configured version, so this test survives a release bump.
            version = package_release.json.loads((root / "apps/desktop/src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"]
            prelude = prelude.replace("v0.12.0", f"v{version}")
            for publish in ["false", "true"]:
                env["PUBLISH_RELEASE"] = publish
                result = subprocess.run(
                    ["pwsh", "-NoProfile", "-NonInteractive", "-Command", prelude + workflow_script("Validate release metadata")],
                    cwd=root, env=env, capture_output=True, text=True, check=False,
                )
                if publish == "false":
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn(f"ARTIFACT_VERSION={version}-preview-{'b' * 40}", Path(env["GITHUB_ENV"]).read_text(encoding="utf-8-sig"))
                else:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("already points to", result.stderr)

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 is required")
    def test_workflow_ci_proof_requires_successful_default_branch_push(self) -> None:
        sha = "b" * 40
        base = {"databaseId": 123, "status": "completed", "conclusion": "success",
                "headBranch": "main", "headSha": sha, "event": "push",
                "url": "https://github.com/example/repo/actions/runs/123", "createdAt": "2026-09-22T00:00:00Z"}
        prelude = '''
function gh {
  if ($args[0] -ne "run" -or $args[1] -ne "list" -or
      $args[$args.IndexOf("--workflow") + 1] -ne "ci.yml" -or
      $args[$args.IndexOf("--event") + 1] -ne "push" -or
      $args[$args.IndexOf("--commit") + 1] -ne $env:GITHUB_SHA) { throw "Incorrect CI query" }
  $global:LASTEXITCODE = 0
  $env:MOCK_RUNS
}
function Start-Sleep { throw "No matching completed CI; would wait" }
'''
        cases = [({}, True), ({"conclusion": "failure"}, False),
                 ({"conclusion": "cancelled"}, False), ({"status": "in_progress"}, False),
                 ({"headSha": "a" * 40}, False), ({"headBranch": "feature"}, False),
                 ({"event": "pull_request"}, False)]
        for change, success in cases:
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "output"
                result = subprocess.run(
                    ["pwsh", "-NoProfile", "-NonInteractive", "-Command", prelude + workflow_script("Wait for successful CI on this commit")],
                    env={**os.environ, "GITHUB_SHA": sha, "RELEASE_BRANCH": "main", "GITHUB_OUTPUT": str(output),
                         "MOCK_RUNS": package_release.json.dumps([{**base, **change}])},
                    capture_output=True, text=True, check=False, timeout=30,
                )
                self.assertEqual(result.returncode == 0, success, result.stderr)
                if success:
                    self.assertEqual(output.read_text(encoding="utf-8-sig").strip(), f"verified-sha={sha}")
                    self.assertIn(base["url"], result.stdout)
                else:
                    self.assertFalse(output.exists())

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 is required")
    def test_workflow_packaging_reuses_only_matching_ci_proof(self) -> None:
        sha = "b" * 40
        cases = [
            ("false", "refs/heads/main", sha, "--preview --skip-validation"),
            ("false", "refs/heads/feature", "", "--preview"),
            ("true", "refs/heads/main", sha, "--skip-validation"),
            ("true", "refs/tags/v0.14.1", sha, "--skip-validation"),
            ("false", "refs/heads/main", "", None),
            ("true", "refs/heads/main", "", None),
            ("false", "refs/heads/feature", sha, None),
            ("false", "refs/tags/main", sha, None),
            ("false", "refs/heads/main", "a" * 40, None),
            ("true", "refs/heads/main", "malformed", None),
        ]
        prelude = 'function python { $global:LASTEXITCODE = 0; Write-Output ($args -join " ") }\n'
        for publish, ref, proof, expected in cases:
            with self.subTest(publish=publish, ref=ref, proof=proof):
                result = subprocess.run(
                    ["pwsh", "-NoProfile", "-NonInteractive", "-Command", prelude + workflow_script("Build Tauri release package")],
                    env={**os.environ, "GITHUB_SHA": sha, "RELEASE_BRANCH": "main", "VERIFIED_CI_SHA": proof,
                         "PUBLISH_RELEASE": publish, "GITHUB_REF": ref, "GITHUB_EVENT_NAME": "workflow_dispatch"},
                    capture_output=True, text=True, check=False, timeout=30,
                )
                self.assertEqual(result.returncode == 0, expected is not None, result.stderr)
                if expected is not None:
                    self.assertEqual(result.stdout.strip(), f"tools/phase4/package_release.py {expected}")
                else:
                    self.assertNotIn("tools/phase4/package_release.py", result.stdout)

    def test_preview_with_ci_validation_keeps_package_checks(self) -> None:
        with tempfile.TemporaryDirectory() as directory, contextlib.ExitStack() as stack:
            root = Path(directory)
            tauri = root / "apps/desktop/src-tauri"
            tauri.mkdir(parents=True)
            (tauri / "tauri.conf.json").write_text('{"version":"0.14.1","productName":"Test"}', encoding="utf-8")
            for path, profile in [("data/phase1", "vanilla"), ("data/profiles/convergence", "convergence")]:
                target = root / path
                target.mkdir(parents=True)
                (target / "manifest.json").write_text(package_release.json.dumps({"id": profile + "-test", "profile": {"id": profile}}), encoding="utf-8")
            exe, msi = root / "app.exe", root / "app.msi"
            exe.write_bytes(b"exe")
            msi.write_bytes(b"msi")
            (root / "LICENSE").write_text("Test license", encoding="utf-8")
            stack.enter_context(patch.object(package_release, "__file__", str(root / "tools/phase4/package_release.py")))
            stack.enter_context(patch("sys.argv", ["package_release.py", "--preview", "--skip-validation"]))
            stack.enter_context(patch.object(package_release, "require_clean_source", return_value="b" * 40))
            unchanged = stack.enter_context(patch.object(package_release, "require_unchanged_tracked_source"))
            run = stack.enter_context(patch.object(package_release, "run"))
            stack.enter_context(patch.object(package_release, "sign_release_binaries_if_configured", return_value=(exe, msi, False, None)))
            identity = stack.enter_context(patch.object(package_release, "verify_msi_identity"))
            payload = stack.enter_context(patch.object(package_release, "verify_msi_payload"))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            self.assertEqual(package_release.main(), 0)
            identity.assert_called_once()
            payload.assert_called_once()
            commands = [call.args[0] for call in run.call_args_list]
            self.assertEqual(len(commands), 3)
            self.assertIn("ci", commands[0])
            self.assertIn("tauri", commands[1])
            self.assertIn("--locked", commands[1])
            self.assertIn("./scripts/smoke-packaged.mjs", commands[2])
            self.assertEqual([call.kwargs["stage"] for call in unchanged.call_args_list],
                             ["release validation", "npm ci", "Tauri build", "packaged app smoke"])
            report_path = next((root / "dist").glob("*/build-report.json"))
            report = package_release.json.loads(report_path.read_text(encoding="utf-8"))
            self.assertTrue(report["validationSkipped"])
            self.assertEqual(set(report["completedGates"]), {"frontend-build", "tauri-release-build", "windows-msi-identity", "packaged-app-smoke"})
            self.assertEqual(len(report["artifacts"]), 2)
            self.assertTrue(next((root / "dist").glob("*.zip")).is_file())
            if shutil.which("pwsh"):
                env = {**os.environ, "RELEASE_REPORT": str(report_path), "RELEASE_VERSION": "0.14.1",
                       "GITHUB_SHA": "b" * 40, "VERIFIED_CI_SHA": "b" * 40, "PUBLISH_RELEASE": "false",
                       "ARTIFACT_VERSION": "0.14.1-preview-" + "b" * 40,
                       "RELEASE_EXE": str(next(report_path.parent.glob("*.exe"))),
                       "RELEASE_MSI": str(next(report_path.parent.glob("*.msi"))),
                       "RELEASE_CHECKSUMS": str(report_path.parent / "SHA256SUMS.txt"),
                       "RELEASE_ZIP": str(next((root / "dist").glob("*.zip")))}
                for change, success in [({}, True), ({"validationSkipped": False}, False),
                                        ({"completedGates": ["frontend-build", "tauri-release-build"]}, False)]:
                    with self.subTest(provenance_change=change):
                        report_path.write_text(package_release.json.dumps({**report, **change}), encoding="utf-8")
                        result = subprocess.run(
                            ["pwsh", "-NoProfile", "-NonInteractive", "-Command",
                             'function python { $global:LASTEXITCODE = 0 }\n' + workflow_script("Verify release provenance and checksums")],
                            env=env, capture_output=True, text=True, check=False, timeout=30,
                        )
                        self.assertEqual(result.returncode == 0, success, result.stderr)

    def test_portable_archive_omits_msi_and_scopes_checksum(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            release_dir = root / "TarnishedsArsenal_0.12.0"
            release_dir.mkdir()
            portable_name = "TarnishedsArsenal_0.12.0_portable.exe"
            portable = release_dir / portable_name
            portable.write_bytes(b"portable")
            msi_name = "TarnishedsArsenal_0.12.0_x64_en-US.msi"
            msi = release_dir / msi_name
            msi.write_bytes(b"installer")
            (release_dir / "README.md").write_text("portable release\n", encoding="utf-8")
            (release_dir / "build-report.json").write_text("{}\n", encoding="utf-8")
            (release_dir / "data-validation.json").write_text("{}\n", encoding="utf-8")
            (release_dir / "LICENSE").write_text("license\n", encoding="utf-8")
            full_checksums = (
                f"{hashlib.sha256(portable.read_bytes()).hexdigest()}  {portable_name}\n"
                f"{hashlib.sha256(msi.read_bytes()).hexdigest()}  {msi_name}\n"
            )
            (release_dir / "SHA256SUMS.txt").write_text(full_checksums, encoding="utf-8")
            archive_path = root / "release.zip"

            package_release.create_portable_archive(archive_path, release_dir, portable_name)

            with zipfile.ZipFile(archive_path) as archive:
                names = archive.namelist()
                expected_names = {
                    f"{release_dir.name}/{name}"
                    for name in (
                        portable_name,
                        "README.md",
                        "build-report.json",
                        "data-validation.json",
                        "LICENSE",
                        "SHA256SUMS.txt",
                    )
                }
                self.assertEqual(set(names), expected_names)
                self.assertFalse(any(name.lower().endswith(".msi") for name in names))
                checksum_name = f"{release_dir.name}/SHA256SUMS.txt"
                checksum_text = archive.read(checksum_name).decode("utf-8")
                self.assertEqual(
                    checksum_text,
                    f"{hashlib.sha256(portable.read_bytes()).hexdigest()}  {portable_name}\n",
                )
                self.assertNotIn(".msi", checksum_text.lower())
            self.assertEqual((release_dir / "SHA256SUMS.txt").read_text(encoding="utf-8"), full_checksums)

    @patch.object(package_release, "msi_property")
    def test_msi_identity_includes_product_version(self, msi_property) -> None:
        properties = {
            "ProductName": package_release.EXPECTED_PRODUCT_NAME,
            "UpgradeCode": package_release.EXPECTED_UPGRADE_CODE,
            "ProductVersion": "0.12.0",
        }
        msi_property.side_effect = lambda _path, name: properties[name]

        package_release.verify_msi_identity(
            Path("release.msi"),
            package_release.EXPECTED_PRODUCT_NAME,
            package_release.EXPECTED_UPGRADE_CODE,
            "0.12.0",
        )

    @patch.object(package_release, "msi_property")
    def test_msi_identity_rejects_product_version_mismatch(self, msi_property) -> None:
        properties = {
            "ProductName": package_release.EXPECTED_PRODUCT_NAME,
            "UpgradeCode": package_release.EXPECTED_UPGRADE_CODE,
            "ProductVersion": "0.11.1",
        }
        msi_property.side_effect = lambda _path, name: properties[name]

        with self.assertRaisesRegex(RuntimeError, "ProductVersion"):
            package_release.verify_msi_identity(
                Path("release.msi"),
                package_release.EXPECTED_PRODUCT_NAME,
                package_release.EXPECTED_UPGRADE_CODE,
                "0.12.0",
            )

    @unittest.skipUnless(os.name == "nt", "MSI validation is Windows-only")
    def test_msi_payload_matches_portable_executable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            msi = root / "release.msi"
            portable = root / "tarnisheds-arsenal-desktop.exe"
            signtool = root / "msiexec.exe"
            msi.write_bytes(b"msi")
            portable.write_bytes(b"portable")
            signtool.write_bytes(b"stub")
            portable_bytes = b"portable-prefix" + package_release.BUNDLE_TYPE_UNKNOWN + b"-suffix"
            embedded_bytes = portable_bytes.replace(
                package_release.BUNDLE_TYPE_UNKNOWN,
                package_release.BUNDLE_TYPE_MSI,
            )
            portable.write_bytes(portable_bytes)

            def extract(command, **kwargs):
                destination = Path(
                    next(value for value in command if value.startswith("TARGETDIR=")).split("=", 1)[1]
                )
                destination.mkdir(parents=True, exist_ok=True)
                (destination / portable.name).write_bytes(embedded_bytes)
                return subprocess.CompletedProcess(command, 0)

            with patch.object(package_release.shutil, "which", return_value=str(signtool)):
                with patch.object(package_release.subprocess, "run", side_effect=extract):
                    package_release.verify_msi_payload(msi, portable, root)

    @unittest.skipUnless(os.name == "nt", "MSI validation is Windows-only")
    def test_msi_payload_matches_captured_signed_executable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            msi = root / "release.msi"
            portable = root / "tarnisheds-arsenal-desktop.exe"
            msiexec = root / "msiexec.exe"
            msi.write_bytes(b"msi")
            portable_bytes = b"portable-prefix" + package_release.BUNDLE_TYPE_UNKNOWN + b"-suffix"
            embedded_bytes = portable_bytes.replace(
                package_release.BUNDLE_TYPE_UNKNOWN,
                package_release.BUNDLE_TYPE_MSI,
            )
            portable.write_bytes(portable_bytes)
            msiexec.write_bytes(b"stub")

            def extract(command, **kwargs):
                destination = Path(
                    next(value for value in command if value.startswith("TARGETDIR=")).split("=", 1)[1]
                )
                destination.mkdir(parents=True, exist_ok=True)
                (destination / portable.name).write_bytes(embedded_bytes)
                return subprocess.CompletedProcess(command, 0)

            with patch.object(package_release.shutil, "which", return_value=str(msiexec)):
                with patch.object(package_release.subprocess, "run", side_effect=extract):
                    package_release.verify_msi_payload(
                        msi,
                        portable,
                        root,
                        expected_payload=embedded_bytes,
                    )

    @unittest.skipUnless(os.name == "nt", "MSI validation is Windows-only")
    def test_msi_payload_rejects_captured_signed_executable_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            msi = root / "release.msi"
            portable = root / "tarnisheds-arsenal-desktop.exe"
            msiexec = root / "msiexec.exe"
            msi.write_bytes(b"msi")
            portable_bytes = b"portable-prefix" + package_release.BUNDLE_TYPE_UNKNOWN + b"-suffix"
            embedded_bytes = portable_bytes.replace(
                package_release.BUNDLE_TYPE_UNKNOWN,
                package_release.BUNDLE_TYPE_MSI,
            )
            portable.write_bytes(portable_bytes)
            msiexec.write_bytes(b"stub")
            expected_payload = bytearray(embedded_bytes)
            expected_payload[0] ^= 1

            def extract(command, **kwargs):
                destination = Path(
                    next(value for value in command if value.startswith("TARGETDIR=")).split("=", 1)[1]
                )
                destination.mkdir(parents=True, exist_ok=True)
                (destination / portable.name).write_bytes(embedded_bytes)
                return subprocess.CompletedProcess(command, 0)

            with patch.object(package_release.shutil, "which", return_value=str(msiexec)):
                with patch.object(package_release.subprocess, "run", side_effect=extract):
                    with self.assertRaisesRegex(RuntimeError, "differs from"):
                        package_release.verify_msi_payload(
                            msi,
                            portable,
                            root,
                            expected_payload=bytes(expected_payload),
                        )

    def test_tauri_signing_config_keeps_credentials_in_environment(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = package_release.write_tauri_signing_config(
                root,
                "tarnisheds-arsenal-desktop.exe",
            )
            config_text = config.read_text(encoding="utf-8")
            script_text = (root / "sign-msi-payload.ps1").read_text(encoding="utf-8")
            parsed = __import__("json").loads(config_text)

            self.assertEqual(parsed["bundle"]["windows"]["signCommand"]["cmd"], "pwsh.exe")
            self.assertIn("%1", parsed["bundle"]["windows"]["signCommand"]["args"])
            self.assertNotIn("WINDOWS_SIGNING_CERTIFICATE", config_text)
            self.assertIn("TAURI_RELEASE_CERTIFICATE_PASSWORD", script_text)
            self.assertIn("TAURI_RELEASE_SIGNED_MSI_PAYLOAD", script_text)
            self.assertIn("TAURI_RELEASE_EXPECTED_MSI_PAYLOAD_SHA256", script_text)

    @unittest.skipUnless(
        shutil.which("pwsh.exe") or shutil.which("pwsh"),
        "PowerShell 7 is required for the signing-hook harness",
    )
    def test_tauri_signing_hook_runs_sign_verify_capture_order(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "tarnisheds-arsenal-desktop.exe"
            payload = b"prefix" + package_release.BUNDLE_TYPE_MSI + b"suffix"
            binary.write_bytes(payload)
            config = package_release.write_tauri_signing_config(root, binary.name)
            script = config.with_name("sign-msi-payload.ps1")
            fake_signtool = root / "fake-signtool.ps1"
            fake_signtool.write_text(
                "param([string]$Operation)\n"
                "Add-Content -LiteralPath $env:TEST_SIGN_LOG -Value $Operation\n"
                "exit 0\n",
                encoding="utf-8",
            )
            sign_log = root / "sign.log"
            snapshot = root / "signed-msi-payload.exe"
            certificate = root / "certificate.pfx"
            certificate.write_bytes(b"certificate")
            env = os.environ.copy()
            env.update(
                {
                    "TAURI_RELEASE_SIGNTOOL": str(fake_signtool),
                    "TAURI_RELEASE_CERTIFICATE": str(certificate),
                    "TAURI_RELEASE_CERTIFICATE_PASSWORD": "unit-test-password",
                    "TAURI_RELEASE_TIMESTAMP_URL": "https://timestamp.invalid",
                    "TAURI_RELEASE_SIGNED_MSI_PAYLOAD": str(snapshot),
                    "TAURI_RELEASE_EXPECTED_MSI_PAYLOAD_SHA256": hashlib.sha256(payload).hexdigest(),
                    "TEST_SIGN_LOG": str(sign_log),
                }
            )
            pwsh = shutil.which("pwsh.exe") or shutil.which("pwsh")
            assert pwsh is not None
            command = [
                    pwsh,
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(script),
                ]
            # Tauri's global callback signs extensions, the patched EXE and the MSI.
            for name in ["WixUtilExtension.dll", binary.name, "release.msi"]:
                target = root / name
                if target != binary:
                    target.write_bytes(b"bundler target")
                result = subprocess.run(
                    [*command, str(target)], check=False, capture_output=True, text=True, env=env,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(snapshot.read_bytes(), payload)
            self.assertEqual(sign_log.read_text(encoding="utf-8").splitlines(), ["sign", "verify"] * 3)
            for name, content, message in [
                (binary.name, payload, "more than once"),
                (binary.name, payload + b"tampered", "changed bytes"),
                (binary.name, b"missing marker", "exactly one MSI"),
                ("unexpected.exe", payload, "unexpected Tauri signing target"),
            ]:
                target = root / name
                target.write_bytes(content)
                result = subprocess.run(
                    [*command, str(target)], check=False, capture_output=True, text=True, env=env,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertEqual(snapshot.read_bytes(), payload)
            self.assertEqual(sign_log.read_text(encoding="utf-8").splitlines(), ["sign", "verify"] * 3)

    def test_expected_msi_payload_sha256_replaces_only_bundle_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "portable.exe"
            portable_bytes = b"prefix" + package_release.BUNDLE_TYPE_UNKNOWN + b"suffix"
            expected = portable_bytes.replace(
                package_release.BUNDLE_TYPE_UNKNOWN,
                package_release.BUNDLE_TYPE_MSI,
            )
            binary.write_bytes(portable_bytes)

            self.assertEqual(
                package_release.expected_msi_payload_sha256(binary),
                hashlib.sha256(expected).hexdigest(),
            )

    def test_signing_orders_msi_payload_before_portable_and_container(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app_dir = root / "apps" / "desktop"
            tauri_dir = app_dir / "src-tauri"
            exe = tauri_dir / "target" / "release" / "tarnisheds-arsenal-desktop.exe"
            exe.parent.mkdir(parents=True)
            portable_bytes = b"portable-prefix" + package_release.BUNDLE_TYPE_UNKNOWN + b"-suffix"
            exe.write_bytes(portable_bytes)
            signtool = root / "signtool.exe"
            sign_calls: list[str] = []

            def fake_sign(_signtool, _certificate, _password, _timestamp_url, binary):
                sign_calls.append(binary.name)

            def fake_verify(_signtool, binary):
                self.assertTrue(binary.is_file())

            def fake_bundle(_command, *, cwd, env):
                self.assertEqual(sign_calls, [])
                self.assertEqual(cwd, app_dir)
                self.assertIsNotNone(env)
                assert env is not None
                self.assertEqual(
                    env["TAURI_RELEASE_EXPECTED_MSI_PAYLOAD_SHA256"],
                    package_release.expected_msi_payload_sha256(exe),
                )
                snapshot = Path(env["TAURI_RELEASE_SIGNED_MSI_PAYLOAD"])
                snapshot.write_bytes(
                    portable_bytes.replace(
                        package_release.BUNDLE_TYPE_UNKNOWN,
                        package_release.BUNDLE_TYPE_MSI,
                    )
                )
                msi = tauri_dir / "target" / "release" / "bundle" / "msi" / "release.msi"
                msi.parent.mkdir(parents=True)
                msi.write_bytes(b"msi")

            with patch.dict(
                os.environ,
                {
                    "WINDOWS_SIGNING_CERTIFICATE_BASE64": "Y2VydA==",
                    "WINDOWS_SIGNING_CERTIFICATE_PASSWORD": "unit-test-password",
                },
                clear=False,
            ):
                with patch.object(package_release, "find_signtool", return_value=signtool):
                    with patch.object(package_release, "sign_windows_binary", side_effect=fake_sign):
                        with patch.object(package_release, "verify_windows_binary", side_effect=fake_verify):
                            with patch.object(package_release, "run", side_effect=fake_bundle):
                                result = package_release.sign_release_binaries_if_configured(
                                    app_dir,
                                    tauri_dir,
                                )

            self.assertEqual(sign_calls, [exe.name])
            self.assertTrue(result[2])
            self.assertEqual(
                result[3],
                portable_bytes.replace(
                    package_release.BUNDLE_TYPE_UNKNOWN,
                    package_release.BUNDLE_TYPE_MSI,
                ),
            )

    @unittest.skipUnless(os.name == "nt", "MSI validation is Windows-only")
    def test_msi_payload_rejects_different_executable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            msi = root / "release.msi"
            portable = root / "tarnisheds-arsenal-desktop.exe"
            signtool = root / "msiexec.exe"
            msi.write_bytes(b"msi")
            portable.write_bytes(b"portable")
            signtool.write_bytes(b"stub")
            portable_bytes = b"portable-prefix" + package_release.BUNDLE_TYPE_UNKNOWN + b"-suffix"
            portable.write_bytes(portable_bytes)

            def extract(command, **kwargs):
                destination = Path(
                    next(value for value in command if value.startswith("TARGETDIR=")).split("=", 1)[1]
                )
                destination.mkdir(parents=True, exist_ok=True)
                different = bytearray(
                    portable_bytes.replace(
                        package_release.BUNDLE_TYPE_UNKNOWN,
                        package_release.BUNDLE_TYPE_MSI,
                    )
                )
                different[0] ^= 1
                (destination / portable.name).write_bytes(different)
                return subprocess.CompletedProcess(command, 0)

            with patch.object(package_release.shutil, "which", return_value=str(signtool)):
                with patch.object(package_release.subprocess, "run", side_effect=extract):
                    with self.assertRaisesRegex(RuntimeError, "differs from"):
                        package_release.verify_msi_payload(msi, portable, root)

    @unittest.skipUnless(os.name == "nt", "MSI validation is Windows-only")
    def test_msi_payload_rejects_unknown_bundle_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            msi = root / "release.msi"
            portable = root / "tarnisheds-arsenal-desktop.exe"
            msiexec = root / "msiexec.exe"
            msi.write_bytes(b"msi")
            portable_bytes = b"portable-prefix" + package_release.BUNDLE_TYPE_UNKNOWN + b"-suffix"
            portable.write_bytes(portable_bytes)
            msiexec.write_bytes(b"stub")

            def extract(command, **kwargs):
                destination = Path(
                    next(value for value in command if value.startswith("TARGETDIR=")).split("=", 1)[1]
                )
                destination.mkdir(parents=True, exist_ok=True)
                (destination / portable.name).write_bytes(portable_bytes)
                return subprocess.CompletedProcess(command, 0)

            with patch.object(package_release.shutil, "which", return_value=str(msiexec)):
                with patch.object(package_release.subprocess, "run", side_effect=extract):
                    with self.assertRaisesRegex(RuntimeError, "MSI bundle marker"):
                        package_release.verify_msi_payload(msi, portable, root)

    @unittest.skipUnless(os.name == "nt", "MSI validation is Windows-only")
    def test_msi_payload_rejects_duplicate_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            msi = root / "release.msi"
            portable = root / "tarnisheds-arsenal-desktop.exe"
            msiexec = root / "msiexec.exe"
            msi.write_bytes(b"msi")
            portable_bytes = (
                b"portable-prefix"
                + package_release.BUNDLE_TYPE_UNKNOWN
                + b"-middle"
                + package_release.BUNDLE_TYPE_UNKNOWN
                + b"-suffix"
            )
            portable.write_bytes(portable_bytes)
            msiexec.write_bytes(b"stub")

            def extract(command, **kwargs):
                destination = Path(
                    next(value for value in command if value.startswith("TARGETDIR=")).split("=", 1)[1]
                )
                destination.mkdir(parents=True, exist_ok=True)
                (destination / portable.name).write_bytes(
                    portable_bytes.replace(
                        package_release.BUNDLE_TYPE_UNKNOWN,
                        package_release.BUNDLE_TYPE_MSI,
                    )
                )
                return subprocess.CompletedProcess(command, 0)

            with patch.object(package_release.shutil, "which", return_value=str(msiexec)):
                with patch.object(package_release.subprocess, "run", side_effect=extract):
                    with self.assertRaisesRegex(RuntimeError, "unknown bundle marker"):
                        package_release.verify_msi_payload(msi, portable, root)


if __name__ == "__main__":
    unittest.main()
