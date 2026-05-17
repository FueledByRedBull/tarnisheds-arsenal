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

- `Tarnished's Arsenal_<version>_x64_en-US.msi` is the normal installer.
- `tarnisheds-arsenal-desktop.exe` is the portable executable.
- `TarnishedsArsenal_<version>.zip` bundles both binaries, runtime data, the README, and the license.

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
- `Max AR + Bleed`
- `AoW First Hit (PvE)`
- `AoW Full Sequence (PvE)`

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
- workbook-backed AoW buff data
- paired and no-two-hand-bonus behavior
- AoW attack rows for PvE damage objectives
- Shadow of the Erdtree Scadutree blessing attack scaling

Current boundaries:

- enemy defense, negation, resistance growth, stamina, poise, and proc explosion damage are not modeled
- status buildup is surfaced for bleed, frost, poison, and scarlet rot
- temporary buff stacking is not yet modeled as a universal layer

## Development

Requirements:

- Rust stable
- Node.js / npm
- Python 3.12 for validation and data tooling

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
cd apps/desktop
npm run build
```

Build a Windows release package:

```powershell
python tools/phase4/package_release.py
```

The release helper runs core and Tauri tests, builds and installs the release-profile local validation binding, runs data validation, installs frontend dependencies, builds the Tauri app, and writes `dist/TarnishedsArsenal_<version>`.

## Refresh Data

Regenerate the base runtime CSVs from a local `regulation.bin`:

```powershell
python tools/phase1/phase1_dump.py `
  --regulation data/raw/regulation.bin `
  --witchybnd C:\path\to\WitchyBND.exe `
  --output data/phase1
```

Refresh workbook-derived AoW attack data by placing this workbook in `data/phase1`:

```text
ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx
```

Then run:

```powershell
python tools/phase1/extract_motion_workbook.py
python tools/phase1/derive_phase1_raw_extras.py --workdir data/_work_phase1_reparse/regulation-bin --phase1 data/phase1 --output data/phase1
python tools/phase1/derive_phase1_extras.py --input data/phase1 --output data/phase1
```

## Repository Layout

| Path | Role |
|---|---|
| `apps/desktop` | Tauri, React, and TypeScript desktop app. |
| `core/er_optimizer_core` | Rust optimizer and optional PyO3 validation binding. |
| `data/phase1` | Committed runtime data snapshot. |
| `tools/phase1` | Extraction and data refresh tooling. |
| `tools/phase4` | Validation, benchmarking, and release packaging. |

## License

Code is MIT-licensed in `LICENSE`.

Elden Ring IP belongs to FromSoftware / Bandai Namco. This is fan-made tooling and does not ship the game itself.
