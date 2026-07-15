<p align="center">
  <img src="docs/images/tarnisheds-arsenal-banner.svg" alt="Tarnished's Arsenal" width="100%">
</p>

# Tarnished's Arsenal

[![CI](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml)
[![Release Package](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/release-package.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/release-package.yml)
[![Latest Release](https://img.shields.io/github/v/release/FueledByRedBull/tarnisheds-arsenal?label=release)](https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/latest)

**Tarnished's Arsenal** is a Windows desktop optimizer for Elden Ring builds.

It searches across weapons, affinities, Ashes of War, upgrade levels, and stat distributions from one canonical build session, then carries that same session through rankings, comparisons, stat paths, and affinity breakpoints.

```text
weapon x affinity x AoW x upgrade x relevant stat distribution
```

## Download

Get the latest Windows build from [Releases](https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/latest).

- `TarnishedsArsenal_<version>_x64_en-US.msi` is the normal installer.
- `TarnishedsArsenal_<version>_portable.exe` is the self-contained standalone app.
- `TarnishedsArsenal_<version>.zip` bundles the complete release folder for archival
  or offline transfer.

Both artifacts contain the same compiled runtime snapshot. The standalone app
does not need an adjacent data directory, workbook, or other support files.
Each release also publishes SHA-256 checksums and a machine-readable build report.

## What It Answers

Most calculators are built around one exact weapon line. This app is built for open-ended questions:

- What is best if `ARC` must stay above 40?
- When does `Occult` overtake `Blood`?
- Which stat path reaches the selected target most efficiently?
- Which affinity wins if weapon, AoW, class, and level budget stay fixed?
- What happens when the same build is evaluated at every upgrade level?
- How does Shadow Realm Scadutree scaling change outgoing damage?

## Workspaces

| Workspace | Purpose |
|---|---|
| `Rankings` | Brute-force ranked build search with lockable result stats. |
| `Compare` | Selected build vs rival lines under the same budget and objective. |
| `Paths` | Current + N routing into solved target builds, level by level. |
| `Affinity Watch` | Affinity leader tracking and crossover breakpoints for a fixed setup. |

Every workspace reads from the same active session, so the app does not drift into separate mini-tools with separate assumptions.

## Interface

The v0.7.0 desktop interface uses one consistent selection model: click any
podium card or ranking row to make it the active build, then use the always-visible
Build Detail panel for full combat stats, AR split, route damage, status, stamina,
warnings, and one-click Compare, Paths, and Affinity Watch actions. Lock remains a
separate action because it changes the next search.

Rankings default to a compact comparison table instead of exposing every modeled
field as a column. The active-query strip keeps objective, character level,
reinforcement policy, handedness, game scope, dataset, and active constraints
visible before a search. At the app's narrower supported widths the decorative
podium is removed and the ranked table receives the available space; no capability
is moved into an extra menu.

AR and skill values shown in Rankings and Build Detail are raw model outputs.
Enemy defense and negation are not applied, and affected route details keep
unsupported-effect warnings adjacent to the result.

## Search Model

The optimizer can lock or open each major constraint:

| Input | Locked | Open |
|---|---|---|
| Weapon Type | Search one weapon family | Search all weapon families |
| Weapon | Search one weapon | Search all weapons |
| Affinity | Search one affinity | Search all legal affinities |
| AoW | Search one Ash of War | Search all legal Ashes of War |
| Upgrade | Evaluate exact `+N` | Evaluate the full `+0..+N` range |
| Combat Stats | Reuse exact locked result stats | Optimize within the session budget |

Supported objectives:

- `Max AR`
- `Bleed, then AR`
- `AoW First Hit (PvE)`
- `AoW Full Sequence (PvE)`

AoW objectives evaluate one legal route at a time. Inspector and Compare expose
the selected route's ordered actions/hits, damage, status buildup, physical hit
attribute, buff timing, stamina, and any explicit modeling warning. Mutually
exclusive branches are never combined into an impossible total.

The Rust optimizer keeps the search exact, but it sizes and enumerates combat
stat candidates per weapon, affinity, and Ash of War. For each loadout it only
varies stats that can affect the selected objective, folds weapon requirements
into the minimum stat floors first, and fills inactive stats only when the level
budget cannot otherwise be consumed. This keeps rankings deterministic while
making broad fixed-upgrade searches substantially smaller than a global
five-stat grid.

## Data

The runtime snapshot is generated from local game data and workbook extraction, not wiki estimates.

Included data covers:

- weapon rows by affinity
- reinforce data
- expanded calc-correct graphs
- exact AoW compatibility rows
- innate weapon passives
- passive overlays by weapon, affinity, and upgrade
- native somber weapon skill attack data
- legal AoW route assignments and explicit exclusions
- numeric PARAM effect-graph data for persistent and per-hit effects
- paired and no-two-hand-bonus behavior
- AoW attack rows for PvE damage objectives
- Shadow of the Erdtree Scadutree blessing attack scaling

The runtime files are one versioned, checksummed snapshot. External data loading is
all-or-nothing; a missing, modified, mixed, or unlisted file fails closed rather
than falling back to a different embedded file.

Current boundaries:

- enemy defense, negation, resistance growth, proc explosion damage, and
  poise/stance damage are not modeled
- route status details cover bleed, frost, poison, scarlet rot, sleep, madness, and
  death buildup, separately from proc damage
- route stamina is reported but is not an optimization objective
- temporary buff stacking is not yet modeled as a universal layer

## Development

Requirements:

- Rust stable
- Node.js / npm
- Python 3.12 for validation and data tooling

Install Python validation helpers when working on data validation, benchmarks, or
release packaging:

```powershell
python -m pip install -r requirements-validation.txt
```

Run the desktop app:

```powershell
cd apps/desktop
npm install
npm run dev
npm run tauri dev
```

Validate the main paths:

```powershell
cargo test --manifest-path core/er_optimizer_core/Cargo.toml
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
python tools/phase4/validate_phase4.py
python tools/phase4/validate_external_calculator.py  # optional network comparison
cd apps/desktop
npm test
npm run build
npm run test:e2e
```

Build a Windows release package:

```powershell
python tools/phase4/package_release.py
```

The release helper runs core and Tauri tests, builds and installs the
release-profile local validation binding, runs data validation, installs
frontend dependencies, builds the Tauri app, and writes
`dist/TarnishedsArsenal_<version>` with the MSI, standalone executable, checksums,
and build report, plus `dist/TarnishedsArsenal_<version>.zip`. See
[the release guide](docs/releasing.md) for the tag-driven release process.

## Refresh Data

Regenerate the complete runtime snapshot from a local `regulation.bin`:

```powershell
python tools/phase1/phase1_dump.py `
  --regulation data/raw/regulation.bin `
  --witchybnd C:\path\to\WitchyBND.exe `
  --output data/phase1
```

The extractor also reads this workbook from `data/phase1`:

```text
ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx
```

Extraction builds and validates a sibling staging snapshot, then promotes its data
files and installs the manifest last. Do not run the lower-level derivation scripts
individually for a release snapshot.

## Repository Layout

| Path | Role |
|---|---|
| `apps/desktop` | Tauri, React, and TypeScript desktop app. |
| `core/er_optimizer_core` | Rust optimizer and optional PyO3 validation binding. |
| `data/phase1` | Committed runtime data snapshot. |
| `tools/phase1` | Extraction and data refresh tooling. |
| `tools/phase4` | Validation, benchmarking, and release packaging. |
| `docs/design` | Current design references for optimizer and release behavior. |
| `docs/release-notes` | Per-version release notes and GitHub release links. |

See [`tools/README.md`](tools/README.md) for the phase-tooling directory split,
[`docs/design/optimizer-overview.md`](docs/design/optimizer-overview.md) for the
current optimizer design reference, and [`CHANGELOG.md`](CHANGELOG.md) for the
release-notes index.

## License

Code is MIT-licensed in `LICENSE`.

Elden Ring IP belongs to FromSoftware / Bandai Namco. This is fan-made tooling and does not ship the game itself.
