from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase4 import benchmark_build_metadata as metadata


class BuildMetadataTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary_root = Path(__file__).resolve().parents[2] / ".codex-tmp"
        temporary_root.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=temporary_root)
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / "project"
        self.root.mkdir()
        self.home = self.base / "cargo-home"
        self.home.mkdir()
        self.manifest = self.root / "Cargo.toml"
        self.manifest.write_text('[package]\nname="fixture"\nversion="0.1.0"\n', encoding="utf-8")
        self.source = self.root / "src" / "lib.rs"
        self.source.parent.mkdir()
        self.source.write_text("pub fn value() -> u32 { 1 }\n", encoding="utf-8")
        for arguments in (("init", "--quiet"), ("add", "Cargo.toml", "src/lib.rs")):
            subprocess.run(["git", *arguments], cwd=self.root, check=True, capture_output=True)
        self.environment = {"CARGO_HOME": str(self.home), "PATH": os.environ.get("PATH", "")}
        self.command = ["cargo", "run", "--release", "--manifest-path", str(self.manifest), "--", "--repeats=5"]
        original = subprocess.check_output

        def output(command, **kwargs):
            if command[:2] == ["git", "rev-parse"]:
                return "a" * 40 + "\n"
            if "--version" in command:
                self.assertIn("--verbose", command)
                return f"{command[0]} 1.97.0\nhost: x86_64-pc-windows-msvc\n"
            return original(command, **kwargs)

        self.addCleanup(patch.stopall)
        patch.object(metadata.subprocess, "check_output", side_effect=output).start()
        self.discover_config_paths = metadata._cargo_config_paths
        self.config_paths = patch.object(metadata, "_cargo_config_paths", return_value=[]).start()

    def capture(self, **extra):
        return metadata.capture_build_metadata(
            self.root, self.manifest, {**self.environment, **extra}, self.command,
        )

    def config(self, text: str, name="config.toml") -> Path:
        path = self.root / ".cargo" / name
        path.parent.mkdir(exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_stable_source_and_compiler_identity_ignore_output_directory_and_request(self):
        first = self.capture(CARGO_TARGET_DIR="first-output")
        self.command[-1] = "--repeats=10"
        second = self.capture(CARGO_TARGET_DIR="second-output", CARGO_BUILD_JOBS="2")
        self.assertEqual(first["source"]["commit"], "a" * 40)
        self.assertEqual(first["source"]["fingerprint"], second["source"]["fingerprint"])
        self.assertEqual(first["compiler_variant_fingerprint"], second["compiler_variant_fingerprint"])
        self.assertNotEqual(first["invocation"], second["invocation"])
        self.assertEqual(second["invocation"]["output_environment"]["CARGO_TARGET_DIR"], "second-output")
        self.assertIn("rustc 1.97.0", first["compiler"]["rustc_version_verbose"])

    def test_source_bytes_deletions_and_untracked_inputs_change_fingerprint(self):
        initial = self.capture()
        self.source.write_text("pub fn value() -> u32 { 2 }\n", encoding="utf-8")
        edited = self.capture()
        self.assertNotEqual(initial["source"]["fingerprint"], edited["source"]["fingerprint"])
        self.assertTrue(edited["source"]["dirty"])
        self.assertNotEqual(initial["source"]["files"]["src/lib.rs"], edited["source"]["files"]["src/lib.rs"])
        added = self.root / "src" / "extra.rs"
        added.write_text("pub const EXTRA: bool = true;", encoding="utf-8")
        untracked = self.capture()
        self.assertIn("src/extra.rs", untracked["source"]["files"])
        self.assertNotEqual(edited["source"]["fingerprint"], untracked["source"]["fingerprint"])
        self.source.unlink()
        deleted = self.capture()
        self.assertIsNone(deleted["source"]["files"]["src/lib.rs"])
        self.assertNotEqual(untracked["source"]["fingerprint"], deleted["source"]["fingerprint"])

    def test_generated_noise_and_secret_files_are_never_read(self):
        initial = self.capture()
        excluded = ["IGNORE/probe.rs", ".codex-tmp/local.rs", "target/debug/build.rs",
                    "node_modules/lib.js", ".env", "credentials.toml", "private-key.pem", "secrets.json",
                    "benchmark-report.json", ".npmrc", ".netrc", "production.env"]
        for name in excluded:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("DO-NOT-READ-SENTINEL", encoding="utf-8")
        subprocess.run(["git", "add", "--force", ".npmrc", ".netrc", "production.env"],
                       cwd=self.root, check=True, capture_output=True)
        original = Path.read_bytes

        def read_bytes(path):
            self.assertNotIn(path.relative_to(self.root).as_posix(), excluded)
            return original(path)

        with patch.object(Path, "read_bytes", read_bytes):
            result = self.capture()
        self.assertEqual(initial["source"]["fingerprint"], result["source"]["fingerprint"])
        self.assertNotIn("DO-NOT-READ-SENTINEL", json.dumps(result))

    def test_profile_config_and_compiler_flag_variants_have_distinct_fingerprints(self):
        variants = [self.capture(**environment)["compiler_variant_fingerprint"] for environment in (
            {}, {"RUSTFLAGS": "-C target-cpu=native"}, {"CARGO_ENCODED_RUSTFLAGS": "-C\x1ftarget-cpu=native"},
            {"CARGO_PROFILE_RELEASE_LTO": "thin"}, {"CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "1"},
            {"CARGO_INCREMENTAL": "1"}, {"CL": "/GL"},
        )]
        config = self.config('[profile.release]\nlto="thin"\n')
        self.config_paths.return_value = [config]
        variants.append(self.capture()["compiler_variant_fingerprint"])
        config.write_text('[profile.release]\nlto="fat"\n', encoding="utf-8")
        variants.append(self.capture()["compiler_variant_fingerprint"])
        self.config_paths.return_value = []
        self.manifest.write_text(self.manifest.read_text() + '[profile.release]\nlto=true\n', encoding="utf-8")
        manifest_variant = self.capture()
        variants.append(manifest_variant["compiler_variant_fingerprint"])
        self.assertEqual(manifest_variant["compiler"]["manifest_profiles"], {"release": {"lto": True}})
        self.assertEqual(len(variants), len(set(variants)))

    def test_config_files_are_hashed_and_secrets_never_recorded(self):
        config = self.config('[registry]\ntoken = "SECRET-REGISTRY-SENTINEL"\n')
        self.config_paths.return_value = [config]
        result = self.capture(CARGO_REGISTRY_TOKEN="SECRET-PROCESS-SENTINEL",
                              CARGO_HTTP_PROXY="http://SECRET-PROXY-SENTINEL", PRIVATE_API_KEY="SECRET-NATIVE-SENTINEL",
                              CARGO_PROFILE_RELEASE_LTO="thin", CL="/GL")
        self.assertEqual(list(result["compiler"]["cargo_config_files"]), [str(config)])
        self.assertEqual(result["compiler"]["environment"], {"CARGO_PROFILE_RELEASE_LTO": "thin", "CL": "/GL"})
        self.assertNotIn("SECRET-", json.dumps(result))

    def test_discovery_prefers_legacy_and_uses_cwd_ancestors_not_manifest_directory(self):
        legacy = self.config('[profile.release]\nlto=true\n', "config")
        self.config('[profile.release]\nlto=false\n')
        parent = self.base / ".cargo"
        parent.mkdir()
        (parent / "config.toml").write_text("", encoding="utf-8")
        (self.home / "config.toml").write_text("", encoding="utf-8")
        paths = self.discover_config_paths(self.root, self.environment)
        self.assertIn(legacy, paths)
        self.assertNotIn(legacy.with_name("config.toml"), paths)
        self.assertLess(paths.index(self.home / "config.toml"), paths.index(parent / "config.toml"))
        self.assertLess(paths.index(parent / "config.toml"), paths.index(legacy))

    def test_symlink_source_is_rejected_before_reading_target(self):
        secret = self.base / "secrets.json"
        secret.write_text("DO-NOT-READ", encoding="utf-8")
        link = self.root / "src" / "linked.rs"
        try:
            link.symlink_to(secret)
        except OSError as error:
            self.skipTest(f"symlink creation unavailable: {error.winerror}")
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.capture()

    def test_process_compiler_override_is_queried(self):
        overridden = self.capture(CARGO_BUILD_RUSTC="environment-rustc")
        self.assertTrue(overridden["compiler"]["rustc_version_verbose"].startswith("environment-rustc"))


if __name__ == "__main__":
    unittest.main()
