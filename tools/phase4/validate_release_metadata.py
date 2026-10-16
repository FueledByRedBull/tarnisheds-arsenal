from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.snapshot_manifest import SCHEMA_VERSION  # noqa: E402
from tools.phase4.package_release import EXPECTED_PRODUCT_NAME, EXPECTED_UPGRADE_CODE  # noqa: E402


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


def expect_exact_version(label: str, version: object, errors: list[str]) -> bool:
    if not isinstance(version, str) or re.fullmatch(r"\d+\.\d+\.\d+", version) is None:
        errors.append(f"{label} is not an exact major.minor.patch version: {version!r}")
        return False
    return True


def mentions(text: str, value: str) -> bool:
    """Match value as a whole token, so v0.17.2 does not match v0.17.20."""
    return re.search(rf"(?<![\w.]){re.escape(value)}(?![-\w]|\.\w)", text) is not None


def release_notes_errors(root: Path, expected_tag: str) -> list[str]:
    errors: list[str] = []
    notes_path = root / "docs" / "release-notes" / f"{expected_tag}.md"
    if not notes_path.is_file() or not notes_path.read_text(encoding="utf-8").strip():
        errors.append(f"missing or empty release notes: {notes_path.relative_to(root)}")

    notes_index = root / "docs" / "release-notes" / "README.md"
    try:
        index_text = notes_index.read_text(encoding="utf-8")
    except OSError:
        errors.append(f"missing release notes index: {notes_index.relative_to(root)}")
        return errors
    if not mentions(index_text, expected_tag):
        errors.append(f"release notes index does not mention {expected_tag}")
    release_url = (
        "https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/tag/"
        f"{expected_tag}"
    )
    if not mentions(index_text, release_url):
        errors.append(
            f"release notes index must link {expected_tag} directly; "
            "do not leave a Pending publication entry"
        )
    return errors


def toolchain_reference_errors(root: Path, rust: str, node: str, python: str, tag: str) -> list[str]:
    """Workflow pins and README prose must name the versions the toolchain files pin."""
    errors: list[str] = []
    for workflow in sorted((root / ".github" / "workflows").glob("*.yml")):
        text = workflow.read_text(encoding="utf-8")
        for pinned in re.findall(r"dtolnay/rust-toolchain@[0-9a-f]{40} # (\S+)", text):
            if pinned != rust:
                errors.append(f"{workflow.name} installs Rust {pinned}; rust-toolchain.toml pins {rust}")
        if re.search(r"\b(?:node|python)-version:", text):
            errors.append(f"{workflow.name} sets Node or Python by literal version; use the version file")
    readme = (root / "README.md").read_text(encoding="utf-8")
    for label, version in (("Rust", rust), ("Node.js", node), ("Python", python)):
        if not mentions(readme, f"{label} {version}"):
            errors.append(f"README.md does not name the pinned {label} {version}")
    for linked in sorted(set(re.findall(r"docs/release-notes/(v\d+\.\d+\.\d+)\.md", readme)) - {tag}):
        errors.append(f"README.md links {linked} release notes; the current release is {tag}")
    return errors


def requirements_errors(text: str) -> list[str]:
    """Each requirement must be an exact pin with sha256 hashes, so pip runs in hash-checking mode."""
    errors: list[str] = []
    pinned: set[str] = set()
    for line in text.replace("\\\n", " ").splitlines():
        requirement = line.split("#", 1)[0].strip()
        if not requirement:
            continue
        match = re.fullmatch(r"([A-Za-z0-9._-]+)==[^\s]+(?:\s+--hash=sha256:[0-9a-f]{64})+", requirement)
        if match is None:
            errors.append(f"requirements-validation.txt entry is not an exact pin with sha256 hashes: {requirement}")
        else:
            pinned.add(match[1].lower())
    for package in ("pyright", "ruff"):
        if package not in pinned:
            errors.append(f"requirements-validation.txt does not pin {package} exactly")
    return errors


def heading_anchors(text: str) -> set[str]:
    """GitHub heading anchors: lower case, punctuation dropped, spaces as hyphens, repeats numbered."""
    anchors: set[str] = set(re.findall(r'id="([^"]+)"', text))
    seen: dict[str, int] = {}
    for heading in re.findall(r"^#{1,6}\s+(.*?)\s*#*\s*$", re.sub(r"```.*?```", "", text, flags=re.S), re.M):
        slug = re.sub(r"[^\w\- ]", "", heading.lower()).replace(" ", "-")
        count = seen.get(slug, 0)
        seen[slug] = count + 1
        anchors.add(slug if count == 0 else f"{slug}-{count}")
    return anchors


def markdown_link_errors(root: Path, files: list[str]) -> list[str]:
    """Relative links in committed Markdown must reach a file and, when given, a heading."""
    errors: list[str] = []
    for name in files:
        path = root / name
        body = re.sub(r"`[^`\n]*`", "", re.sub(r"```.*?```", "", path.read_text(encoding="utf-8"), flags=re.S))
        for target in re.findall(r"\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)", body):
            if re.match(r"[a-z]+:", target):
                continue
            relative, _, anchor = target.partition("#")
            destination = (path.parent / relative).resolve() if relative else path
            if not destination.exists():
                errors.append(f"{name} links missing {target}")
            elif anchor and destination.suffix == ".md" and anchor not in heading_anchors(
                destination.read_text(encoding="utf-8")
            ):
                errors.append(f"{name} links missing heading {target}")
    return errors


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
    expect_equal(
        "Tauri productName",
        tauri_config.get("productName"),
        EXPECTED_PRODUCT_NAME,
        errors,
    )
    windows = tauri_config.get("app", {}).get("windows", [])
    if not windows:
        errors.append("Tauri app.windows is empty")
    for index, window in enumerate(windows):
        expect_equal(
            f"Tauri app.windows[{index}].title",
            window.get("title"),
            EXPECTED_PRODUCT_NAME,
            errors,
        )
    expect_equal(
        "Tauri WiX upgradeCode",
        tauri_config.get("bundle", {})
        .get("windows", {})
        .get("wix", {})
        .get("upgradeCode"),
        EXPECTED_UPGRADE_CODE,
        errors,
    )
    toolchain = load_toml(root / "rust-toolchain.toml").get("toolchain", {})
    rust_version = toolchain.get("channel")
    python_version = (root / ".python-version").read_text(encoding="utf-8").strip()
    node_version = (root / ".node-version").read_text(encoding="utf-8").strip()
    profile_manifests = {
        "vanilla": root / "data/phase1/manifest.json",
        "convergence": root / "data/profiles/convergence/manifest.json",
    }
    for profile_id, manifest_path in profile_manifests.items():
        if not manifest_path.is_file():
            errors.append(f"missing {profile_id} profile manifest: {manifest_path.relative_to(root)}")
            continue
        manifest = load_json(manifest_path)
        expect_equal(
            f"{profile_id} manifest profile id",
            manifest.get("profile", {}).get("id"),
            profile_id,
            errors,
        )
        if manifest.get("schemaVersion") != SCHEMA_VERSION:
            errors.append(
                f"{profile_id} manifest schemaVersion must be {SCHEMA_VERSION}"
            )
        if not str(manifest.get("id", "")).startswith(f"{profile_id}-"):
            errors.append(f"{profile_id} manifest id is not profile-bound")
        regulation = next(
            (source for source in manifest.get("sources", []) if source.get("kind") == "regulation"),
            None,
        )
        if not regulation or regulation.get("bundled") is not False:
            errors.append(f"{profile_id} regulation provenance must be recorded but not bundled")

    if args.tag:
        expect_equal("release tag", args.tag, expected_tag, errors)

    errors.extend(release_notes_errors(root, expected_tag))

    expect_exact_version("Rust toolchain", rust_version, errors)
    expect_exact_version("Python toolchain", python_version, errors)
    expect_exact_version("Node toolchain", node_version, errors)
    errors.extend(toolchain_reference_errors(root, rust_version, node_version, python_version, expected_tag))
    markdown = subprocess.check_output(["git", "ls-files", "*.md"], cwd=root, text=True).split()
    errors.extend(markdown_link_errors(root, markdown))

    errors.extend(requirements_errors((root / "requirements-validation.txt").read_text(encoding="utf-8")))

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

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print(f"Release metadata validated for {expected_tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
