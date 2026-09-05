from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase4 import package_release


class PackageReleaseTests(unittest.TestCase):
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
            result = subprocess.run(
                [
                    pwsh,
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(script),
                    str(binary),
                ],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(snapshot.read_bytes(), payload)
            self.assertEqual(sign_log.read_text(encoding="utf-8").splitlines(), ["sign", "verify"])

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

            self.assertEqual(sign_calls, [exe.name, "release.msi"])
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
