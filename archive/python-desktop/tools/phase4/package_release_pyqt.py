from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path


def run(cmd: list[str], cwd: Path, env: dict[str, str] | None = None) -> None:
    full_env = os.environ.copy()
    if env:
        full_env.update(env)
    subprocess.run(cmd, cwd=cwd, check=True, env=full_env)


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

    run(["cargo", "test", "--manifest-path", str(crate_dir / "Cargo.toml")], cwd=root)
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
    run(["python", "-m", "pip", "install", "--force-reinstall", str(wheel)], cwd=root)
    run(["python", "tools/phase4/validate_phase4.py"], cwd=root)
    run(["python", "tools/phase4/smoke_ui.py"], cwd=root, env={"QT_QPA_PLATFORM": "offscreen"})

    version = wheel.name.split("-")[1]

    release_dir = root / "dist" / f"ERBuildOptimizer_{version}"
    if release_dir.exists():
        shutil.rmtree(release_dir)
    release_dir.mkdir(parents=True, exist_ok=True)

    shutil.copy2(wheel, release_dir / wheel.name)
    for desktop_file in sorted((root / "ui" / "desktop").glob("*.py")):
        shutil.copy2(desktop_file, release_dir / desktop_file.name)
    if (root / "LICENSE").exists():
        shutil.copy2(root / "LICENSE", release_dir / "LICENSE")

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
                "# Tarnished's Arsenal",
                "",
                "Portable Windows bundle for the session-driven Elden Ring optimizer UI.",
                "",
                "## Included",
                "- `app.py`, `models.py`, `services.py`, `theme.py`, `widgets.py`",
                "- `data/phase1/*.csv` runtime snapshot",
                "- `er_optimizer_core` wheel",
                "- `LICENSE`",
                "- `install.ps1`",
                "- `run.ps1`",
                "",
                "## Install",
                "```powershell",
                ".\\install.ps1",
                "```",
                "",
                "## Launch",
                "```powershell",
                ".\\run.ps1",
                "```",
                "",
                "## What You Get",
                "- Ranked build search",
                "- Compare workspace",
                "- Embedded Paths workspace",
                "- Embedded Affinity Watch workspace",
                "- AoW first-hit and full-sequence PvE objectives",
                "",
                "## Notes",
                "- Keep the folder layout exactly as shipped.",
                "- Requires Python 3.10+ on the target machine.",
            ]
        )
        + "\n",
    )

    print(f"Release packaged: {release_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
