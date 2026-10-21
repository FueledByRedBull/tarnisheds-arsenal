"""Capture scoped source identity and compiler inputs for local release benchmarks."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import tomllib
from pathlib import Path
from typing import Any

# Compiler-affecting variables; Cargo's own are matched by prefix in _compiler_env_name.
ENV_KEYS = {
    "RUSTFLAGS", "RUSTDOCFLAGS", "RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
    "RUSTC_BOOTSTRAP", "RUSTUP_TOOLCHAIN",
    "CC", "CXX", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "AR", "ARFLAGS",
    "CL", "_CL_", "_LINK_", "LIB", "INCLUDE",
}
# Output locations and parallelism do not define a compiler variant.
OUTPUT_ENV_KEYS = {"CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_BUILD_DIR", "CARGO_BUILD_JOBS"}
EXCLUDED_PARTS = {
    "ignore", ".codex-tmp", ".git", "target", "node_modules", "dist", "dist_release",
    "build", "build_release", "gen", "__pycache__", ".venv", "venv", ".pytest_cache",
    ".mypy_cache", ".ruff_cache", "playwright-report", "test-results",
}
SOURCE_LOCATIONS = {"src", "core", "apps", "tools", "data", "scripts", ".cargo", ".github"}
ROOT_INPUTS = {
    "cargo.toml", "cargo.lock", "rust-toolchain", "rust-toolchain.toml", "package.json",
    "package-lock.json", "pyproject.toml", "requirements-validation.txt", ".node-version",
    ".python-version", ".gitattributes", "build.rs",
}


def _fingerprint(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def _output(root: Path, command: list[str], environment: dict[str, str] | None = None) -> str:
    return subprocess.check_output(command, cwd=root, env=environment, text=True, stderr=subprocess.PIPE).strip()


def _secret_path(path: Path) -> bool:
    return any(
        part.lower().startswith(".env")
        or part.lower() in {".npmrc", ".pypirc", ".netrc", "_netrc", ".git-credentials", ".dockercfg", ".ssh", ".gnupg"}
        or re.search(r"credential|secret|token|private[-_]?key|^id_(rsa|dsa|ecdsa|ed25519)", part, re.I)
        for part in path.parts
    ) or path.suffix.lower() in {".env", ".pem", ".pfx", ".p12", ".key", ".keystore"}


def _source_path(path: str, tracked: bool) -> bool:
    relative = Path(path)
    parts = [part.lower() for part in relative.parts]
    if _secret_path(relative) or any(part in EXCLUDED_PARTS for part in parts):
        return False
    if path.startswith(("data/raw/", "data/_", "archive/python-desktop/", "ui/desktop/")):
        return False
    return tracked or parts[0] in SOURCE_LOCATIONS or path.lower() in ROOT_INPUTS


def _source_identity(root: Path) -> dict[str, Any]:
    tracked = set(filter(None, _output(root, ["git", "ls-files", "--cached", "-z"]).split("\0")))
    untracked = set(filter(None, _output(root, ["git", "ls-files", "--others", "--exclude-standard", "-z"]).split("\0")))
    selected = sorted(name for name in tracked | untracked if _source_path(name, name in tracked))
    files: dict[str, str | None] = {}
    for name in selected:
        path = root / name
        if path.is_symlink() or not path.resolve().is_relative_to(root):
            raise ValueError(f"source input is a symlink or escapes repository: {name}")
        files[name] = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None
    changed: set[str] = set()
    for options in ([], ["--cached"]):
        changed.update(filter(None, _output(root, ["git", "diff", *options, "--name-only", "-z"]).split("\0")))
    commit = _output(root, ["git", "rev-parse", "HEAD"])
    return {
        "commit": commit, "dirty": bool(set(selected) & (changed | untracked)),
        "files": files, "fingerprint": _fingerprint({"commit": commit, "files": files}),
        "scope": "tracked safe inputs and untracked source/runtime/config locations; generated and secret-like paths excluded",
    }


def _cargo_config_paths(root: Path, environment: dict[str, str]) -> list[Path]:
    cargo_home = Path(environment.get("CARGO_HOME") or Path.home() / ".cargo")
    if not cargo_home.is_absolute():
        cargo_home = root / cargo_home
    directories = [cargo_home, *(parent / ".cargo" for parent in reversed(root.parents)), root / ".cargo"]
    paths: list[Path] = []
    for directory in directories:
        path = directory / "config"
        if not path.is_file():
            path = directory / "config.toml"
        if path.is_file() and path not in paths:
            paths.append(path)
    return paths


def _compiler_env_name(name: str) -> bool:
    # An allowlist: registry tokens, proxies and other CARGO_* settings are never recorded.
    return name in ENV_KEYS or (name not in OUTPUT_ENV_KEYS and bool(
        re.fullmatch(r"CARGO_(PROFILE|BUILD|TARGET|UNSTABLE|ENCODED)_[A-Z0-9_]+|CARGO_INCREMENTAL", name)
    ))


def capture_build_metadata(
    root: Path, manifest: Path, environment: dict[str, str], command: list[str],
) -> dict[str, Any]:
    """Hash what selects the compiler build; Cargo itself resolves profiles and precedence."""
    cargo_arguments = command[:command.index("--")] if "--" in command else command
    build_arguments = [argument for argument in cargo_arguments if argument != str(manifest)]
    root, manifest = root.resolve(), manifest.resolve()
    config_files = {
        str(path): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in _cargo_config_paths(root, environment)
    }
    rustc = environment.get("RUSTC") or environment.get("CARGO_BUILD_RUSTC") or "rustc"
    compiler = {
        "cargo_version": _output(root, [command[0], "--version", "--verbose"], environment),
        "rustc_version_verbose": _output(root, [rustc, "--version", "--verbose"], environment),
        "manifest_profiles": tomllib.loads(manifest.read_text(encoding="utf-8")).get("profile", {}),
        "cargo_config_files": config_files,
        "environment": {key: value for key, value in sorted(environment.items()) if _compiler_env_name(key)},
    }
    # Config paths differ between worktrees; their ordered contents define the variant.
    variant = {**compiler, "cargo_config_files": list(config_files.values()), "build_arguments": build_arguments}
    return {
        "source": _source_identity(root), "compiler": compiler,
        "compiler_variant_fingerprint": _fingerprint(variant),
        "invocation": {
            "command": command, "cwd": str(root), "manifest": str(manifest),
            "output_environment": {key: environment[key] for key in sorted(OUTPUT_ENV_KEYS) if key in environment},
        },
    }


def require_matching_setup(
    baseline: dict[str, Any], current: dict[str, Any], keys: tuple[str, ...]
) -> None:
    """Refuse to compare timings recorded on a different profile, dataset, model or thread count."""
    differing = [
        f"{key}: baseline {baseline[key]!r}, current {current[key]!r}"
        for key in keys
        if key in baseline and key in current and baseline[key] != current[key]
    ]
    if differing:
        raise ValueError("baseline was recorded with a different setup; " + "; ".join(differing))
