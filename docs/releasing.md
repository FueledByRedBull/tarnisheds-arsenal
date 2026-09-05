# Releasing Tarnished's Arsenal

Releases are created from Git tags. The Release workflow can package a tag that
already exists, or create the configured version tag and publish it when a manual
run is started from the default branch with `publish` enabled.

## Prepare a release

1. Run the version preparation helper:

   ```powershell
   python tools/phase4/prepare_release.py <version>
   ```

   It updates the synchronized application/core manifests and local lock entries.
   Keep the WiX `upgradeCode` pinned to the historical product-family GUID; changing
   the display name must not create a second Windows Installer product family.
   The pinned value was verified directly from the published v0.8.1, v0.9.0,
   v0.9.1, and v0.9.2 MSI Property tables. v0.10.0 is the known one-release fork
   caused by its unpinned display-name change and is not the identity source of truth.
2. Add `docs/release-notes/v<version>.md` and update the release-notes index with
   the final GitHub Releases URL. The URL is deterministic, so use the real link
   in the release preparation commit; do not leave a `Pending publication` row
   that needs a follow-up documentation commit.
3. Validate all metadata and run the targeted source checks:

   ```powershell
   python tools/phase4/validate_release_metadata.py --tag v<version>
   python -m unittest discover -s tools/phase1 -p 'test_*.py'
   python -m unittest discover -s tools/phase4 -p 'test_*.py'
   ```

4. Review the diff, commit the release preparation, and push `main` normally.
5. Wait for the ordinary `CI` workflow to succeed on that exact commit. The release
   workflow is the source of truth for the complete package build. If a local
   rehearsal is useful, run `python tools/phase4/package_release.py` from this clean
   committed checkout after the push; it refuses dirty source and does not replace
   the CI artifacts.

## Publish

For the explicit tag path, create and push an annotated tag that exactly matches
the configured version:

```powershell
git tag -a v<version> -m "Release v<version>"
git push origin v<version>
```

The tag-triggered workflow waits for successful ordinary CI on the exact tagged
commit, then builds and publishes:

- `TarnishedsArsenal_<version>_x64_en-US.msi`
- `TarnishedsArsenal_<version>_portable.exe`
- `TarnishedsArsenal_<version>.zip`
- `TarnishedsArsenal_<version>_SHA256SUMS.txt`
- `TarnishedsArsenal_<version>_build-report.json`

To make tagging and publication one operation, run the Release workflow from the
default branch with `publish` enabled. It performs the same exact-commit CI wait,
packages the commit, and asks GitHub to create `v<version>` at that commit while
publishing the release. A failed package never creates a release. A manual run with
`publish` disabled only uploads the package for inspection.

```powershell
gh workflow run release-package.yml --ref main -f publish=true
```

The release workflow independently verifies that ordinary `CI` has succeeded for
the exact source commit. Normal source tests, lint, type checks, formatting, Clippy,
and data validation belong to that CI run; the release job packages the already
validated commit and keeps the release-only MSI identity, MSI payload, packaged
startup smoke, signing, and checksum checks. The final Tauri build runs Cargo in
locked mode.

Both binaries contain the verified compile-time Vanilla and Convergence runtime
snapshots. Neither needs an adjacent data directory, `regulation.bin`, FMG XML, or
source workbook. The build report identifies both profile manifest IDs and the
exact source commit. CI keeps the data-validation report as a workflow artifact;
it is not duplicated as an end-user release asset. Authenticode signing is conditional:
configure the protected `WINDOWS_SIGNING_CERTIFICATE_BASE64` and
`WINDOWS_SIGNING_CERTIFICATE_PASSWORD` repository secrets, plus the optional
`WINDOWS_SIGNING_TIMESTAMP_URL` variable. The package job signs the executable,
rebuilds the MSI around it, signs the MSI, verifies both signatures, extracts the
MSI administrative payload, and compares its executable bytes with the tested
portable executable. It records `codeSigned` and the `windows-code-signing`
provenance gate. Without both secrets, the artifacts remain explicitly unsigned
and do not claim that gate.

The packager refuses a dirty worktree and records the exact source commit with
`sourceDirty: false`. Run targeted pre-commit checks directly; create local release
artifacts only after the intended source is committed.
