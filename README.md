# Tarnished's Arsenal

High-speed Elden Ring build optimizer for:

`weapon x affinity x AoW x upgrade level x combat stat distribution`

Given your class and level budget, it brute-forces the valid search space in Rust and returns the best builds for the selected objective.

## What It Solves

Most calculators give one AR number for one manual setup.

This app searches the whole constraint space and answers:

- What is the best build for my current level?
- Should I stay Blood or switch Occult?
- How should I redistribute STR/DEX/INT/FAI/ARC for this weapon?
- How does each option scale across upgrades (+0..+N)?

## Core Features

- Rust optimization core (fast exhaustive search)
- Desktop UI (PyQt6) with searchable dropdowns
- Class-aware stat floors and derived character level
- Requirement highlighting (red when weapon requirements are unmet)
- Objective modes:
  - `Max AR`
  - `Max AR + Bleed`
- Somber filter:
  - `All`
  - `Standard Only`
  - `Somber Only`
- Side-by-side upgrade comparison with independent best stat distribution per row
- Async search progress (checked/total, eligible count, best score, elapsed time)

## Lock/Open Model (How Search Works)

Each selector can be either **locked** or **open**:

- Locked weapon/affinity/AoW narrows search to that exact choice.
- `<Open>` means the optimizer searches all valid options.
- Upgrade can be:
  - Range mode: `+0..+max_upgrade`
  - Exact mode: `Lock Upgrade Exact` checked
- Combat stats can be:
  - Free to optimize (from class base + min floors + level budget)
  - Exactly locked via `Use As Locks` + `Lock Stats Too (Exact)`

The optimizer always respects:

- Class minimum stats
- Input min floors (`Min STR/DEX/INT/FAI/ARC`)
- Weapon stat requirements
- Level budget

## UI Controls Reference

### Character

- `Class`: starting class baseline (sets minimum allowed stat values)
- `Character Level (Derived)`: read-only; computed from your 8 current stats
- `Current` stats:
  - `VIG/MND/END` are fixed
  - `STR/DEX/INT/FAI/ARC` are treated as current profile input for level budget
- `Min Floor`:
  - Applies only to `STR/DEX/INT/FAI/ARC`
  - Forces optimizer to keep each combat stat at or above the floor

### Weapon / Affinity

- `Weapon Type`: optional filter
- `Weapon`: lock or open
- `Affinity`: lock or open
- `AoW`: lock or open

### Options

- `Upgrade`: max upgrade considered
- `Top K`: number of top results to keep
- `Objective`: `Max AR` or `Max AR + Bleed`
- `Somber Filter`: all/standard/somber
- `Lock Upgrade Exact`: evaluate only exactly `+Upgrade`
- `Two Handing`: applies 1.5x effective STR (capped at 99) for req + AR math
- `Lock Stats Too (Exact)`: when using `Use As Locks`, lock STR/DEX/INT/FAI/ARC exactly

### Results + Comparison

- Results table shows ranked builds and a `Use As Locks` button per row.
- Upgrade Comparison tab compares:
  - Selected result
  - Explicit comparison weapon (type/weapon/affinity/AoW)
- Comparison rows are optimized independently for the active objective at your level budget, then plotted across upgrades.

## Tech Stack

- `core/er_optimizer_core`: Rust math + optimizer + PyO3 bindings
- `ui/desktop/app.py`: PyQt6 desktop app
- `data/phase1/*.csv`: pre-dumped game data snapshot
- `tools/phase1/phase1_dump.py`: dump pipeline from `regulation.bin`
- `tools/phase4/*`: validation, smoke tests, packaging helpers

## Quick Start (Windows PowerShell)

Requirements:

- Python 3.10+
- Rust toolchain (stable)

```powershell
python -m pip install --upgrade pip
python -m pip install PyQt6 maturin
python -m maturin develop --manifest-path core/er_optimizer_core/Cargo.toml --features python
python ui/desktop/app.py
```

## Validate Locally

```powershell
cargo test --manifest-path core/er_optimizer_core/Cargo.toml
python tools/phase4/validate_phase4.py
python tools/phase4/smoke_ui.py
```

## Phase 1 Data Dump (Optional)

`tools/phase1/phase1_dump.py` expects:

- Elden Ring `regulation.bin`
- A local WitchyBND install path passed via `--witchybnd`

Example:

```powershell
python tools/phase1/phase1_dump.py `
  --regulation data/raw/regulation.bin `
  --witchybnd C:\path\to\WitchyBND.exe `
  --output data/phase1
```

## Notes

- This project focuses on AR/stat optimization, not active skill motion-value simulation.
- AoW handling is passive-status focused for optimization scoring.
- Elden Ring belongs to FromSoftware / Bandai Namco. This is a fan tooling project.
