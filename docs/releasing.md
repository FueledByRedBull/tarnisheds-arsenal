# Releasing Tarnished's Arsenal

Releases are created from Git tags. A manual run of the Release workflow builds
and uploads a workflow artifact for inspection, but it never publishes a GitHub
release.

## Prepare a release

1. Set the same version in:
   - `apps/desktop/package.json`
   - `apps/desktop/package-lock.json` (top level and root package)
   - `apps/desktop/src-tauri/tauri.conf.json`
   - `apps/desktop/src-tauri/Cargo.toml` and its two local-package lock entries
   - `core/er_optimizer_core/Cargo.toml` and its local-package lock entry
2. Add `docs/release-notes/v<version>.md` and update the release-notes index.
3. Validate all metadata:

   ```powershell
   python tools/phase4/validate_release_metadata.py --tag v<version>
   ```

4. Run the complete release build:

   ```powershell
   python tools/phase4/package_release.py
   ```

   The command refuses to overwrite an existing version directory by default. Use
   `--replace-output` only when deliberately refreshing the known files for the same
   local pre-commit build; it refuses directories containing unexpected entries.
   Packaging installs the locked Chromium runtime when needed and runs the
   Playwright frontend-contract suite before the Tauri production build.

5. Review the diff and generated checksums, then commit and push `main` normally.
6. Wait for the ordinary `CI` workflow to succeed on that exact commit.

## Publish

Create and push an annotated tag that exactly matches the configured version:

```powershell
git tag -a v<version> -m "Release v<version>"
git push origin v<version>
```

The tag-triggered workflow repeats all release gates and publishes:

- `TarnishedsArsenal_<version>_x64_en-US.msi`
- `TarnishedsArsenal_<version>_portable.exe`
- `TarnishedsArsenal_<version>.zip`
- `TarnishedsArsenal_<version>_SHA256SUMS.txt`
- `TarnishedsArsenal_<version>_build-report.json`
- `TarnishedsArsenal_<version>_data-validation.json`

The release workflow independently verifies that the ordinary `CI` workflow has
already succeeded for the exact tag commit. If a tag is pushed while CI is still
running, release packaging waits; failed, cancelled, missing, or timed-out CI blocks
publication. Rust, Python, Node, and validation-tool versions are pinned so local, CI,
and release checks do not silently drift apart.

Packaging refuses a dirty input tree and fails if the build changes tracked source
other than Windows-only CRLF normalization.
Before upload, the workflow independently checks the build commit, clean-source and
validation flags, completed gates, artifact sizes, and EXE/MSI SHA-256 hashes.
The final Tauri bundle build also runs Cargo in locked mode.

Both binaries contain the verified compile-time Vanilla and Convergence runtime
snapshots. Neither needs an adjacent data directory, `regulation.bin`, FMG XML, or
source workbook. The build report must identify both profile manifest IDs and the
validation report covers both snapshots. Authenticode signing is conditional:
configure the protected `WINDOWS_SIGNING_CERTIFICATE_BASE64` and
`WINDOWS_SIGNING_CERTIFICATE_PASSWORD` repository secrets, plus the optional
`WINDOWS_SIGNING_TIMESTAMP_URL` variable. The package job signs the executable,
rebuilds the MSI around it, signs the MSI, verifies both signatures, and records
`codeSigned` and the `windows-code-signing` provenance gate. Without both secrets,
the artifacts remain explicitly unsigned and do not claim that gate.

The packager refuses a dirty worktree and records the exact source commit with
`sourceDirty: false`. Run targeted pre-commit checks directly; create release
artifacts only after the intended source is committed.
