from __future__ import annotations

import argparse
import base64
import ctypes
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

EXPECTED_PRODUCT_NAME = "Tarnished’s Arsenal"
EXPECTED_UPGRADE_CODE = "{EC17FDAC-E313-5440-BD56-B985F2F0DA58}"


def npm_cmd() -> str:
    return "npm.cmd" if os.name == "nt" else "npm"


def node_cmd() -> str:
    return "node.exe" if os.name == "nt" else "node"


def python_cmd() -> str:
    return "python"


def run(cmd: list[str], cwd: Path, *, env: dict[str, str] | None = None) -> None:
    subprocess.run(cmd, cwd=cwd, check=True, env=env)


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def newest(path_glob: str, root: Path) -> Path:
    matches = sorted(root.glob(path_glob), key=lambda path: path.stat().st_mtime)
    if not matches:
        raise FileNotFoundError(f"no artifact matched {path_glob} under {root}")
    return matches[-1]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_commit(root: Path) -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()


def git_status(root: Path, *, include_untracked: bool) -> list[str]:
    result = subprocess.run(
        [
            "git",
            "status",
            "--porcelain=v1",
            f"--untracked-files={'all' if include_untracked else 'no'}",
        ],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return [line for line in result.stdout.splitlines() if line]


def require_clean_source(root: Path) -> str:
    commit = git_commit(root)
    changes = git_status(root, include_untracked=True)
    if changes:
        raise RuntimeError(
            f"refusing to package a dirty source tree ({len(changes)} changed or untracked paths)"
        )
    return commit


def git_changed_paths(root: Path) -> list[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", "--ignore-cr-at-eol", "HEAD", "--"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return [line for line in result.stdout.splitlines() if line]


def safe_path_summary(paths: list[str]) -> str:
    safe_entries: list[str] = []
    secret_markers = (
        ".env",
        "secret",
        "token",
        "credential",
        "private",
        "password",
        ".pem",
        ".key",
    )
    for path in paths[:5]:
        safe_path = (
            "<redacted>" if any(marker in path.lower() for marker in secret_markers) else path
        )
        safe_entries.append(safe_path)
    if len(paths) > len(safe_entries):
        safe_entries.append(f"... and {len(paths) - len(safe_entries)} more")
    return ", ".join(safe_entries)


def public_manifest_diff(root: Path, changed_paths: list[str]) -> str:
    manifest = "apps/desktop/src-tauri/Cargo.toml"
    if changed_paths != [manifest]:
        return ""
    result = subprocess.run(
        [
            "git",
            "diff",
            "--ignore-cr-at-eol",
            "--unified=1",
            "HEAD",
            "--",
            manifest,
        ],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def require_unchanged_tracked_source(root: Path, expected_commit: str, *, stage: str) -> None:
    if git_commit(root) != expected_commit:
        raise RuntimeError(f"source commit changed during {stage}")
    changed_paths = git_changed_paths(root)
    if changed_paths:
        message = f"{stage} modified tracked source: {safe_path_summary(changed_paths)}"
        manifest_diff = public_manifest_diff(root, changed_paths)
        if manifest_diff:
            message += f"\n{manifest_diff}"
        raise RuntimeError(message)


def require_replaceable(path: Path) -> None:
    if not path.exists():
        return
    try:
        with path.open("rb+"):
            pass
    except PermissionError as exc:
        raise PermissionError(
            f"release artifact is in use; close the running app or file viewer: {path}"
        ) from exc


def find_signtool() -> Path:
    direct = shutil.which("signtool")
    if direct:
        return Path(direct)
    kits = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Windows Kits/10/bin"
    candidates = sorted(kits.glob("*/x64/signtool.exe"), reverse=True)
    if not candidates:
        raise FileNotFoundError("Windows signing credentials are configured but signtool.exe was not found")
    return candidates[0]


def msi_property(path: Path, property_name: str) -> str:
    if os.name != "nt":
        raise RuntimeError("MSI metadata validation requires Windows")

    handle_type = ctypes.c_uint
    msi = ctypes.WinDLL("msi.dll", use_last_error=True)
    msi.MsiOpenDatabaseW.argtypes = [
        ctypes.c_wchar_p,
        ctypes.c_wchar_p,
        ctypes.POINTER(handle_type),
    ]
    msi.MsiOpenDatabaseW.restype = handle_type
    msi.MsiDatabaseOpenViewW.argtypes = [
        handle_type,
        ctypes.c_wchar_p,
        ctypes.POINTER(handle_type),
    ]
    msi.MsiDatabaseOpenViewW.restype = handle_type
    msi.MsiViewExecute.argtypes = [handle_type, handle_type]
    msi.MsiViewExecute.restype = handle_type
    msi.MsiViewFetch.argtypes = [handle_type, ctypes.POINTER(handle_type)]
    msi.MsiViewFetch.restype = handle_type
    msi.MsiRecordGetStringW.argtypes = [
        handle_type,
        ctypes.c_uint,
        ctypes.POINTER(ctypes.c_wchar),
        ctypes.POINTER(ctypes.c_uint),
    ]
    msi.MsiRecordGetStringW.restype = handle_type
    msi.MsiCloseHandle.argtypes = [handle_type]
    msi.MsiCloseHandle.restype = handle_type

    database = handle_type()
    result = msi.MsiOpenDatabaseW(str(path), None, ctypes.byref(database))
    if result:
        raise RuntimeError(f"cannot open MSI {path} (Windows Installer error {result})")

    view = handle_type()
    try:
        query = f"SELECT `Value` FROM `Property` WHERE `Property` = '{property_name}'"
        result = msi.MsiDatabaseOpenViewW(database, query, ctypes.byref(view))
        if result:
            raise RuntimeError(f"cannot query MSI {path} (Windows Installer error {result})")
        result = msi.MsiViewExecute(view, 0)
        if result:
            raise RuntimeError(f"cannot read MSI {path} (Windows Installer error {result})")

        record = handle_type()
        try:
            result = msi.MsiViewFetch(view, ctypes.byref(record))
            if result == 259:
                raise RuntimeError(f"MSI {path} has no {property_name} property")
            if result:
                raise RuntimeError(f"cannot fetch MSI {path} (Windows Installer error {result})")

            capacity = 256
            while True:
                buffer = ctypes.create_unicode_buffer(capacity)
                length = ctypes.c_uint(capacity - 1)
                result = msi.MsiRecordGetStringW(record, 1, buffer, ctypes.byref(length))
                if result == 234:
                    capacity = length.value + 1
                    continue
                if result:
                    raise RuntimeError(
                        f"cannot read MSI {property_name} (Windows Installer error {result})"
                    )
                return buffer.value
        finally:
            if record.value:
                msi.MsiCloseHandle(record)
    finally:
        if view.value:
            msi.MsiCloseHandle(view)
        msi.MsiCloseHandle(database)


def verify_msi_identity(
    path: Path,
    product_name: str,
    upgrade_code: str,
    product_version: str,
) -> None:
    actual_product_name = msi_property(path, "ProductName")
    actual_upgrade_code = msi_property(path, "UpgradeCode")
    actual_product_version = msi_property(path, "ProductVersion")
    if actual_product_name != product_name:
        raise RuntimeError(
            f"MSI ProductName is {actual_product_name!r}; expected {product_name!r}"
        )
    if actual_upgrade_code != upgrade_code:
        raise RuntimeError(
            f"MSI UpgradeCode is {actual_upgrade_code!r}; expected {upgrade_code!r}"
        )
    if actual_product_version != product_version:
        raise RuntimeError(
            f"MSI ProductVersion is {actual_product_version!r}; expected {product_version!r}"
        )


def verify_msi_payload(path: Path, portable_exe: Path, workspace: Path) -> None:
    """Extract the MSI administrative payload and compare its executable bytes."""
    if os.name != "nt":
        raise RuntimeError("MSI payload validation requires Windows")
    msiexec = shutil.which("msiexec.exe") or str(
        Path(os.environ.get("WINDIR", "C:/Windows")) / "System32" / "msiexec.exe"
    )
    if not Path(msiexec).is_file():
        raise FileNotFoundError("msiexec.exe was not found for MSI payload validation")

    workspace.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="tarnisheds-msi-verify-", dir=workspace) as directory:
        destination = Path(directory)
        result = subprocess.run(
            [msiexec, "/a", str(path), "/qn", f"TARGETDIR={destination}"],
            check=False,
            capture_output=True,
            text=True,
            timeout=180,
        )
        if result.returncode != 0:
            detail = (result.stderr or result.stdout or "msiexec returned an error").strip()
            raise RuntimeError(
                f"MSI administrative extraction failed for {path.name} "
                f"(exit {result.returncode}): {detail}"
            )

        embedded = list(destination.rglob(portable_exe.name))
        if len(embedded) != 1:
            raise RuntimeError(
                f"MSI administrative extraction found {len(embedded)} copies of "
                f"{portable_exe.name}; expected exactly one"
            )
        if sha256(embedded[0]) != sha256(portable_exe):
            raise RuntimeError(
                f"MSI payload executable differs from the tested portable executable: "
                f"{portable_exe.name}"
            )


def sign_windows_binary(
    signtool: Path,
    certificate: Path,
    password: str,
    timestamp_url: str,
    binary: Path,
) -> None:
    command = [
        str(signtool),
        "sign",
        "/fd",
        "SHA256",
        "/td",
        "SHA256",
        "/tr",
        timestamp_url,
        "/f",
        str(certificate),
        "/p",
        password,
        str(binary),
    ]
    try:
        subprocess.run(command, check=True, capture_output=True, text=True)
        subprocess.run(
            [str(signtool), "verify", "/pa", "/v", str(binary)],
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as error:
        detail = (error.stderr or error.stdout or "signtool returned an error").strip()
        raise RuntimeError(f"Authenticode signing failed for {binary.name}: {detail}") from None


def sign_release_binaries_if_configured(
    app_dir: Path,
    tauri_dir: Path,
) -> tuple[Path, Path, bool]:
    exe = newest("target/release/tarnisheds-arsenal-desktop.exe", tauri_dir)
    msi = newest("target/release/bundle/msi/*.msi", tauri_dir)
    encoded_certificate = os.environ.get("WINDOWS_SIGNING_CERTIFICATE_BASE64", "").strip()
    password = os.environ.get("WINDOWS_SIGNING_CERTIFICATE_PASSWORD", "")
    if not encoded_certificate and not password:
        return exe, msi, False
    if not encoded_certificate or not password:
        raise RuntimeError("Windows signing requires both certificate and password credentials")

    signtool = find_signtool()
    timestamp_url = (
        os.environ.get("WINDOWS_SIGNING_TIMESTAMP_URL", "").strip()
        or "http://timestamp.digicert.com"
    )
    with tempfile.TemporaryDirectory(prefix="tarnisheds-signing-") as directory:
        certificate = Path(directory) / "certificate.pfx"
        try:
            certificate.write_bytes(base64.b64decode(encoded_certificate, validate=True))
        except ValueError as error:
            raise RuntimeError("Windows signing certificate is not valid base64") from error
        sign_windows_binary(signtool, certificate, password, timestamp_url, exe)
        run(
            [npm_cmd(), "run", "tauri", "--", "bundle", "--bundles", "msi"],
            cwd=app_dir,
        )
        msi = newest("target/release/bundle/msi/*.msi", tauri_dir)
        sign_windows_binary(signtool, certificate, password, timestamp_url, msi)
    return exe, msi, True


def main() -> int:
    parser = argparse.ArgumentParser(description="Build the Windows release package.")
    parser.add_argument(
        "--skip-validation",
        action="store_true",
        help="Skip source tests and lint when exact-commit CI already validated this commit.",
    )
    parser.add_argument(
        "--replace-output",
        action="store_true",
        help="Deliberately refresh only the known files in an existing version output.",
    )
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[2]
    app_dir = root / "apps" / "desktop"
    tauri_dir = app_dir / "src-tauri"
    tauri_config = json.loads((tauri_dir / "tauri.conf.json").read_text(encoding="utf-8"))
    product_name = tauri_config["productName"]
    version = tauri_config["version"]
    completed_gates: list[str] = []
    validation_report = root / "build_release" / "data-validation.json"
    validation_completed = False
    release_dir = root / "dist" / f"TarnishedsArsenal_{version}"
    zip_path = root / "dist" / f"TarnishedsArsenal_{version}.zip"
    source_commit = require_clean_source(root)

    if release_dir.exists():
        if not args.replace_output:
            raise FileExistsError(
                "release output already exists; pass --replace-output to refresh its known files: "
                f"{release_dir}"
            )
        if not release_dir.is_dir():
            raise NotADirectoryError(f"release output is not a directory: {release_dir}")
        expected_names = {
            "LICENSE",
            "README.md",
            "SHA256SUMS.txt",
            "build-report.json",
            "data-validation.json",
            f"TarnishedsArsenal_{version}_portable.exe",
            f"TarnishedsArsenal_{version}_x64_en-US.msi",
        }
        unexpected_names = {path.name for path in release_dir.iterdir()} - expected_names
        if unexpected_names:
            raise RuntimeError(
                "refusing to refresh a release directory with unexpected entries: "
                + ", ".join(sorted(unexpected_names))
            )
        for name in expected_names:
            require_replaceable(release_dir / name)
        if args.skip_validation and (release_dir / "data-validation.json").exists():
            raise RuntimeError(
                "refusing to retain a stale data-validation.json while validation is skipped; "
                "run validation or use a fresh output directory"
            )
    if zip_path.exists():
        if not args.replace_output:
            raise FileExistsError(
                f"release archive already exists; pass --replace-output to refresh it: {zip_path}"
            )
        require_replaceable(zip_path)

    if not args.skip_validation:
        run(
            [
                python_cmd(),
                "tools/phase4/validate_release_metadata.py",
                "--tag",
                f"v{version}",
            ],
            cwd=root,
        )
        for test_dir in ("tools/phase1", "tools/phase4"):
            run(
                [
                    python_cmd(),
                    "-m",
                    "unittest",
                    "discover",
                    "-s",
                    test_dir,
                    "-p",
                    "test_*.py",
                ],
                cwd=root,
            )
        run([python_cmd(), "-m", "ruff", "check", "tools"], cwd=root)
        run([python_cmd(), "-m", "pyright", "tools"], cwd=root)
        run(
            [
                "cargo",
                "fmt",
                "--all",
                "--manifest-path",
                str(root / "core/er_optimizer_core/Cargo.toml"),
                "--",
                "--check",
            ],
            cwd=root,
        )
        run(
            [
                "cargo",
                "fmt",
                "--all",
                "--manifest-path",
                str(tauri_dir / "Cargo.toml"),
                "--",
                "--check",
            ],
            cwd=root,
        )
        for manifest in [root / "core/er_optimizer_core/Cargo.toml", tauri_dir / "Cargo.toml"]:
            run(
                [
                    "cargo",
                    "clippy",
                    "--locked",
                    "--manifest-path",
                    str(manifest),
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ],
                cwd=root,
            )
        run(
            [
                "cargo",
                "test",
                "--locked",
                "--manifest-path",
                str(root / "core/er_optimizer_core/Cargo.toml"),
            ],
            cwd=root,
        )
        run(
            ["cargo", "test", "--locked", "--manifest-path", str(tauri_dir / "Cargo.toml")],
            cwd=root,
        )
        run(
            [
                python_cmd(),
                "tools/phase4/validate_phase4.py",
                "--report",
                str(validation_report),
            ],
            cwd=root,
        )
        validation_completed = True
        completed_gates.extend(
            [
                "release-metadata",
                "python-unit-tests",
                "ruff",
                "pyright",
                "rustfmt",
                "clippy",
                "core-tests",
                "tauri-tests",
                "runtime-data-validation",
            ]
        )
    require_unchanged_tracked_source(root, source_commit, stage="release validation")
    run([npm_cmd(), "ci", "--prefer-offline", "--no-audit", "--fund=false"], cwd=app_dir)
    require_unchanged_tracked_source(root, source_commit, stage="npm ci")
    if not args.skip_validation:
        run(
            [
                node_cmd(),
                "./node_modules/@playwright/test/cli.js",
                "install",
                "chromium",
            ],
            cwd=app_dir,
        )
        require_unchanged_tracked_source(root, source_commit, stage="Playwright browser install")
        run([npm_cmd(), "test"], cwd=app_dir)
        require_unchanged_tracked_source(root, source_commit, stage="frontend tests")
        run([npm_cmd(), "run", "test:e2e"], cwd=app_dir)
        require_unchanged_tracked_source(root, source_commit, stage="frontend E2E tests")
    run([npm_cmd(), "run", "tauri", "--", "build", "--", "--locked"], cwd=app_dir)
    require_unchanged_tracked_source(root, source_commit, stage="Tauri build")
    packaged_exe, packaged_msi, code_signed = sign_release_binaries_if_configured(
        app_dir,
        tauri_dir,
    )
    if code_signed:
        completed_gates.append("windows-code-signing")
    verify_msi_identity(packaged_msi, EXPECTED_PRODUCT_NAME, EXPECTED_UPGRADE_CODE, version)
    verify_msi_payload(packaged_msi, packaged_exe, root / "dist")
    completed_gates.append("windows-msi-identity")
    run(
        [node_cmd(), "./scripts/smoke-packaged.mjs", str(packaged_exe)],
        cwd=app_dir,
    )
    require_unchanged_tracked_source(root, source_commit, stage="packaged app smoke")
    completed_gates.extend(["frontend-build", "tauri-release-build", "packaged-app-smoke"])
    if not args.skip_validation:
        completed_gates.extend(["playwright-browser", "frontend-tests", "frontend-e2e"])

    release_dir.mkdir(parents=True, exist_ok=args.replace_output)
    if validation_completed:
        shutil.copy2(validation_report, release_dir / "data-validation.json")

    exe = packaged_exe
    msi = packaged_msi
    exe_out = release_dir / f"TarnishedsArsenal_{version}_portable.exe"
    msi_out = release_dir / f"TarnishedsArsenal_{version}_x64_en-US.msi"
    shutil.copy2(exe, exe_out)
    shutil.copy2(msi, msi_out)
    if (root / "LICENSE").exists():
        shutil.copy2(root / "LICENSE", release_dir / "LICENSE")

    write_text(
        release_dir / "README.md",
        "\n".join(
            [
                f"# {product_name} {version}",
                "",
                "Windows desktop release built with Tauri.",
                "",
                "## Included",
                f"- `{msi_out.name}` installer",
                f"- `{exe_out.name}` portable executable",
                "- `SHA256SUMS.txt` integrity hashes",
                "- `build-report.json` build provenance",
                *( ["- `data-validation.json` generated-data validation report"] if validation_completed else [] ),
                "- `LICENSE`",
                "",
                "## Install",
                "Run the MSI installer, or launch the standalone executable directly.",
                "The portable executable requires Microsoft Edge WebView2 to be installed.",
                "The MSI downloads the WebView2 bootstrapper if it is needed.",
                "",
                "## Runtime Data",
                "Both artifacts contain the same compile-time Vanilla and Convergence runtime snapshots.",
                "No adjacent data directory, regulation.bin, or source workbook is required.",
            ]
        )
        + "\n",
    )

    artifacts = [exe_out, msi_out]
    artifact_rows = [
        {
            "name": artifact.name,
            "bytes": artifact.stat().st_size,
            "sha256": sha256(artifact),
        }
        for artifact in artifacts
    ]
    write_text(
        release_dir / "SHA256SUMS.txt",
        "".join(f"{row['sha256']}  {row['name']}\n" for row in artifact_rows),
    )
    profile_manifest_paths = [
        root / "data/phase1/manifest.json",
        root / "data/profiles/convergence/manifest.json",
    ]
    data_manifests = [
        json.loads(path.read_text(encoding="utf-8")) for path in profile_manifest_paths
    ]
    data_manifest_ids = {
        manifest["profile"]["id"]: manifest["id"] for manifest in data_manifests
    }
    if set(data_manifest_ids) != {"vanilla", "convergence"}:
        raise RuntimeError(f"release profile manifests are incomplete: {data_manifest_ids}")
    write_text(
        release_dir / "build-report.json",
        json.dumps(
            {
                "version": version,
                "commit": source_commit,
                "sourceDirty": False,
                "dataManifestIds": data_manifest_ids,
                "dataValidationReport": "data-validation.json" if validation_completed else None,
                "artifacts": artifact_rows,
                "validationSkipped": args.skip_validation,
                "codeSigned": code_signed,
                "completedGates": completed_gates,
            },
            indent=2,
        )
        + "\n",
    )
    shutil.make_archive(
        str(zip_path.with_suffix("")),
        "zip",
        root / "dist",
        release_dir.name,
    )

    print(f"Release packaged: {release_dir}")
    print(f"Installer: {msi_out}")
    print(f"Executable: {exe_out}")
    print(f"Checksums: {release_dir / 'SHA256SUMS.txt'}")
    print(f"Build report: {release_dir / 'build-report.json'}")
    print(f"Archive: {zip_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
