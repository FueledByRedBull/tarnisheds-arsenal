from __future__ import annotations

import argparse
import json
import re
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


def expect_exact_version(label: str, version: object, errors: list[str]) -> bool:
    if not isinstance(version, str) or re.fullmatch(r"\d+\.\d+\.\d+", version) is None:
        errors.append(f"{label} is not an exact major.minor.patch version: {version!r}")
        return False
    return True


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
    python_version = (root / ".python-version").read_text(encoding="utf-8").strip()
    node_version = (root / ".node-version").read_text(encoding="utf-8").strip()
    ci_workflow = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    release_workflow = (root / ".github/workflows/release-package.yml").read_text(encoding="utf-8")
    package_script = (root / "tools/phase4/package_release.py").read_text(encoding="utf-8")

    if args.tag:
        expect_equal("release tag", args.tag, expected_tag, errors)

    if expect_exact_version("Rust toolchain", rust_version, errors):
        expected_rust_action = f"dtolnay/rust-toolchain@{rust_version}"
        expect_contains("CI Rust setup", ci_workflow, expected_rust_action, errors)
        expect_contains("release Rust setup", release_workflow, expected_rust_action, errors)
    expect_exact_version("Python toolchain", python_version, errors)
    expect_exact_version("Node toolchain", node_version, errors)

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
    expect_contains("CI Node setup", ci_workflow, 'node-version-file: ".node-version"', errors)
    expect_contains(
        "release Node setup",
        release_workflow,
        'node-version-file: ".node-version"',
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
    if "--upgrade pip" in ci_workflow or "--upgrade pip" in release_workflow:
        errors.append("workflows must not install an unpinned latest pip")
    expect_contains(
        "release clean-source preflight", package_script, "require_clean_source(root)", errors
    )
    expect_contains(
        "release tracked-source postflight",
        package_script,
        "require_unchanged_tracked_source(root, source_commit",
        errors,
    )
    expect_contains(
        "release locked Tauri build",
        package_script,
        '"tauri", "--", "build", "--", "--locked"',
        errors,
    )
    expect_contains("release-specific Cargo cache", release_workflow, "-release-cargo-", errors)
    expect_contains(
        "release provenance verification",
        release_workflow,
        "Verify release provenance and checksums",
        errors,
    )
    expect_contains(
        "release clean-source verification",
        release_workflow,
        "if ($report.sourceDirty -ne $false)",
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
