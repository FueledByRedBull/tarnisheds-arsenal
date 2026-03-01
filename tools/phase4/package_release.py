from __future__ import annotations

import shutil
import subprocess
from pathlib import Path


def run(cmd: list[str], cwd: Path) -> None:
    subprocess.run(cmd, cwd=cwd, check=True)


def newest_wheel(wheel_dir: Path) -> Path:
    wheels = sorted(wheel_dir.glob("er_optimizer_core-*.whl"), key=lambda p: p.stat().st_mtime)
    if not wheels:
        raise FileNotFoundError(f"no wheel found in {wheel_dir}")
    return wheels[-1]


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    crate_dir = root / "core" / "er_optimizer_core"
    wheel_dir = crate_dir / "target" / "wheels"

    run(
        [
            "python",
            "-m",
            "maturin",
            "build",
            "--manifest-path",
            str(crate_dir / "Cargo.toml"),
            "--features",
            "python",
        ],
        cwd=root,
    )

    wheel = newest_wheel(wheel_dir)
    version = wheel.name.split("-")[1]

    release_dir = root / "dist" / f"ERBuildOptimizer_{version}"
    if release_dir.exists():
        shutil.rmtree(release_dir)
    release_dir.mkdir(parents=True, exist_ok=True)

    shutil.copy2(wheel, release_dir / wheel.name)
    shutil.copy2(root / "ui" / "desktop" / "app.py", release_dir / "app.py")

    data_out = release_dir / "data" / "phase1"
    data_out.mkdir(parents=True, exist_ok=True)
    for csv_file in (root / "data" / "phase1").glob("*.csv"):
        shutil.copy2(csv_file, data_out / csv_file.name)

    write_text(
        release_dir / "requirements.txt",
        "\n".join(
            [
                "PyQt6>=6.10,<7",
                "# install local wheel below after this requirements file",
            ]
        )
        + "\n",
    )

    write_text(
        release_dir / "install.ps1",
        f"""$ErrorActionPreference = 'Stop'
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
python -m pip install --force-reinstall .\\{wheel.name}
Write-Host 'Install complete.'
""",
    )

    write_text(
        release_dir / "run.ps1",
        """$ErrorActionPreference = 'Stop'
python .\\app.py
""",
    )

    write_text(
        release_dir / "README.md",
        "\n".join(
            [
                "# ER Build Optimizer",
                "",
                "## Contents",
                "- `app.py`",
                "- `data/phase1/*.csv` snapshot",
                "- `er_optimizer_core` wheel",
                "- `install.ps1`",
                "- `run.ps1`",
                "",
                "## Setup (Windows PowerShell)",
                "```powershell",
                ".\\install.ps1",
                "```",
                "",
                "## Run",
                "```powershell",
                ".\\run.ps1",
                "```",
                "",
                "## Notes",
                "- Keep `data/phase1` next to `app.py` exactly as shipped.",
                "- Use Python 3.10+.",
            ]
        )
        + "\n",
    )

    print(f"Release packaged: {release_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
