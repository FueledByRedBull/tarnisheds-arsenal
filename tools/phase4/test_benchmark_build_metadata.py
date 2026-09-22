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
        second = self.capture(CARGO_TARGET_DIR="second-output")
        self.assertEqual(first["source"]["commit"], "a" * 40)
        self.assertEqual(first["source"]["fingerprint"], second["source"]["fingerprint"])
        self.assertEqual(first["compiler_variant_fingerprint"], second["compiler_variant_fingerprint"])
        self.assertNotEqual(first["invocation"], second["invocation"])
        self.assertEqual(first["compiler"]["release_profile"]["codegen-units"], 16)
        self.assertEqual(first["compiler"]["release_profile"]["lto"], False)
        self.assertEqual(first["compiler"]["release_profile"]["build-override"]["codegen-units"], 256)
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

    def test_manifest_config_and_environment_profile_precedence(self):
        self.manifest.write_text(self.manifest.read_text() + '''
[profile.release]
lto = true
codegen-units = 8
[profile.release.package.special]
opt-level = 1
[profile.release.build-override]
debug = true
''', encoding="utf-8")
        config = self.config('[profile.release]\nlto="thin"\ncodegen-units=4\n')
        self.config_paths.return_value = [config]
        result = self.capture(CARGO_PROFILE_RELEASE_CODEGEN_UNITS="2")
        profile = result["compiler"]["release_profile"]
        self.assertEqual(profile["lto"], "thin")
        self.assertEqual(profile["codegen-units"], 2)
        self.assertEqual(profile["package"]["special"]["opt-level"], 1)
        self.assertEqual(profile["build-override"]["debug"], True)

    def test_compiler_flag_and_profile_variants_have_distinct_fingerprints(self):
        variants = [self.capture(**environment)["compiler_variant_fingerprint"] for environment in (
            {}, {"RUSTFLAGS": "-C target-cpu=native"}, {"CARGO_ENCODED_RUSTFLAGS": "-C\x1ftarget-cpu=native"},
            {"CARGO_PROFILE_RELEASE_LTO": "thin"}, {"CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "1"},
        )]
        self.assertEqual(len(variants), len(set(variants)))

    def test_config_merging_preserves_arrays_and_redacts_unrelated_secrets(self):
        home = self.home / "config.toml"
        home.write_text('[build]\nrustflags=["-C", "opt-level=2"]\n[profile.release]\nlto=true\n', encoding="utf-8")
        local = self.config('''
[build]
rustflags = ["-C", "target-cpu=native"]
[profile.release]
lto = "thin"
[target.'cfg(windows)']
rustflags = ["--cfg", "fixture"]
[registry]
token = "SECRET-REGISTRY-SENTINEL"
[http]
proxy = "SECRET-PROXY-SENTINEL"
[env]
DATABASE_PASSWORD = "SECRET-ENV-SENTINEL"
RUSTFLAGS = { value = "-C opt-level=1", force = true }
''')
        self.config_paths.return_value = [home, local]
        result = self.capture(CARGO_REGISTRY_TOKEN="SECRET-PROCESS-SENTINEL")
        compiler = result["compiler"]
        self.assertEqual(compiler["cargo_config"]["build"]["rustflags"],
                         ["-C", "opt-level=2", "-C", "target-cpu=native"])
        self.assertEqual(compiler["release_profile"]["lto"], "thin")
        self.assertIn("cfg(windows)", compiler["cargo_config"]["target"])
        self.assertIn("RUSTFLAGS", compiler["cargo_config"]["env"])
        self.assertNotIn("SECRET-", json.dumps(result))
        self.assertIn("not evaluated", compiler["rustflags_resolution"])

    def test_unsupported_includes_and_cli_config_fail_explicitly(self):
        config = self.config('include=["extra.toml"]\n')
        self.config_paths.return_value = [config]
        with self.assertRaisesRegex(ValueError, "include"):
            self.capture()
        self.config_paths.return_value = []
        self.command = ["cargo", "--config", "profile.release.lto=true", "run", "--release"]
        with self.assertRaisesRegex(ValueError, "--config"):
            self.capture()

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

    def test_incremental_implicit_codegen_units_and_explicit_override(self):
        self.assertEqual(self.capture(CARGO_INCREMENTAL="1")["compiler"]["release_profile"]["codegen-units"], 256)
        self.assertEqual(self.capture(CARGO_INCREMENTAL="1", CARGO_PROFILE_RELEASE_CODEGEN_UNITS="8")
                         ["compiler"]["release_profile"]["codegen-units"], 8)

    def test_config_output_directory_changes_do_not_change_compiler_variant(self):
        config = self.config('[build]\ntarget-dir="first-output"\n')
        self.config_paths.return_value = [config]
        first = self.capture()
        config.write_text('[build]\ntarget-dir="second-output"\n', encoding="utf-8")
        second = self.capture()
        self.assertEqual(first["compiler_variant_fingerprint"], second["compiler_variant_fingerprint"])
        self.assertEqual(second["invocation"]["target_dir"], str((self.root / "second-output").resolve()))

    def test_cli_custom_profile_is_not_reported_as_release_defaults(self):
        self.command = ["cargo", "run", "--release", "--profile=custom"]
        with self.assertRaisesRegex(ValueError, "release profile"):
            self.capture()

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

    def test_native_compiler_flags_change_variant_without_recording_unrelated_environment(self):
        initial = self.capture()
        native = self.capture(CFLAGS="/O2", CL="/GL", INCLUDE="native/include", PRIVATE_API_KEY="SECRET-NATIVE-SENTINEL")
        self.assertNotEqual(initial["compiler_variant_fingerprint"], native["compiler_variant_fingerprint"])
        self.assertEqual(native["compiler"]["environment"]["CL"], "/GL")
        self.assertNotIn("SECRET-NATIVE-SENTINEL", json.dumps(native))

    def test_config_compiler_path_uses_config_base_and_process_override_takes_precedence(self):
        config = self.home / "config.toml"
        config.write_text('[build]\nrustc="bin/custom-rustc"\n', encoding="utf-8")
        self.config_paths.return_value = [config]
        configured = self.capture()
        self.assertTrue(configured["compiler"]["rustc_version_verbose"].startswith(str(self.base / "bin/custom-rustc")))
        overridden = self.capture(CARGO_BUILD_RUSTC="environment-rustc")
        self.assertTrue(overridden["compiler"]["rustc_version_verbose"].startswith("environment-rustc"))


if __name__ == "__main__":
    unittest.main()
