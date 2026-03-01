# Tarnished's Arsenal

[![CI](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml)
[![Release Package](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/release-package.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/release-package.yml)

Stop hand-testing one build at a time.

This app brute-forces:

`weapon x affinity x AoW x upgrade x STR/DEX/INT/FAI/ARC distribution`

for your exact class + level budget, then ranks the best outcomes for your objective.

## Why It Feels Different

Typical calculator flow:

- set stats
- choose one weapon
- read one number
- repeat forever

Tarnished's Arsenal flow:

1. Set your class and current profile.
2. Lock what you care about.
3. Leave the rest open.
4. Press `Search`.
5. Get ranked best builds + upgrade comparison.

## Lock/Open Search Model

Every selector is either locked or open.

| Input | Locked | Open |
|---|---|---|
| Weapon | Only that weapon is searched | All weapons are searched |
| Affinity | Only that affinity | All valid affinities |
| AoW | Only that AoW | All valid AoWs (passive effects) |
| Upgrade | Exact level when `Lock Upgrade Exact` is checked | `+0..+N` |
| Combat Stats | Exact if using `Use As Locks` + `Lock Stats Too (Exact)` | Optimized within level budget and floors |

## Controls Cheat Sheet

### Character panel

- `Class`: sets hard minimum for every stat.
- `Character Level (Derived)`: locked; computed from current 8 stats.
- `Current` stats:
  - `VIG/MND/END`: fixed
  - `STR/DEX/INT/FAI/ARC`: used to derive level budget context
- `Min Floor`:
  - applies to `STR/DEX/INT/FAI/ARC`
  - forces optimizer to keep those minimums

### Options panel

- `Upgrade`: max upgrade considered
- `Top K`: number of top results returned
- `Objective`:
  - `Max AR`
  - `Max AR + Bleed`
- `Somber Filter`:
  - `All`
  - `Standard Only`
  - `Somber Only`
- `Lock Upgrade Exact`: evaluate only exactly `+Upgrade`
- `Two Handing`: 1.5x effective STR (cap 99) for both requirements and AR math
- `Lock Stats Too (Exact)`: only active when you apply `Use As Locks` from a result row

### Comparison tab

You can compare a second weapon path live:

- Compare Type
- Compare Weapon
- Compare Affinity
- Compare AoW (`<Match Selected>` supported)

Each row is optimized independently for the current objective at your level budget.

## What Happens Under the Hood

- Rust core does exhaustive constrained search.
- Search space estimator reports combinations before run.
- Async worker updates progress (`checked/total`, eligible count, best score, elapsed time).
- Requirement failures are highlighted in red when selected weapon requirements are not met.

## Tech Stack

- `core/er_optimizer_core`: Rust optimizer + PyO3 API
- `ui/desktop/app.py`: PyQt6 desktop UI
- `data/phase1/*.csv`: runtime data snapshot
- `tools/phase1/phase1_dump.py`: optional data re-dump pipeline
- `tools/phase4/*`: validation, smoke tests, packaging

## Local Setup (Windows)

Requirements:

- Python 3.10+
- Rust stable

```powershell
python -m pip install --upgrade pip
python -m pip install PyQt6 maturin
python -m maturin build --manifest-path core/er_optimizer_core/Cargo.toml --features python
python -m pip install --force-reinstall core/er_optimizer_core/target/wheels/er_optimizer_core-*.whl
python ui/desktop/app.py
```

## Validation

```powershell
cargo test --manifest-path core/er_optimizer_core/Cargo.toml
python tools/phase4/validate_phase4.py
python tools/phase4/smoke_ui.py
```

## Optional: Refresh Data Snapshot

You can regenerate `data/phase1` with your own `regulation.bin`:

```powershell
python tools/phase1/phase1_dump.py `
  --regulation data/raw/regulation.bin `
  --witchybnd C:\path\to\WitchyBND.exe `
  --output data/phase1
```

`tools/phase1/README.md` explains why WitchyBND is not bundled in this repo.

## Scope

Included:

- AR-focused optimization
- passive AoW status effects for objective scoring

Out of scope:

- active skill motion-value simulation
- poise/stamina modeling
- enemy resistances

---

Elden Ring IP belongs to FromSoftware / Bandai Namco. This is fan-made tooling.
