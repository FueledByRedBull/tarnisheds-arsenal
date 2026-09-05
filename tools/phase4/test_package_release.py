from __future__ import annotations

import os
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

            def extract(command, **kwargs):
                destination = Path(
                    next(value for value in command if value.startswith("TARGETDIR=")).split("=", 1)[1]
                )
                destination.mkdir(parents=True, exist_ok=True)
                (destination / portable.name).write_bytes(portable.read_bytes())
                return subprocess.CompletedProcess(command, 0)

            with patch.object(package_release.shutil, "which", return_value=str(signtool)):
                with patch.object(package_release.subprocess, "run", side_effect=extract):
                    package_release.verify_msi_payload(msi, portable, root)

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

            def extract(command, **kwargs):
                destination = Path(
                    next(value for value in command if value.startswith("TARGETDIR=")).split("=", 1)[1]
                )
                destination.mkdir(parents=True, exist_ok=True)
                (destination / portable.name).write_bytes(b"different")
                return subprocess.CompletedProcess(command, 0)

            with patch.object(package_release.shutil, "which", return_value=str(signtool)):
                with patch.object(package_release.subprocess, "run", side_effect=extract):
                    with self.assertRaisesRegex(RuntimeError, "differs from"):
                        package_release.verify_msi_payload(msi, portable, root)


if __name__ == "__main__":
    unittest.main()
