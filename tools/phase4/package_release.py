from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path


def npm_cmd() -> str:
    return "npm.cmd" if os.name == "nt" else "npm"


def python_cmd() -> str:
    return "python"


def run(cmd: list[str], cwd: Path) -> None:
    subprocess.run(cmd, cwd=cwd, check=True)


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
    return subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=root, text=True
    ).strip()


def git_is_dirty(root: Path) -> bool:
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return bool(result.stdout.strip())


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


def main() -> int:
    parser = argparse.ArgumentParser(description="Build the Windows release package.")
    parser.add_argument(
        "--skip-validation",
        action="store_true",
        help="Skip test/data validation when CI already validated this commit.",
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
    release_dir = root / "dist" / f"TarnishedsArsenal_{version}"
    zip_path = root / "dist" / f"TarnishedsArsenal_{version}.zip"

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
    if zip_path.exists():
        if not args.replace_output:
            raise FileExistsError(
                "release archive already exists; pass --replace-output to refresh it: "
                f"{zip_path}"
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
                "-m",
                "maturin",
                "build",
                "--release",
                "--locked",
                "--manifest-path",
                str(root / "core/er_optimizer_core/Cargo.toml"),
                "--features",
                "python",
            ],
            cwd=root,
        )
        wheel = newest("target/wheels/er_optimizer_core-*.whl", root / "core" / "er_optimizer_core")
        run([python_cmd(), "-m", "pip", "install", "--force-reinstall", str(wheel)], cwd=root)
        run([python_cmd(), "tools/phase4/validate_phase4.py"], cwd=root)
        completed_gates.extend(
            [
                "release-metadata",
                "ruff",
                "pyright",
                "rustfmt",
                "clippy",
                "core-tests",
                "tauri-tests",
                "runtime-data-validation",
            ]
        )
    run([npm_cmd(), "ci", "--prefer-offline", "--no-audit", "--fund=false"], cwd=app_dir)
    run([npm_cmd(), "run", "tauri", "--", "build"], cwd=app_dir)
    completed_gates.extend(["frontend-build", "tauri-release-build"])

    release_dir.mkdir(parents=True, exist_ok=args.replace_output)

    exe = newest("target/release/tarnisheds-arsenal-desktop.exe", tauri_dir)
    msi = newest("target/release/bundle/msi/*.msi", tauri_dir)
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
                f"- `{exe_out.name}` self-contained standalone executable",
                "- `SHA256SUMS.txt` integrity hashes",
                "- `build-report.json` build provenance",
                "- `LICENSE`",
                "",
                "## Install",
                "Run the MSI installer, or launch the standalone executable directly.",
                "",
                "## Runtime Data",
                "Both artifacts contain the same compile-time runtime snapshot.",
                "No adjacent data directory or source workbook is required.",
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
    data_manifest = json.loads((root / "data/phase1/manifest.json").read_text(encoding="utf-8"))
    write_text(
        release_dir / "build-report.json",
        json.dumps(
            {
                "version": version,
                "commit": git_commit(root),
                "sourceDirty": git_is_dirty(root),
                "dataManifestId": data_manifest["id"],
                "artifacts": artifact_rows,
                "validationSkipped": args.skip_validation,
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
