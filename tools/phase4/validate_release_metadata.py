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


def expect_contains(label: str, text: str, expected: str, errors: list[str]) -> None:
    if expected not in text:
        errors.append(f"{label} does not contain {expected!r}")


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
    toolchain = load_toml(root / "rust-toolchain.toml").get("toolchain", {})
    rust_version = toolchain.get("channel")
    ci_workflow = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    release_workflow = (root / ".github/workflows/release-package.yml").read_text(encoding="utf-8")

    if args.tag:
        expect_equal("release tag", args.tag, expected_tag, errors)

    if not isinstance(rust_version, str) or not rust_version:
        errors.append("rust-toolchain.toml is missing a pinned toolchain.channel")
    else:
        expected_rust_action = f"dtolnay/rust-toolchain@{rust_version}"
        expect_contains("CI Rust setup", ci_workflow, expected_rust_action, errors)
        expect_contains("release Rust setup", release_workflow, expected_rust_action, errors)

    expect_contains(
        "CI Python setup", ci_workflow, 'python-version-file: ".python-version"', errors
    )
    expect_contains(
        "CI Playwright browser install",
        ci_workflow,
        "node ./node_modules/@playwright/test/cli.js install chromium",
        errors,
    )
    expect_contains(
        "release Python setup",
        release_workflow,
        'python-version-file: ".python-version"',
        errors,
    )
    expect_contains("release CI prerequisite", release_workflow, "needs: verify-ci", errors)
    expect_contains(
        "release exact-SHA CI check", release_workflow, "--commit $env:GITHUB_SHA", errors
    )
    expect_contains(
        "release default-branch CI check",
        release_workflow,
        "RELEASE_BRANCH: ${{ github.event.repository.default_branch }}",
        errors,
    )
    expect_contains(
        "release native-command failure handling",
        release_workflow,
        "$PSNativeCommandUseErrorActionPreference = $true",
        errors,
    )

    validation_requirements = (root / "requirements-validation.txt").read_text(encoding="utf-8")
    for package in ("maturin", "pyright", "ruff"):
        if not any(
            line.startswith(f"{package}==") for line in validation_requirements.splitlines()
        ):
            errors.append(f"requirements-validation.txt does not pin {package} exactly")

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
        cargo_lock_package_version(
            root / "core" / "er_optimizer_core" / "Cargo.lock", "er_optimizer_core"
        ),
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
