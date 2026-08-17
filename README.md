<p align="center">
  <img src="docs/images/tarnisheds-arsenal-banner.svg" alt="Tarnished’s Arsenal" width="100%">
</p>

# Tarnished’s Arsenal

[![CI](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml)
[![Release Package](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/release-package.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/release-package.yml)
[![Latest Release](https://img.shields.io/github/v/release/FueledByRedBull/tarnisheds-arsenal?label=release)](https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/latest)

**Tarnished’s Arsenal** is a Windows desktop optimizer for Elden Ring builds. It
performs an exact Rust search across weapons, affinities, Ashes of War, upgrade
levels, and relevant stat distributions, then carries one versioned build session
through rankings, comparisons, stat paths, affinity breakpoints, exports, and presets.

Choose a class, level, objective, and constraints; inspect or lock a ranked build;
then answer follow-up questions without rebuilding the assumptions.

> Fan-made tooling, unaffiliated with FromSoftware or Bandai Namco Entertainment;
> it does not include or distribute Elden Ring.

## Download

Get the latest Windows build from [Releases](https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/latest).

- `TarnishedsArsenal_<version>_x64_en-US.msi` is the standard installer.
- `TarnishedsArsenal_<version>_portable.exe` is the standalone app.
- `TarnishedsArsenal_<version>.zip` bundles the complete release folder for archival
  or offline transfer.

The app uses Microsoft Edge WebView2. The MSI downloads its bootstrapper if needed;
the portable executable expects WebView2 to already be installed. The installer and
portable executable include checksummed Vanilla 1.16.1 and Convergence 3.0.0.1
profiles. Each release publishes SHA-256 checksums plus a machine-readable build
report. The standalone app
needs no adjacent data directory, `regulation.bin`, workbook, or FMG files.

## What It Answers

- What is best if `ARC` must stay above 40?
- When does `Occult` overtake `Blood`?
- Which stat path reaches the selected target most efficiently?
- Which affinity wins when weapon, Ash of War, class, and level budget stay fixed?
- What happens when the same build is evaluated at every upgrade level?
- In Vanilla, how does Shadow Realm Scadutree scaling change outgoing damage?

The five supported objectives are:

- `Max AR`
- `Max Physical AR`
- `Bleed, then AR`
- `AoW First Hit (PvE)`
- `AoW Full Sequence (PvE)`

## Interface

Select any podium card or ranking row to make it the active build.<br>
Build Detail shows combat stats, AR split, route damage, status, stamina, and warnings.<br>
Compare, Paths, and Affinity Watch reuse that active session.<br>
The active-query strip keeps objective, level, reinforcement, handedness, profile, and constraints visible.<br>
CSV export offers 25, 100, 500, and 2,000-row limits with model and profile provenance.

![Rankings workspace showing Tarnished’s Arsenal build comparisons, active constraints, and result details](docs/images/tarnisheds-arsenal-rankings.jpg)

## Model Details

### Search model

The optimizer searches legal combinations while each major constraint can be locked
or opened:

| Input | Locked | Open |
|---|---|---|
| Weapon type | One family | All families |
| Weapon | One weapon | All weapons |
| Affinity | One affinity | All legal affinities |
| Ash of War | One skill | All legal skills |
| Upgrade | Exact `+N` | Full `+0..+N` range |
| Combat stats | Exact locked result | Optimize within the session budget |

Max AR and Max Physical AR use a bounded dynamic program over relevant combat stats.
Other objectives enumerate only stats that can affect their metric. Weapon requirements
become minimum floors first, and inactive stats are filled deterministically when the
level budget would otherwise be unused.

AoW damage objectives evaluate one legal route at a time. Inspector and Compare expose
the selected route’s ordered actions and hits, damage, status buildup, physical-hit
attribute, buff timing, stamina, and modeling warnings.

### Data and profiles

Runtime snapshots are generated from local game and mod data, FMG names, and workbook
extraction. They cover weapon affinities, reinforcement, scaling graphs, AoW
compatibility and routes, innate and upgrade-dependent passives, skill attack data,
effect graphs, paired and two-hand behavior, and Shadow of the Erdtree attack scaling.

Each profile is independent, versioned, and checksummed. Missing, modified, mixed, or
unlisted files fail closed instead of loading data from another profile.

### Convergence 3.0.0.1 beta

The Convergence profile is beta. Its extracted weapon model is regression-tested and
differentially validated against a version-bound reference and currently supports melee
weapon AR, status buildup, passives, affinities, and Ash of War compatibility.
Ammunition weapon AR is excluded until arrow, bolt, and projectile damage are modeled;
Ash of War hit and route damage is disabled until mod-specific motion and sequence data
is mapped. Vanilla data is never substituted. Broader in-game verification remains in
progress, while the application itself remains a stable release.

Its profile rules use one weapon reinforcement path from `+0` through `+15`, remove
Scadutree Blessing scaling, keep weapon status fixed across stats and upgrades, expose
extended `S+`/`S++` grades, and apply every declared nonzero attribute scaling to every
nonzero damage component for row-0 attack-element weapons.

### Current model boundaries

- Enemy defense, negation, resistance growth, proc explosion damage, and poise/stance
  damage are not modeled.
- Route status details cover bleed, frost, poison, scarlet rot, sleep, madness, and
  death buildup separately from proc damage.
- Route stamina is reported but is not an optimization objective.
- Temporary buff stacking is not modeled as a universal layer.

## For Contributors

### Requirements

- Rust 1.97.0 with `clippy` and `rustfmt` (pinned by `rust-toolchain.toml`)
- Node.js 22.23.1 with npm (pinned by `.node-version`)
- Python 3.12.10 for validation and data tooling (pinned by `.python-version`)

Install Python validation helpers for data validation or release packaging:

```powershell
python -m pip install -r requirements-validation.txt
```

### Run and validate

```powershell
cd apps/desktop
npm ci
npm run tauri dev
```

`npm run tauri dev` is the preferred desktop command: Tauri’s `beforeDevCommand`
starts Vite. `npm run dev` is the frontend-only alternative.

```powershell
cargo test --manifest-path core/er_optimizer_core/Cargo.toml
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
python tools/phase4/validate_phase4.py
cd apps/desktop
npm test
npm run build
npm run test:e2e
```

### Release and data

Build a Windows release package with:

```powershell
python tools/phase4/package_release.py
```

The helper produces the MSI, portable executable, checksums, build report, and ZIP;
see the [release guide](docs/releasing.md) for the tag-driven process. See the [phase 1
tooling guide](tools/phase1/README.md) to refresh versioned data snapshots.

More references: [tooling overview](tools/README.md), [optimizer design](docs/design/optimizer-overview.md),
[runtime invariants](docs/architecture/runtime-invariants.md), [performance guide](docs/performance.md),
[release notes](docs/release-notes/README.md), and [CHANGELOG](CHANGELOG.md).

## License

Code is available under the [MIT License](LICENSE).
