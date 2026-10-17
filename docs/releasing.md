# Releasing Tarnished's Arsenal

[Home](../README.md) · [Release notes](release-notes/README.md) · [Extraction guide](../tools/phase1/README.md)

Prepare the version and documentation together, merge validated source, then use
one workflow run to build, tag, and publish. Final download links are committed
before publication, so no follow-up documentation commit is needed.

[Prepare](#prepare-a-release) · [Publish](#publish) · [Preview](#build-a-preview) · [Retry](#retry-publication) · [Package checks](#package-and-signing-checks) · [Repository settings](#repository-settings)

| Stage | What must be true before continuing |
| --- | --- |
| **Prepare** | App/core versions, lockfiles, release notes, and download links agree |
| **CI** | Required checks pass; ordinary CI succeeds on the exact merged commit |
| **Package** | MSI, portable EXE, embedded data, startup, and hashes pass verification |
| **Publish** | All five remote assets match the original verified package |

## Prepare a release

1. Run the version preparation helper:

   ```powershell
   python tools/phase4/prepare_release.py <version>
   ```

   It updates the synchronized application/core manifests and local lock entries.
   Keep the WiX `upgradeCode` pinned to the historical product-family GUID; changing
   the display name must not create a second Windows Installer product family.
2. Add `docs/release-notes/v<version>.md` and update the release-notes index with
   the final GitHub Releases URL. The URL is deterministic, so use the real link
   in the release preparation commit; do not leave a `Pending publication` row
   that needs a follow-up documentation commit.
   Update the README's current-release links. Keep historical notes at their
   original versions. The release body is copied from the notes file: use
   absolute GitHub URLs for its cross-document links so they work there too, and
   word its Downloads section so it still reads correctly after publication.
3. Validate all metadata and run the targeted source checks:

   ```powershell
   python tools/phase4/validate_release_metadata.py --tag v<version>
   python -m unittest discover -s tools/phase1 -p 'test_*.py'
   python -m unittest discover -s tools/phase4 -p 'test_*.py'
   ```

4. Review the diff, commit the release preparation on a branch, and open a pull
   request. Merge after `rust-and-data`, `linux-core-and-data`, and
   `desktop-frontend` pass with the branch up to date. Making these checks
   required on `main` is a [repository setting](#repository-settings).
5. Wait for the ordinary `CI` workflow to succeed on that exact commit. The release
   workflow is the source of truth for the complete package build. If a local
   rehearsal is useful, run `python tools/phase4/package_release.py` from this clean
   committed checkout after the push; it refuses dirty source and does not replace
   the CI artifacts. It also refuses an existing `dist/TarnishedsArsenal_<version>`
   output or ZIP; `--replace-output` deliberately refreshes only that output's
   known files and stops if the directory holds anything else.

Frontend source validation includes `npm run lint` and `npm run test:contracts`,
before browser installation and unit/browser tests. Full local package validation
runs these gates too. A preview reusing verified exact-commit CI retains that proof
and skips duplicate source checks; package-specific validation is still mandatory.

## Publish

Run the Release workflow from the default branch with `publish` enabled:

```powershell
gh workflow run release-package.yml --ref main -f publish=true
```

It waits for successful CI on the exact source commit and builds and verifies
the package. The publish job then creates and pushes the annotated `v<version>`
tag at that commit, creates a draft release, uploads the assets and publishes the
draft. A failed package creates neither a tag nor a release. If publication fails
after the tag push, [retry](#retry-publication); to abandon the release instead,
delete the draft release and the remote tag by hand. The five release assets are:

- `TarnishedsArsenal_<version>_x64_en-US.msi`
- `TarnishedsArsenal_<version>_portable.exe`
- `TarnishedsArsenal_<version>.zip`
- `TarnishedsArsenal_<version>_SHA256SUMS.txt`
- `TarnishedsArsenal_<version>_build-report.json`

`SHA256SUMS.txt` lists the MSI, the portable executable and the ZIP. The ZIP is a
portable-only archive with the standalone executable, release documentation,
`LICENSE` and `NOTICE`; the MSI installs `LICENSE` and `NOTICE` beside the
application. Verification requires exactly one executable at the expected path
and checks its bytes against the standalone portable executable's SHA-256; matching
filenames alone are insufficient. The MSI remains a separate release asset.

### Alternative: push an explicit tag

Create and push an annotated tag matching the configured version, from the exact
commit validated by default-branch CI:

```powershell
git tag -a v<version> -m "Release v<version>"
git push origin v<version>
```

The tag-triggered workflow performs the same CI wait, package verification, and
publication. Because the tag already exists, a failed package leaves it without a
release: delete the tag by hand before retrying, or release a new version.
Choose one publication path for a release.

## Build a preview

Build-only runs can use any branch and an already released application version.
On the default branch, they wait for a successful `ci.yml` push run on that exact
commit and reuse its source validation. Other branches run full source validation
in the packaging job. Artifacts are named `<version>-preview-<full commit SHA>`.
Previews neither create nor require a release tag. For the same local check, use
`python tools/phase4/package_release.py --preview` from clean committed source;
add `--skip-validation` only when successful exact-commit CI has already validated
that source. The workflow verifies the default branch, event, commit and CI result
before passing that flag, and logs the successful CI run URL.

```powershell
gh workflow run release-package.yml --ref <branch> -f publish=false
```

## Retry publication

Packaging and publication are separate jobs. If publication fails, use **Re-run
failed jobs** on that original workflow run. The publish job downloads the original
verified package, checks every existing remote asset against it, and uploads only
missing files. It publishes the draft only after all five assets match, then checks
the published release again. A mismatching remote file stops publication; files
are never silently replaced. Starting another build is not a publication retry.

## Package and signing checks

Publication independently verifies that ordinary `CI` has succeeded for
the exact source commit. Default-branch previews reuse the same verification;
other previews validate source in the packaging job, as described
[above](#build-a-preview).
Normal source tests, lint, type checks, formatting, Clippy, data snapshot
validation and a seeded comparison with the pinned reference calculator belong to
that CI run. Independent Windows jobs run the test and release profiles for both
Rust crates; a Linux job repeats the core, Python and snapshot checks on a second
OS. Every default-branch push runs the full suite.

On pull requests, [`ci_scope.py`](../tools/phase4/ci_scope.py) skips the Rust, data
and Linux jobs only when every changed path is `README.md`, `LICENSE`, a Markdown
or image file under `docs/`, or frontend source (`apps/desktop/src/`,
`apps/desktop/tests/` and the few frontend config files it lists). Frontend jobs
are skipped only when every path is one of those documentation files. Any other
path runs every check. The required `rust-and-data` aggregate accepts only
successful selected jobs and explicitly unselected skips. Pull requests that
change the packaging path (this workflow, `package_release.py`,
`publish_release.py`, `release_gate.py`, `tauri.conf.json`, the desktop
`Cargo.toml` or the core `build.rs`) also build a full preview package.

Every CI run first lints all workflows with actionlint 1.7.12 (with the runner's
shellcheck) and the Dependabot and workflow files with zizmor 1.30.1's offline
audits. Both are release binaries whose SHA-256 is pinned in `ci.yml`; update the
URL and hash together. The release workflow's gating logic (default branch,
verified CI wait, release tag and packaging mode) lives in
[`release_gate.py`](../tools/phase4/release_gate.py) with unit tests, not inline
in the workflow. The release package job uses no dependency caches, so a poisoned
cache cannot reach a release build.

CI's core tests use optimization level 1 with debug assertions and overflow
checks enabled; backend tests keep the default test profile. Release tests retain
the shipping ThinLTO profile. Both profiles run library and level-range benchmark
assertion tests; Clippy and release checks cover all targets. The release job
packages the already validated commit from a clean Cargo target directory and
keeps the release-only MSI identity, MSI payload, packaged startup smoke, signing,
and checksum checks. The final Tauri build runs Cargo in locked mode. The packaged
smoke sets `TARNISHEDS_ARSENAL_SMOKE_DEBUG_PORT` and an isolated
`WEBVIEW2_USER_DATA_FOLDER`; the app opens a loopback remote-debugging port only when
both are present and the folder is a `tarnisheds-arsenal-smoke-*` directory. There is no
command-line switch, so shortcuts, file associations and other users cannot enable it.
WebView2's own `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` would need no app code, but
GitHub's Windows runners do not honour it although it works locally. The same variable
also makes a failed start report its error on stderr without the startup message box,
which would otherwise hold back the exit code.

Both binaries contain the verified compile-time Vanilla and Convergence runtime
snapshots. Neither needs an adjacent data directory, `regulation.bin`, FMG XML, or
source workbook. The build report identifies both profile manifest IDs and the
exact source commit. CI keeps the data-validation report as a workflow artifact;
it is not duplicated as an end-user release asset. Published packages also carry
GitHub build provenance attestations for the executable, MSI and ZIP; check a
download with
`gh attestation verify <file> --repo FueledByRedBull/tarnisheds-arsenal`.

Releases are currently unsigned and say so. Authenticode signing is conditional:
configure the `WINDOWS_SIGNING_CERTIFICATE_BASE64` and
`WINDOWS_SIGNING_CERTIFICATE_PASSWORD` secrets, plus the optional
`WINDOWS_SIGNING_TIMESTAMP_URL` variable. `package_release.py` removes both
secrets from its environment before any build step, imports the certificate into
the current user's store through `pwsh.exe` (the password reaches only that child
process's environment), signs by thumbprint and removes the certificate
afterwards. The package job builds the executable
without bundling, then uses Tauri's signing hook to sign the MSI executable after
its bundle marker is patched. The hook checks that its input differs from the
compiled portable only by that marker. After Tauri restores the portable, the job
signs the portable. Tauri also invokes the hook for its extension DLLs and the
finished MSI; those targets are signed and verified without applying the EXE-only
marker check or overwriting the captured payload. The finished MSI is verified
again without a second signing pass.
It verifies the signatures and extracts the MSI administrative payload. Unsigned
payloads must match the portable executable except for Tauri's exact `UNK`/`MSI`
bundle marker; signed payloads must match the signed installer variant captured
by the hook. The portable marker is preserved. It records `codeSigned` and the `windows-code-signing`
provenance gate. Without both secrets, the artifacts remain explicitly unsigned
and do not claim that gate.

The Dependency audit workflow runs `cargo audit` over both Cargo lockfiles and
`npm audit` over the bundled runtime packages weekly and when a lockfile changes.
Dependabot opens weekly update pull requests for actions, Cargo, npm and the
Python validation requirements.

`requirements-validation.txt` pins every validation package, including pyright's
dependencies, with the SHA-256 of each PyPI file of that version, so the same file
verifies on Windows and Linux. CI installs it with `--require-hashes`; when a pin
changes, take the new hashes from PyPI (Dependabot updates them in its pull
requests). pyright runs the npm package bundled in its wheel, so the wheel hash
covers it and nothing is fetched from npm. It runs on the Node.js found on `PATH`
(the runner's, or setup-node's in the release job). Do not set
`PYRIGHT_PYTHON_FORCE_VERSION`: that makes pyright download an unverified npm
package.

The packager refuses a dirty worktree and records the exact source commit with
`sourceDirty: false`. Run targeted pre-commit checks directly; create local release
artifacts only after the intended source is committed.

## Repository settings

These protections are GitHub settings, not workflow code, so they cannot be
confirmed from the repository itself. All are configured except the signing secrets:
releases are deliberately unsigned for now.

| Setting | Expected configuration |
| --- | --- |
| Branch protection on `main` | Require a pull request and the `rust-and-data`, `linux-core-and-data` and `desktop-frontend` checks with the branch up to date; applies to admins; no force pushes or deletion |
| Tag ruleset for `v*` | Nobody but repository admins may move or delete release tags. Creation stays open so the publish job can create the tag |
| `release` environment | Required reviewer: the maintainer. Deployment branches and tags: `main` and `v*`. The publish job already uses it; GitHub creates it unprotected until configured |
| Signing secrets | Store `WINDOWS_SIGNING_CERTIFICATE_BASE64` and `WINDOWS_SIGNING_CERTIFICATE_PASSWORD` as repository secrets. The package job passes them to its build step only on publishing runs (version tags or a manual `publish=true` run), so pull request and preview packages are unsigned. Do not add `environment: release` to the package job: it also runs for pull requests and previews, which that environment does not allow |
| Private vulnerability reporting | Enabled, so the [security policy](../SECURITY.md) link works |
| Dependabot alerts and security updates | Enabled |

<details>
<summary>Why the WiX upgrade code stays fixed</summary>

The pinned value was verified directly from the published v0.8.1, v0.9.0,
v0.9.1, and v0.9.2 MSI Property tables. v0.10.0 is the known one-release fork
caused by its unpinned display-name change and is not the identity source of truth.

</details>
