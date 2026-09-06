"""Resume publication from one verified package; never replace remote assets."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any


def sha256(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def gh(*args: str, missing_ok: bool = False) -> str | None:
    result = subprocess.run(["gh", *args], capture_output=True, text=True, check=False)
    if result.returncode:
        if missing_ok and result.stderr.strip() == "release not found":
            return None
        raise RuntimeError(result.stderr.strip() or "GitHub command failed")
    return result.stdout


def release_info(repo: str, tag: str) -> dict[str, Any] | None:
    result = gh("release", "view", tag, "--repo", repo, "--json", "isDraft,assets", missing_ok=True)
    return json.loads(result) if result is not None else None


def verify_assets(repo: str, tag: str, release: dict[str, Any], files: list[Path]) -> list[Path]:
    remote = {asset["name"]: asset for asset in release["assets"]}
    if len(remote) != len(release["assets"]) or set(remote) - {path.name for path in files}:
        raise RuntimeError("Release contains unexpected or duplicate assets")
    missing = []
    for path in files:
        asset = remote.get(path.name)
        if asset is None:
            missing.append(path)
            continue
        digest = asset.get("digest")
        if digest is None:
            with tempfile.TemporaryDirectory(dir=path.parent) as directory:
                gh("release", "download", tag, "--repo", repo, "--pattern", path.name, "--dir", directory)
                digest = "sha256:" + sha256(Path(directory) / path.name)
        if asset.get("state") != "uploaded" or asset["size"] != path.stat().st_size or digest != "sha256:" + sha256(path):
            raise RuntimeError(f"Remote asset differs from this package: {path.name}. Resume the original workflow's failed publish job.")
    return missing


def publish(repo: str, tag: str, commit: str, assets: Path) -> None:
    if not re.fullmatch(r"v\d+\.\d+\.\d+", tag) or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected a version tag and full source commit")
    prefix = f"TarnishedsArsenal_{tag[1:]}"
    files = [assets / f"{prefix}{suffix}" for suffix in (
        "_portable.exe", "_x64_en-US.msi", ".zip", "_SHA256SUMS.txt", "_build-report.json",
    )]
    notes = assets / f"release-notes-{tag}.md"
    for path in [*files, notes]:
        if not path.is_file() or not path.stat().st_size:
            raise RuntimeError(f"Missing or empty release input: {path.name}")
    report = json.loads(files[-1].read_text(encoding="utf-8-sig"))
    if report["commit"] != commit or report["version"] != tag[1:] or report["sourceDirty"] is not False:
        raise RuntimeError("Package provenance does not match this source")
    for path, row in zip(files[:2], report["artifacts"], strict=True):
        if row != {"name": path.name, "bytes": path.stat().st_size, "sha256": sha256(path)}:
            raise RuntimeError(f"Package provenance hash mismatch: {path.name}")
    checksums = "".join(f"{sha256(path)}  {path.name}\n" for path in files[:2])
    if files[-2].read_text(encoding="utf-8-sig") != checksums:
        raise RuntimeError("Package checksum file does not match its binaries")

    release = release_info(repo, tag)
    if release is None:
        gh("release", "create", tag, "--repo", repo, "--verify-tag", "--draft",
           "--title", f"Tarnished's Arsenal {tag}", "--notes-file", str(notes))
        release = release_info(repo, tag)
    if release is None:
        raise RuntimeError("Release was not created")
    missing = verify_assets(repo, tag, release, files)
    if missing:
        gh("release", "upload", tag, "--repo", repo, *(str(path) for path in missing))
    release = release_info(repo, tag)
    if release is None or verify_assets(repo, tag, release, files):
        raise RuntimeError("Release asset upload is incomplete")
    if release["isDraft"]:
        gh("release", "edit", tag, "--repo", repo, "--draft=false", "--latest", "--notes-file", str(notes))
    release = release_info(repo, tag)
    if release is None or release["isDraft"] or verify_assets(repo, tag, release, files):
        raise RuntimeError("Published release verification failed")
    print(f"Verified published release {tag} at {commit}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--assets", type=Path, required=True)
    args = parser.parse_args()
    publish(args.repo, args.tag, args.commit, args.assets)


if __name__ == "__main__":
    main()
