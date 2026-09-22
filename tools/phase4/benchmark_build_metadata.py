"""Capture scoped source identity and compiler inputs for local release benchmarks."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import tomllib
from pathlib import Path
from typing import Any

RELEASE_DEFAULTS: dict[str, Any] = {
    "opt-level": 3, "debug": False, "strip": "none", "debug-assertions": False,
    "overflow-checks": False, "lto": False, "panic": "unwind", "incremental": False,
    "codegen-units": 16, "rpath": False, "split-debuginfo": "target-dependent",
}
PROFILE_KEYS = set(RELEASE_DEFAULTS)
BUILD_KEYS = {
    "rustc", "rustc-wrapper", "rustc-workspace-wrapper", "rustdoc", "target",
    "target-dir", "build-dir", "rustflags", "rustdocflags", "incremental", "jobs",
}
TARGET_KEYS = {"linker", "runner", "rustflags", "rustdocflags"}
ENV_KEYS = {
    "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS",
    "RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTC_BOOTSTRAP",
    "RUSTUP_TOOLCHAIN", "CARGO_INCREMENTAL", "CARGO_BUILD_TARGET", "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC",
    "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTFLAGS", "CARGO_BUILD_RUSTDOCFLAGS", "CARGO_BUILD_INCREMENTAL",
    "CC", "CXX", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "AR", "ARFLAGS",
    "CL", "_CL_", "_LINK_", "LIB", "INCLUDE",
}
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
    profile_names = {f"CARGO_PROFILE_RELEASE_{key.upper().replace('-', '_')}" for key in PROFILE_KEYS}
    profile_names.update(f"CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_{key.upper().replace('-', '_')}" for key in PROFILE_KEYS)
    return name in ENV_KEYS or name in profile_names or bool(
        re.fullmatch(r"CARGO_TARGET_[A-Z0-9_]+_(LINKER|RUNNER|RUSTFLAGS|RUSTDOCFLAGS)", name)
    )


def _merge(lower: dict[str, Any], higher: dict[str, Any]) -> dict[str, Any]:
    result = dict(lower)
    for key, value in higher.items():
        old = result.get(key)
        if isinstance(old, dict) and isinstance(value, dict):
            result[key] = _merge(old, value)
        elif isinstance(old, list) and isinstance(value, list):
            result[key] = old + value
        else:
            result[key] = value
    return result


def _profile_settings(profile: dict[str, Any]) -> dict[str, Any]:
    unsupported = set(profile) - PROFILE_KEYS - {"package", "build-override"}
    if unsupported:
        raise ValueError("unsupported release profile keys: " + ", ".join(sorted(unsupported)))
    result = {key: value for key, value in profile.items() if key in PROFILE_KEYS}
    if "package" in profile:
        result["package"] = {name: _profile_settings(values) for name, values in profile["package"].items()}
    if "build-override" in profile:
        result["build-override"] = _profile_settings(profile["build-override"])
    return result


def _read_config(path: Path) -> dict[str, Any]:
    if _secret_path(Path(path.name)) or path.is_symlink():
        raise ValueError("refusing secret-like or symlink Cargo configuration")
    config = tomllib.loads(path.read_text(encoding="utf-8"))
    if any(key in config for key in ("include", "patch", "paths", "unstable")):
        raise ValueError("unsupported Cargo include, patch, paths, or unstable configuration")
    result: dict[str, Any] = {}
    if "build" in config:
        result["build"] = {key: value for key, value in config["build"].items() if key in BUILD_KEYS}
    if "profile" in config and "release" in config["profile"]:
        result["profile"] = {"release": _profile_settings(config["profile"]["release"])}
    if "target" in config:
        targets = {}
        for target, values in config["target"].items():
            if any(isinstance(value, dict) for value in values.values()):
                raise ValueError("unsupported Cargo target build-script overrides")
            targets[target] = {key: value for key, value in values.items() if key in TARGET_KEYS}
        result["target"] = targets
    if "env" in config:
        result["env"] = {
            key: ({field: setting for field, setting in value.items() if field in ("value", "force", "relative")}
                  if isinstance(value, dict) else value)
            for key, value in config["env"].items() if _compiler_env_name(key)
        }
    return result


def _profile_environment_value(value: str) -> Any:
    if value in ("true", "false"):
        return value == "true"
    if value.isdecimal():
        return int(value)
    return value


def capture_build_metadata(
    root: Path, manifest: Path, environment: dict[str, str], command: list[str],
) -> dict[str, Any]:
    root, manifest = root.resolve(), manifest.resolve()
    cargo_arguments = command[:command.index("--")] if "--" in command else command[:]
    if any(argument == "--config" or argument.startswith("--config=") for argument in cargo_arguments):
        raise ValueError("--config overrides require explicit support before benchmark provenance capture")
    if "--release" not in cargo_arguments or any(
        argument == "--profile" or argument.startswith("--profile=") for argument in cargo_arguments
    ):
        raise ValueError("benchmark provenance capture requires the standard --release profile")
    manifest_data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    if "workspace" in manifest_data or "workspace" in manifest_data.get("package", {}):
        raise ValueError("workspace profile inheritance is not supported by this benchmark collector")
    for parent in manifest.parent.parents:
        if not parent.is_relative_to(root):
            break
        parent_manifest = parent / "Cargo.toml"
        if parent_manifest.is_file() and "workspace" in tomllib.loads(parent_manifest.read_text(encoding="utf-8")):
            raise ValueError("workspace profile inheritance is not supported by this benchmark collector")
    manifest_profile = _profile_settings(manifest_data.get("profile", {}).get("release", {}))
    config: dict[str, Any] = {}
    config_inputs = []
    config_target_base = root
    config_rustc_base = root
    for path in _cargo_config_paths(root, environment):
        settings = _read_config(path)
        config_inputs.append({"path": str(path), "settings": settings, "fingerprint": _fingerprint(settings)})
        config = _merge(config, settings)
        if "target-dir" in settings.get("build", {}):
            config_target_base = path.parent.parent
        if "rustc" in settings.get("build", {}):
            config_rustc_base = path.parent.parent
    compiler_environment = {key: value for key, value in sorted(environment.items()) if _compiler_env_name(key)}
    explicit_profile = _merge(manifest_profile, config.get("profile", {}).get("release", {}))
    for key in PROFILE_KEYS:
        variable = f"CARGO_PROFILE_RELEASE_{key.upper().replace('-', '_')}"
        if variable in environment:
            explicit_profile[key] = _profile_environment_value(environment[variable])
        build_variable = f"CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_{key.upper().replace('-', '_')}"
        if build_variable in environment:
            explicit_profile.setdefault("build-override", {})[key] = _profile_environment_value(environment[build_variable])
    profile = _merge(RELEASE_DEFAULTS, explicit_profile)
    profile["build-override"] = _merge(
        {"opt-level": 0, "codegen-units": 256, "debug": False}, profile.get("build-override", {}),
    )
    incremental = environment.get("CARGO_INCREMENTAL", environment.get("CARGO_BUILD_INCREMENTAL"))
    if incremental is not None:
        if incremental not in ("0", "1", "true", "false"):
            raise ValueError("invalid Cargo incremental environment value")
        profile["incremental"] = incremental in ("1", "true")
    elif "incremental" in config.get("build", {}):
        profile["incremental"] = config["build"]["incremental"]
    if profile["incremental"] and "codegen-units" not in explicit_profile:
        profile["codegen-units"] = 256
    rustc = environment.get("RUSTC") or environment.get("CARGO_BUILD_RUSTC")
    if rustc is None:
        rustc = config.get("build", {}).get("rustc", "rustc")
        if ("/" in rustc or "\\" in rustc) and not Path(rustc).is_absolute():
            rustc = str((config_rustc_base / rustc).resolve())
    compiler = {
        "cargo_version": _output(root, [command[0], "--version", "--verbose"], environment),
        "rustc_version_verbose": _output(root, [rustc, "--version", "--verbose"], environment),
        "rustc_executable": rustc,
        "release_profile": profile,
        "profile_interpretation": "configured release profile before rustflags; target-dependent defaults unresolved; build dependency debug=false when possible; test harnesses ignore configured panic strategy",
        "manifest_release_profile": manifest_profile,
        "cargo_config": config, "cargo_config_inputs": config_inputs,
        "environment": compiler_environment,
        "rustflags_resolution": "input tables retained; cfg-target conditions and compiler-flag overrides not evaluated; Cargo [env] describes subprocess environment",
    }
    target_dir = config.get("build", {}).get("target-dir")
    target_base = config_target_base
    for name in ("CARGO_BUILD_TARGET_DIR", "CARGO_TARGET_DIR"):
        if name in environment:
            target_dir, target_base = environment[name], root
    build_arguments = []
    index = 0
    while index < len(cargo_arguments):
        argument = cargo_arguments[index]
        if argument in ("--manifest-path", "--target-dir"):
            if index + 1 >= len(cargo_arguments):
                raise ValueError(f"missing argument for {argument}")
            if argument == "--target-dir":
                target_dir, target_base = cargo_arguments[index + 1], root
            index += 2
            continue
        if argument.startswith("--manifest-path=") or argument.startswith("--target-dir="):
            if argument.startswith("--target-dir="):
                target_dir, target_base = argument.split("=", 1)[1], root
        else:
            build_arguments.append(argument)
        index += 1
    target_path = Path(target_dir) if target_dir is not None else manifest.parent / "target"
    if not target_path.is_absolute():
        target_path = target_base / target_path
    variant_config = _merge({}, config)
    variant_config["build"] = {key: value for key, value in config.get("build", {}).items() if key not in ("target-dir", "build-dir", "jobs")}
    variant = {key: value for key, value in compiler.items() if key not in ("cargo_config", "cargo_config_inputs")}
    variant.update({"cargo_config": variant_config, "build_arguments": build_arguments})
    return {
        "source": _source_identity(root), "compiler": compiler,
        "compiler_variant_fingerprint": _fingerprint(variant),
        "invocation": {"command": command, "cwd": str(root), "manifest": str(manifest),
                       "target_dir": str(target_path.resolve()),
                       "cargo_home": str(Path(environment.get("CARGO_HOME") or Path.home() / ".cargo")),
                       "build_dir_input": environment.get("CARGO_BUILD_BUILD_DIR", config.get("build", {}).get("build-dir")),
                       "cargo_build_jobs": environment.get("CARGO_BUILD_JOBS", config.get("build", {}).get("jobs"))},
    }
