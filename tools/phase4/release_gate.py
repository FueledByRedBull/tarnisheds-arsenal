"""Release workflow gates: default branch, verified CI, release tag and packaging mode.

Standard library only: the verify-ci job checks out just tools/phase4.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CI_WAIT_SECONDS = 40 * 60
CI_POLL_SECONDS = 15


def run(*command: str) -> str:
    return subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()


def append(variable: str, **values: str) -> None:
    """Append name=value lines to a GitHub Actions file such as GITHUB_OUTPUT or GITHUB_ENV."""
    with open(os.environ[variable], "a", encoding="utf-8") as handle:
        handle.writelines(f"{name}={value}\n" for name, value in values.items())


def full_sha(value: str) -> bool:
    return re.fullmatch(r"[0-9a-f]{40}", value) is not None


def require_default_branch(ref: str, ref_name: str, branch: str) -> None:
    if ref != f"refs/heads/{branch}":
        raise SystemExit(f"Manual publication must run from the default branch; got {ref_name}")


def wait_for_ci(sha: str, branch: str) -> None:
    if not full_sha(sha):
        raise SystemExit("Invalid source commit SHA")
    deadline = time.monotonic() + CI_WAIT_SECONDS
    last_error = ""
    while time.monotonic() < deadline:
        try:
            runs = json.loads(run(
                "gh", "run", "list", "--workflow", "ci.yml", "--commit", sha, "--event", "push",
                "--limit", "20", "--json", "databaseId,status,conclusion,headBranch,headSha,event,url,createdAt",
            ))
        except subprocess.CalledProcessError as error:
            # A transient gh or API failure is retried until the deadline.
            last_error = (error.stderr or "").strip() or str(error)
            print(f"gh run list failed; retrying: {last_error}", flush=True)
            time.sleep(CI_POLL_SECONDS)
            continue
        last_error = ""
        matching = [
            item for item in runs
            if item["headSha"] == sha and item["headBranch"] == branch and item["event"] == "push"
        ]
        latest = max(matching, key=lambda item: item["createdAt"], default=None)
        if latest is not None and latest["status"] == "completed":
            if latest["conclusion"] != "success":
                raise SystemExit(f"CI run {latest['databaseId']} concluded {latest['conclusion']}: {latest['url']}")
            print(f"CI run {latest['databaseId']} succeeded for {sha}: {latest['url']}")
            append("GITHUB_OUTPUT", **{"verified-sha": sha})
            return
        if latest is not None:
            print(f"Waiting for CI run {latest['databaseId']}: {latest['status']}", flush=True)
        else:
            print(f"Waiting for a {branch} CI run for {sha}", flush=True)
        time.sleep(CI_POLL_SECONDS)
    raise SystemExit(f"Timed out waiting for successful CI on {sha}" + (f"; last gh error: {last_error}" if last_error else ""))


def local_tag_commit(tag: str) -> str | None:
    if run("git", "tag", "--list", tag) != tag:
        return None
    commit = run("git", "rev-list", "-n", "1", f"refs/tags/{tag}^{{commit}}")
    if not commit:
        raise SystemExit(f"Unable to resolve local release tag {tag}")
    return commit


def release_metadata(ref: str, ref_name: str, sha: str, publish: bool) -> None:
    config = json.loads((ROOT / "apps/desktop/src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    version = config["version"]
    tag = ref_name if ref.startswith("refs/tags/") else f"v{version}"
    append("GITHUB_ENV", RELEASE_VERSION=version, RELEASE_TAG=tag)
    if publish:
        existing = local_tag_commit(tag)
        if existing is not None and existing != sha:
            raise SystemExit(f"Release tag {tag} already points to {existing}; expected {sha}")
    append("GITHUB_ENV", ARTIFACT_VERSION=version if publish else f"{version}-preview-{sha}")
    validator = subprocess.run([sys.executable, "tools/phase4/validate_release_metadata.py", "--tag", tag], cwd=ROOT)
    if validator.returncode:
        raise SystemExit(validator.returncode)


def package_arguments(publish: bool, event: str, ref: str, branch: str, sha: str, verified_sha: str) -> list[str]:
    reuse_ci = bool(verified_sha)
    if reuse_ci and (not full_sha(verified_sha) or verified_sha != sha):
        raise SystemExit("Verified CI SHA does not match this source commit")
    main_preview = event == "workflow_dispatch" and ref == f"refs/heads/{branch}"
    if (publish or main_preview) and not reuse_ci:
        raise SystemExit("Default-branch preview and publication require verified CI")
    if reuse_ci and not publish and not main_preview:
        raise SystemExit("Only default-branch previews can reuse verified CI")
    if publish:
        return ["--skip-validation"]
    return ["--preview", "--skip-validation"] if reuse_ci else ["--preview"]


def remote_tag_commit(tag: str) -> str | None:
    tag_ref = f"refs/tags/{tag}"
    try:
        lines = run("git", "ls-remote", "origin", tag_ref, f"{tag_ref}^{{}}").splitlines()
    except subprocess.CalledProcessError as error:
        raise SystemExit(f"Unable to inspect remote release tag {tag}") from error
    direct = None
    for parts in (line.split() for line in lines):
        if len(parts) < 2:
            continue
        if parts[1] == f"{tag_ref}^{{}}":
            return parts[0]
        if parts[1] == tag_ref:
            direct = parts[0]
    return direct


def release_tag(tag: str, sha: str, event: str) -> None:
    # Manual publication creates the tag; a tag push or an existing tag is only verified.
    if event == "workflow_dispatch" and not remote_tag_commit(tag):
        local = local_tag_commit(tag)
        if local is None:
            run("git", "config", "user.name", "github-actions[bot]")
            run("git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
            run("git", "tag", "-a", tag, "-m", f"Release {tag}", sha)
        elif local != sha:
            raise SystemExit(f"Local release tag {tag} already points to {local}; expected {sha}")
        # A push that loses a race with a concurrent creation is judged by the check below.
        subprocess.run(["git", "push", "origin", tag], cwd=ROOT, check=False)
    remote = remote_tag_commit(tag)
    if remote != sha:
        raise SystemExit(f"Remote release tag {tag} points to {remote or ''}; expected {sha}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gate", choices=["require-default-branch", "wait-for-ci", "release-metadata", "package", "release-tag"])
    gate = parser.parse_args().gate
    env = os.environ.get
    if gate == "require-default-branch":
        require_default_branch(env("GITHUB_REF", ""), env("GITHUB_REF_NAME", ""), env("RELEASE_BRANCH", ""))
    elif gate == "wait-for-ci":
        wait_for_ci(env("GITHUB_SHA", ""), env("RELEASE_BRANCH", ""))
    elif gate == "release-metadata":
        release_metadata(env("GITHUB_REF", ""), env("GITHUB_REF_NAME", ""), env("GITHUB_SHA", ""),
                         env("PUBLISH_RELEASE") == "true")
    elif gate == "package":
        arguments = package_arguments(
            env("PUBLISH_RELEASE") == "true", env("GITHUB_EVENT_NAME", ""), env("GITHUB_REF", ""),
            env("RELEASE_BRANCH", ""), env("GITHUB_SHA", ""), env("VERIFIED_CI_SHA", ""),
        )
        raise SystemExit(subprocess.run([sys.executable, "tools/phase4/package_release.py", *arguments], cwd=ROOT).returncode)
    else:
        release_tag(env("RELEASE_TAG", ""), env("GITHUB_SHA", ""), env("GITHUB_EVENT_NAME", ""))


if __name__ == "__main__":
    main()
