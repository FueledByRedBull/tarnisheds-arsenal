# Tarnished's Arsenal

[![CI](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/ci.yml)
[![Release Package](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/release-package.yml/badge.svg)](https://github.com/FueledByRedBull/tarnisheds-arsenal/actions/workflows/release-package.yml)

Stop checking one weapon at a time.

`Tarnished's Arsenal` is a session-driven Elden Ring desktop optimizer. You give it a real class, real stat budget, real floors, real weapon constraints, and it brute-forces the search space across:

`weapon x affinity x AoW x upgrade x stat distribution`

Then it keeps going. Rankings feed Compare. Compare feeds Paths. Paths and Affinity Watch stay tied to the same active build session instead of behaving like separate mini-tools.

## Why This Exists

Most calculators are good at answering:

- `What does this one weapon do with these exact stats?`

This app is built to answer:

- `What is actually best if I keep ARC above 40?`
- `When does Occult overtake Blood?`
- `What does my upgrade spread look like if I lock this build?`
- `What is the best Current + N stat route into the target build?`
- `Which affinity leads if I keep the weapon and AoW fixed?`

## The Model

The app runs around one canonical session:

- `Build Session`
  - class
  - current 8 stats
  - derived level
  - min floors
  - two-handing
- `Search Scope`
  - weapon type
  - weapon
  - affinity
  - AoW
  - somber filter
  - upgrade policy
  - top-k
- `Objective`
  - `Max AR`
  - `Max AR + Bleed`
  - `AoW First Hit (PvE)`
  - `AoW Full Sequence (PvE)`
- `Locked Combat Stats`
  - optional exact `STR/DEX/INT/FAI/ARC` lock taken from a solved result
- `Analysis State`
  - selected result
  - compare target
  - horizon
  - active workspace

That matters because every workspace is looking at the same truth.

## Workspaces

### `RANKINGS`

- brute-force ranked search results
- scaling shown per result at the actual upgrade level
- `Use As Locks` promotes the selected result into exact combat-stat locks
- result cards are summaries of the current ranking state, not a separate logic path

### `COMPARE`

- selected build vs explicit rival or derived rival lines
- each row is re-optimized under the same class, budget, floors, objective, and handing mode
- upgrade spread locks that row's solved combat stats and evaluates the chosen metric across upgrade levels

### `PATHS`

- embedded workspace, not just a pop-up
- uses the selected build and compare target from the current session
- solves the exact `Current + N` target state for each lane
- then routes the stat points into that target level by level
- shows chart + per-lane step tables

### `AFFINITY WATCH`

- embedded workspace, not just a pop-up
- keeps weapon, AoW, upgrade, class, budget, and objective fixed
- varies affinity only
- shows which legal affinity leads from `Current` to `Current + N`
- includes crossover breakpoints and final stat states

## Lock / Open Search Rules

Every constraint can be treated as either locked or open.

| Input | Locked | Open |
|---|---|---|
| Weapon Type | Search only that weapon family | Search all weapon families |
| Weapon | Search only that weapon | Search all weapons |
| Affinity | Search only that affinity | Search all legal affinities |
| AoW | Search only that AoW | Search all legal AoWs |
| Upgrade | Exact level with `Lock Upgrade Exact` | Full `+0..+N` range |
| Combat Stats | Exact with `Use As Locks` + `Use Locked Result Stats` | Optimized inside the session budget |

## Controls That Matter

### `Build`

- `Starting Class`
  - sets hard minimums
- `Derived Level`
  - computed from the visible 8 stats
- `VIG / MND / END`
  - fixed inputs
- `STR / DEX / INT / FAI / ARC`
  - establish your current budget context
- `Min Floor`
  - lower bound for optimizer-controlled combat stats

### `Constraints`

- `Weapon Type`, `Weapon`, `Affinity`, `AoW`
  - lock a lane or leave it open
- `Somber Filter`
  - `All`, `Standard Only`, `Somber Only`
- `Max Upgrade`
  - max evaluated upgrade
- `Top Results`
  - number of returned top rows

### `Search`

- `Objective`
  - ranking metric for search and downstream analysis
- `Lock Upgrade Exact`
  - force exactly `+N`
- `Two Handing`
  - 1.5x effective STR, capped at 99, for requirements and scaling behavior where applicable
  - paired weapons and paired uniques that do not receive the generic two-hand STR bonus are excluded from that boost
- `Use Locked Result Stats`
  - reuses the exact combat stats captured via `Use As Locks`

## Objectives

### `Max AR`

Ranks by total AR.

### `Max AR + Bleed`

Ranks by:

- total AR
- plus total bleed buildup after upgrade and stat scaling

The app also computes frost, poison, and scarlet rot buildup, but this objective still ranks specifically on AR plus bleed.

### `AoW First Hit (PvE)`

Ranks by the first damaging full-FP hit row for the selected AoW.

### `AoW Full Sequence (PvE)`

Ranks by the total damaging full-FP sequence for the selected AoW.

## Data and Accuracy Notes

This project is not using wiki guesses. The runtime snapshot is derived from local game data and workbook extraction.

Current runtime data includes:

- weapon rows by affinity
- reinforce data
- expanded calc-correct graphs
- exact AoW compatibility rows
- innate weapon passives
- weapon rules for paired / no-two-hand-bonus behavior
- AoW attack-data extraction for PvE damage objectives

Important boundaries:

- this is still an optimizer, not a full enemy simulator
- enemy defense, negation, resistance growth, proc explosion damage, poise, and stamina are not part of the current scoring model
- unique somber weapon-skill damage is not yet treated as a complete separate universal layer outside the generic AoW pipeline
- status buildup is split and surfaced for bleed, frost, poison, and scarlet rot, but the per-effect `isUseStatusAilmentAtkPowerCorrect` flag is not yet modeled as a separate runtime gate

## Local Setup

Requirements:

- Python 3.10+
- Rust stable

```powershell
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
python -m maturin build --manifest-path core/er_optimizer_core/Cargo.toml --features python
$wheel = Get-ChildItem core/er_optimizer_core/target/wheels/er_optimizer_core-*.whl | Sort-Object LastWriteTime | Select-Object -Last 1
python -m pip install --force-reinstall $wheel.FullName
python ui/desktop/app.py
```

## Validation

```powershell
cargo test --manifest-path core/er_optimizer_core/Cargo.toml
python tools/phase4/validate_phase4.py
python tools/phase4/smoke_ui.py
```

Those checks cover:

- optimizer behavior
- data integrity
- session-driven UI flow
- selection preservation
- path and affinity-watch stability

## Build A Windows Release

For a distributable bundle:

```powershell
python tools/phase4/package_release.py
```

For a standalone Windows app folder with an `.exe`:

```powershell
python -m pip install pyinstaller
python -m PyInstaller --noconfirm --clean --windowed --name "TarnishedsArsenal" --collect-all er_optimizer_core --add-data "data\phase1;data\phase1" ui\desktop\app.py
```

## Refresh The Data Snapshot

If you want to regenerate the base runtime CSVs from your own `regulation.bin`:

```powershell
python tools/phase1/phase1_dump.py `
  --regulation data/raw/regulation.bin `
  --witchybnd C:\path\to\WitchyBND.exe `
  --output data/phase1
```

If you also want workbook-derived AoW attack data refreshed, place:

- `data/phase1/ER - Motion Values and Attack Data (App Ver. 1.16.1).xlsx`

then run:

```powershell
python tools/phase1/extract_motion_workbook.py
python tools/phase1/derive_phase1_raw_extras.py
python tools/phase1/derive_weapon_rules.py
```

## Repo Layout

- `core/er_optimizer_core`
  - Rust optimizer and PyO3 bridge
- `ui/desktop`
  - PyQt6 desktop app, canonical session models, desktop services
- `data/phase1`
  - committed runtime snapshot used by the app
- `tools/phase1`
  - dump and extraction scripts
- `tools/phase4`
  - validation, smoke tests, release packaging

## License / IP

Code is MIT-licensed in `LICENSE`.

Elden Ring IP belongs to FromSoftware / Bandai Namco. This repo is fan-made tooling and does not ship the game itself.
