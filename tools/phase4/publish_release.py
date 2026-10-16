"""Resume publication from one verified package; never replace remote assets."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.phase1.snapshot_manifest import sha256_file as sha256  # noqa: E402


def verify_portable_archive(
    archive_path: Path, expected_entry: str, expected_sha256: str
) -> None:
    with zipfile.ZipFile(archive_path) as archive:
        executables = []
        for entry in archive.infolist():
            # Account for Windows separators, streams, and trailing-dot/space aliases.
            basename = entry.filename.replace("\\", "/").rstrip("/").rsplit("/", 1)[-1]
            basename = basename.split(":", 1)[0].rstrip(" .").casefold()
            if basename.endswith(".exe"):
                executables.append(entry)
        # Require the raw spelling too: ZipInfo normalizes separators and truncates NULs.
        if len(executables) != 1 or executables[0].orig_filename != expected_entry:
            raise RuntimeError(
                f"Portable archive must contain exactly one executable at {expected_entry}"
            )
        with archive.open(executables[0]) as payload:
            digest = hashlib.sha256(payload.read()).hexdigest()
        if digest != expected_sha256:
            raise RuntimeError("Portable archive executable does not match the verified binary")


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


def tag_commit(repo: str, tag: str) -> str:
    """Return the commit a tag points to, peeling an annotated tag."""
    target = json.loads(gh("api", f"repos/{repo}/git/ref/tags/{tag}") or "{}")["object"]
    if target["type"] == "tag":
        target = json.loads(gh("api", f"repos/{repo}/git/tags/{target['sha']}") or "{}")["object"]
    return target["sha"]


def version_key(tag: str) -> tuple[int, ...]:
    return tuple(int(part) for part in tag[1:].split("."))


def is_latest_version(repo: str, tag: str) -> bool:
    releases = json.loads(
        gh("release", "list", "--repo", repo, "--exclude-drafts", "--json", "tagName", "--limit", "1000")
        or "[]"
    )
    published = [
        release["tagName"] for release in releases
        if re.fullmatch(r"v\d+\.\d+\.\d+", release["tagName"])
    ]
    return all(version_key(tag) >= version_key(other) for other in published)


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
    verify_portable_archive(
        files[2], f"{prefix}/{files[0].name}", report["artifacts"][0]["sha256"]
    )
    if tag_commit(repo, tag) != commit:
        raise RuntimeError(f"Tag {tag} does not point to the packaged commit {commit}")

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
        latest = "--latest" if is_latest_version(repo, tag) else "--latest=false"
        gh("release", "edit", tag, "--repo", repo, "--draft=false", latest, "--notes-file", str(notes))
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
