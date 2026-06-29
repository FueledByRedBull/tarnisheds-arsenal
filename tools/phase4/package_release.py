from __future__ import annotations

import argparse
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


def main() -> int:
    parser = argparse.ArgumentParser(description="Build the Windows release package.")
    parser.add_argument(
        "--skip-validation",
        action="store_true",
        help="Skip test/data validation when CI already validated this commit.",
    )
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[2]
    app_dir = root / "apps" / "desktop"
    tauri_dir = app_dir / "src-tauri"
    tauri_config = json.loads((tauri_dir / "tauri.conf.json").read_text(encoding="utf-8"))
    product_name = tauri_config["productName"]
    version = tauri_config["version"]

    if not args.skip_validation:
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
    run([npm_cmd(), "ci", "--prefer-offline", "--no-audit", "--fund=false"], cwd=app_dir)
    run([npm_cmd(), "run", "tauri", "--", "build"], cwd=app_dir)

    release_dir = root / "dist" / f"TarnishedsArsenal_{version}"
    zip_path = root / "dist" / f"TarnishedsArsenal_{version}.zip"
    if release_dir.exists():
        shutil.rmtree(release_dir)
    if zip_path.exists():
        zip_path.unlink()
    release_dir.mkdir(parents=True, exist_ok=True)

    exe = newest("target/release/tarnisheds-arsenal-desktop.exe", tauri_dir)
    msi = newest("target/release/bundle/msi/*.msi", tauri_dir)
    shutil.copy2(exe, release_dir / exe.name)
    shutil.copy2(msi, release_dir / msi.name)
    shutil.copytree(root / "data" / "phase1", release_dir / "data" / "phase1")
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
                f"- `{msi.name}` installer",
                f"- `{exe.name}` portable executable",
                "- `data/phase1/` portable runtime data",
                "- `LICENSE`",
                "",
                "## Install",
                "Run the MSI installer, or launch the executable directly from this folder for a portable run.",
                "",
                "## Runtime Data",
                "The installer bundles the committed `data/phase1` runtime snapshot as a Tauri resource.",
                "The portable executable loads the adjacent `data/phase1` folder in this release directory.",
            ]
        )
        + "\n",
    )

    print(f"Release packaged: {release_dir}")
    print(f"Installer: {release_dir / msi.name}")
    print(f"Executable: {release_dir / exe.name}")
    shutil.make_archive(str(zip_path.with_suffix("")), "zip", root / "dist", release_dir.name)
    print(f"Archive: {zip_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
