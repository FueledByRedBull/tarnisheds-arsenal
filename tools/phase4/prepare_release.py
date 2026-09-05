"""Update all local package versions together without touching dependencies."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def version_updates(root: Path, version: str) -> dict[Path, str]:
    if not re.fullmatch(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)", version):
        raise ValueError("version must be major.minor.patch without leading zeroes")
    if any(value > limit for value, limit in zip(map(int, version.split(".")), (255, 255, 65535))):
        raise ValueError("version exceeds Windows Installer limits (255.255.65535)")
    updates: dict[Path, str] = {}
    for relative in (
        "apps/desktop/package.json",
        "apps/desktop/package-lock.json",
        "apps/desktop/src-tauri/tauri.conf.json",
    ):
        path = root / relative
        data = json.loads(path.read_text(encoding="utf-8"))
        data["version"] = version
        if path.name == "package-lock.json":
            data["packages"][""]["version"] = version
        updates[path] = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
    for directory, packages in (
        ("apps/desktop/src-tauri", ("tarnisheds-arsenal-desktop", "er_optimizer_core")),
        ("core/er_optimizer_core", ("er_optimizer_core",)),
    ):
        for filename, names in (("Cargo.toml", packages[:1]), ("Cargo.lock", packages)):
            path = root / directory / filename
            content = path.read_text(encoding="utf-8")
            for name in names:
                pattern = rf'(\[\[?package\]\]?\nname = "{re.escape(name)}"\nversion = ")[^"]+("\n)'
                content, count = re.subn(pattern, lambda match: f"{match[1]}{version}{match[2]}", content)
                if count != 1:
                    raise ValueError(f"expected one {name} version in {path.relative_to(root)}")
            updates[path] = content
    return updates


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    args = parser.parse_args()
    # Prepare every edit first so malformed metadata cannot cause a partial bump.
    updates = version_updates(ROOT, args.version)
    for path, content in updates.items():
        path.write_text(content, encoding="utf-8", newline="\n")
    print(f"Updated {len(updates)} version files to {args.version}; prepare release notes, then validate metadata.")


if __name__ == "__main__":
    main()
