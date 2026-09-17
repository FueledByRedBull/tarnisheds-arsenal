<p align="center">
  <img src="docs/images/tarnisheds-arsenal-banner.svg" alt="Tarnished’s Arsenal" width="100%">
</p>

# Tarnished’s Arsenal

**Find your weapon. Compare your options. Plan your next levels.**

A Windows desktop build optimizer for Elden Ring. Search weapons, affinities,
Ashes of War, upgrades, and combat stats, then carry a selected build into
comparisons and progression previews.

[![CI](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml)
[![Published release](https://img.shields.io/github/v/release/FueledByRedBull/tarnisheds-arsenal?label=published)](https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/latest)

**[Download for Windows](https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/latest)**
· [What’s new in v0.14.0](docs/release-notes/v0.14.0.md)
· [Your first build](#your-first-build)
· [Documentation](#documentation)

The Rankings workspace puts each result’s combat stats and weapon scaling beside
its score, with the selected build’s full breakdown on the right.

![Rankings with weapon setups, combat stats, scaling, and the selected build’s damage breakdown](docs/images/tarnisheds-arsenal-rankings.png)

## Download

Choose an asset from the [latest published release](https://github.com/FueledByRedBull/tarnisheds-arsenal/releases/latest).
The [v0.14.0 notes](docs/release-notes/v0.14.0.md) include its versioned download
links; those links become available when that release is published.

| Choose | Best for |
| --- | --- |
| **Installer** (`.msi`) | Normal installation, with WebView2 setup if needed |
| **Portable app** (`.exe`) | Run directly with WebView2 already installed |
| **Archive** (`.zip`) | Keep or transfer the compressed release folder |

Both binaries include the Vanilla **1.17** and Convergence **3.0.0.1** snapshots.
No adjacent data folder, game installation, `regulation.bin`, or source workbook is
needed. Each release includes SHA-256 checksums and a build report identifying its
source and data. The MSI’s WebView2 bootstrapper needs internet access if that
runtime is missing.

## Your first build

1. Choose **Vanilla**, your starting class, and your character stats or level.
2. Pick an objective such as **Max AR**. Set any weapon, affinity, skill, or
   upgrade constraints; open fields allow all legal choices.
3. Press **Search**, then select a row to inspect its damage, scaling, and stats.
4. Pin another result for **Compare**, trace future levels in **Paths**, or
   explore affinity crossover points in **Affinity Watch**.

Save builds to return to them later, or export rankings as CSV. The active-query
strip shows the assumptions behind the results; changed inputs mark old results
as stale. Convergence uses a different [fixed-stat workflow](#convergence-3001-beta).

## Interface

| Workspace | The question it answers |
| --- | --- |
| **Rankings** | What wins within my level, upgrade caps, stat locks, and filters? |
| **Compare** | How does another loadout compare with the selected baseline? |
| **Paths** | Which stat comes next on a no-respec path, or what wins with a respec? |
| **Affinity Watch** | When does one affinity overtake another at future levels? |

<details>
<summary><strong>Compare — see the differences against your baseline</strong></summary>

Pinned loadouts keep their weapon, affinity, and skill. Their stats and upgrades
are reoptimized for the current budget; the upgrade chart shows how each develops.

**AR / Bleed tradeoffs** keeps the selected weapon, affinity, skill, upgrade,
level budget, stat floors/locks, and handling fixed. Compute its complete frontier,
then inspect maximum AR, maximum bleed, or the most bleed within a stated AR loss.
The 1%, 3%, and 5% choices are sacrifice limits, not bleed-proc breakpoints. An
all-options table and optional plot show the same achievable allocations.
Use **Use exact allocation** to lock its equipment, upgrade, and combat stats in
Rankings; the existing Build Detail and save actions then use those exact stats.

Compare needs fresh Rankings results. If you change the search inputs, update
Rankings before comparing again.

![Compare showing aligned differences between the selected baseline and another loadout](docs/images/tarnisheds-arsenal-compare.png)

</details>

<details>
<summary><strong>Paths — follow both builds level by level</strong></summary>

No-respec paths optimize a terminal allocation, then add points greedily toward
that target. The optimum envelope instead finds the best allocation at each level
and identifies transitions that require a respec. Two lanes share one paginated
level table.

![Paths showing selected and comparison builds across future levels](docs/images/tarnisheds-arsenal-paths.png)

</details>

<details>
<summary><strong>Affinity Watch — find the crossover points</strong></summary>

Keep the weapon, skill, and upgrade fixed while each legal affinity optimizes its
combat stats at each future level. Exact stat locks are ignored; minimums remain.

![Affinity Watch showing damage curves and rankings for legal affinities](docs/images/tarnisheds-arsenal-affinity-watch.png)

</details>

## What the numbers mean

Rankings compare modeled attack rating, status buildup, or raw skill damage under
your chosen constraints. **Enemy defenses and status-proc damage are not modeled.**
Status values describe buildup, not the damage from triggering a proc. Some skill
interactions remain unsupported; warnings in Build Detail identify those limits.

Results use exact arithmetic for ranking and rounded numbers for display. This
avoids intermediate rounding deciding close winners; it does not certify every
formula against the game. **Snapshot loaded** means the profile passed its data
integrity checks, not that every mechanic is modeled.

See the [model reference](docs/model-reference.md) for supported objectives, skill
effects, profile rules, and known differences from other calculators.

### Convergence 3.0.0.1 beta

**Convergence is experimental.** Enter exact **Custom stats**; their total is not a
Rune Level. The profile supports melee weapon AR, fixed status, and upgrades through
`+15`. AoW damage, ammunition AR, and class-dependent workflows (Compare, Paths,
Affinity Watch, and class optimization) are unavailable.

Its final AR mechanics and customization legality still need independent
verification. Vanilla rules and missing damage tables are never substituted into
the mod profile. See [Convergence coverage](docs/model-reference.md#convergence).

## Documentation

Choose a guide for the task at hand:

| I want to… | Read |
| --- | --- |
| See what changed in **v0.14.0** | [Release notes](docs/release-notes/v0.14.0.md) · [All versions](docs/release-notes/README.md) |
| Understand supported mechanics and limitations | [Model reference](docs/model-reference.md) |
| Find components and trace a calculation | [Optimizer overview](docs/design/optimizer-overview.md) |
| Inspect the exact ranking contract and proof | [Optimizer mathematics](docs/design/optimizer-math.md) |
| Work on jobs, caches, profiles, or saved builds | [Runtime invariants](docs/architecture/runtime-invariants.md) |
| Measure performance or native responsiveness | [Performance guide](docs/performance.md) |
| Refresh a game-data snapshot | [Extraction guide](tools/phase1/README.md) |
| Prepare and publish a release | [Release guide](docs/releasing.md) |

## For contributors

[Report a bug](https://github.com/FueledByRedBull/tarnisheds-arsenal/issues) with the
app version, game profile, reproduction steps, and expected versus actual results.
Use [private vulnerability reporting](SECURITY.md) for security issues.

Submit focused pull requests against `main`, explain the problem and checks run,
and add a regression for behavior changes. Keep raw game files, credentials,
build output, and local audit reports out of commits.

<details>
<summary><strong>Build and validate locally</strong></summary>

Use the repository’s pinned toolchains: **Rust 1.97.0**, **Node.js 22.23.1**, and
**Python 3.12.10**. Windows desktop development also needs the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#windows), including
Microsoft C++ Build Tools and WebView2. Rust’s `clippy` and `rustfmt` components
are declared in `rust-toolchain.toml`.

From the repository root, start the native desktop:

```powershell
cd apps/desktop
npm ci
npm run tauri dev
```

Tauri starts Vite automatically. `npm run dev` is the frontend-only alternative
and uses preview data in the browser.

Run core, backend, and data checks from the **repository root**:

```powershell
python -m pip install -r requirements-validation.txt
cargo test --locked --manifest-path core/er_optimizer_core/Cargo.toml
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
python tools/phase4/validate_phase4.py
```

Run frontend checks from **`apps/desktop`**:

```powershell
npm test
npm run build
npx playwright install chromium
npm run test:e2e
```

The full checks and setup live in [CI](.github/workflows/ci.yml). The core is in
`core/er_optimizer_core`; the desktop is in `apps/desktop`. Historical phase names
remain for extraction (`tools/phase1`) and validation/packaging (`tools/phase4`).

</details>

## License

Code is available under the [MIT License](LICENSE). Fan-made tooling, unaffiliated
with FromSoftware or Bandai Namco Entertainment; it does not distribute Elden Ring.
