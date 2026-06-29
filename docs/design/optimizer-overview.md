# Optimizer Design Overview

This document is the current-state design reference for Tarnished's Arsenal.
It intentionally avoids historical planning notes and tracks the implementation
shape that exists in the repository today.

## Product Shape

Tarnished's Arsenal is a Windows Tauri desktop app backed by one Rust optimizer
core. The user works from one build session and carries that session through
rankings, comparisons, stat paths, and affinity breakpoints.

The primary search space is:

```text
weapon x affinity x Ash of War x upgrade x relevant stat distribution
```

The public request/response contracts live in the Tauri DTO layer and are shared
with the frontend tests. The Python binding is an optional validation and
benchmarking path for local tooling and CI.

## Data Model

Runtime data is committed under `data/phase1` as CSV snapshots generated from
local game data and workbook extraction. The shipped snapshot includes weapon
rows, reinforce rows, calc-correct graphs, Ash of War compatibility, passive
status data, native skill attack data, buff data, and Scadutree scaling inputs.

Data refresh tooling lives in `tools/phase1`. Validation, benchmarking, and
release packaging helpers live in `tools/phase4`.

## Search Behavior

The optimizer accepts locked or open constraints for weapon type, weapon,
affinity, Ash of War, upgrade caps, combat stat floors, exact combat stat locks,
two-handing, Scadutree scaling, objective, and result count.

Standard and Somber upgrade caps are tracked separately in the app-facing
contract. Exact-upgrade searches use the cap that matches each weapon class, so
Somber-only exact `+10` searches evaluate Somber weapons at `+10` rather than
being blocked by the Standard `+25` scale.

When a specific weapon is selected, rankings may return multiple loadouts for
that weapon. When weapon is open, rankings return at most one row per weapon:
the best affinity, Ash of War, upgrade, and stat distribution for the selected
metric.

## Optimization Core

The Rust core narrows stat enumeration per weapon, affinity, Ash of War, and
objective. It only varies combat stats that can affect the selected metric,
folds requirements into the minimum stat floors, and fills inactive stats only
when the session level budget cannot otherwise be consumed.

Medium and broad searches use Rayon when the estimated combination count and
work-unit count justify parallel execution. Parallel work is split by individual
Ash of War choices for better load balance. Candidate ranking uses a lightweight
score-only buffer first, then materializes full result rows only after local
top-K pruning.

Final ordering and de-duplication still use the full result comparison logic so
tie handling, same-loadout replacement, cancellation, and progress reporting
stay deterministic.

## Release Flow

CI validates Rust, DTO, data, Python binding, frontend build, and e2e contract
coverage. Tag pushes matching `v<app version>` run the release workflow, which
builds the Tauri package, prepares MSI/portable/zip assets, and publishes the
GitHub release with notes from `docs/release-notes`.
