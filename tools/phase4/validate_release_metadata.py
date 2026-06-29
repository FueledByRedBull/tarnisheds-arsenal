from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def load_toml(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def cargo_lock_package_version(path: Path, package_name: str) -> str | None:
    data = load_toml(path)
    for package in data.get("package", []):
        if package.get("name") == package_name:
            return package.get("version")
    return None


def expect_equal(label: str, actual: str | None, expected: str, errors: list[str]) -> None:
    if actual != expected:
        errors.append(f"{label} is {actual!r}; expected {expected!r}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate release version metadata.")
    parser.add_argument("--tag", help="Git tag to validate, for example v0.5.0")
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[2]
    app_dir = root / "apps" / "desktop"
    tauri_dir = app_dir / "src-tauri"

    tauri_config = load_json(tauri_dir / "tauri.conf.json")
    version = tauri_config["version"]
    expected_tag = f"v{version}"
    errors: list[str] = []

    if args.tag:
        expect_equal("release tag", args.tag, expected_tag, errors)

    expect_equal(
        "apps/desktop/package.json version",
        load_json(app_dir / "package.json").get("version"),
        version,
        errors,
    )

    package_lock = load_json(app_dir / "package-lock.json")
    expect_equal(
        "apps/desktop/package-lock.json version",
        package_lock.get("version"),
        version,
        errors,
    )
    expect_equal(
        "apps/desktop/package-lock.json root package version",
        package_lock.get("packages", {}).get("", {}).get("version"),
        version,
        errors,
    )

    expect_equal(
        "apps/desktop/src-tauri/Cargo.toml version",
        load_toml(tauri_dir / "Cargo.toml").get("package", {}).get("version"),
        version,
        errors,
    )
    expect_equal(
        "core/er_optimizer_core/Cargo.toml version",
        load_toml(root / "core" / "er_optimizer_core" / "Cargo.toml")
        .get("package", {})
        .get("version"),
        version,
        errors,
    )
    expect_equal(
        "core/er_optimizer_core/Cargo.lock package version",
        cargo_lock_package_version(root / "core" / "er_optimizer_core" / "Cargo.lock", "er_optimizer_core"),
        version,
        errors,
    )
    expect_equal(
        "apps/desktop/src-tauri/Cargo.lock desktop package version",
        cargo_lock_package_version(tauri_dir / "Cargo.lock", "tarnisheds-arsenal-desktop"),
        version,
        errors,
    )
    expect_equal(
        "apps/desktop/src-tauri/Cargo.lock core package version",
        cargo_lock_package_version(tauri_dir / "Cargo.lock", "er_optimizer_core"),
        version,
        errors,
    )

    notes_path = root / "docs" / "release-notes" / f"{expected_tag}.md"
    if not notes_path.exists():
        errors.append(f"missing release notes: {notes_path.relative_to(root)}")

    notes_index = root / "docs" / "release-notes" / "README.md"
    if expected_tag not in notes_index.read_text(encoding="utf-8"):
        errors.append(f"release notes index does not mention {expected_tag}")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print(f"Release metadata validated for {expected_tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
