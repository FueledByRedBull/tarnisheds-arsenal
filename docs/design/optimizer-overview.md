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
with the frontend tests. Validation and benchmarking call the Rust core directly
through tests and small release-mode examples.

## Desktop Interface

The desktop shell uses a three-region composition: continuously visible session
controls, the active workspace, and an always-visible Build Detail panel. The
Rankings workspace has one selection contract for podium cards, mouse-activated
rows, and keyboard-activated rows. Selecting any rank updates Build Detail;
locking is the only separate row action because it mutates search inputs and reruns
the optimizer.

The default ranking grid contains rank, weapon, affinity/Ash setup, upgrade,
scaling, raw AR/status, raw skill damage, objective score, and Lock. Full combat
stats, AR split, route actions/hits, status, stamina, buff timing, and warnings stay
in Build Detail without a modal. A persistent active-query strip exposes the major
assumptions before execution, including whether reinforcement levels are exact or
searched from zero to the configured caps. Scaling uses a responsive five-token
STR/DEX/INT/FAI/ARC grid. Status uses a wrapping seven-token grid for bleed, frost,
poison, scarlet rot, sleep, madness, and death blight; zeroes remain visible so a
missing buildup cannot be mistaken for omitted data.

The visual system is intentionally lightweight: CSS perspective, short
state-driven transitions, and a 19 KB low-contrast WebP texture provide depth
without WebGL or a runtime animation library. `prefers-reduced-motion` collapses
decorative animation to effectively zero duration. Responsive states prioritize
the table and remove the redundant podium at narrower supported window widths;
the page itself does not scroll horizontally.

## Data Model

Runtime data is committed as separate manifest-bound Vanilla (`data/phase1`) and
Convergence (`data/profiles/convergence`) snapshots generated from local game/mod
data. Every runtime file is size/hash checked and each profile loads all-or-nothing.
The manifest exposes profile-specific capabilities: Vanilla includes the full AoW
attack/route model, while Convergence currently exposes melee weapon AR, affinities,
compatibility, and passive status data but explicitly excludes ammunition weapons
and disables unsupported AoW hit/route damage. Profile rules also define reinforcement caps, whether Standard
and Somber paths are separate, Scadutree availability, attack-element fallback
semantics, status-scaling behavior, and extended scaling grades. Runtime commands,
jobs, caches, presets, and exports carry an explicit profile identity and cannot
mix snapshots. Convergence `levelSyncCorrectId` values are not applied as normal
player-panel AR multipliers: the version-bound calculator model omits them, and the
verified +13 Galvanic formula uses only reinforcement, weapon scaling, correction
routing, and character stats.

Data refresh tooling lives in `tools/phase1`. Validation, benchmarking, and
release packaging helpers live in `tools/phase4`.

## Search Behavior

The optimizer accepts locked or open constraints for weapon type, weapon,
affinity, Ash of War, upgrade caps, combat stat floors, exact combat stat locks,
two-handing, profile-supported world scaling, objective, and result count.

Standard and Somber upgrade caps are tracked separately in the app-facing
contract. Exact-upgrade searches use the cap that matches each weapon class, so
Somber-only exact `+10` searches evaluate Somber weapons at `+10` rather than
being blocked by the Standard `+25` scale.

When a specific weapon is selected, rankings may return multiple loadouts for
that weapon. When weapon is open, rankings return at most one row per weapon:
the best affinity, Ash of War, upgrade, and stat distribution for the selected
metric.

## Optimization Core

The Rust core narrows stat work per weapon, affinity, Ash of War, and objective.
Max AR and Max Physical AR use an exact dynamic program over relevant stats, then
evaluate constant AoW buffs once per solved weapon/upgrade allocation. Other
objectives enumerate only combat stats that can affect their selected metric.
Requirements are folded into minimum floors, and inactive stats are filled only
when the session level budget cannot otherwise be consumed.

Medium and broad searches use Rayon when the estimated combination count and
work-unit count justify parallel execution. Parallel work is split by individual
Ash of War choices for better load balance. Candidate ranking uses a lightweight
score-only buffer first, then materializes full result rows only after local
top-K pruning.

Final ordering and de-duplication still use the full result comparison logic so
tie handling, same-loadout replacement, cancellation, and progress reporting
stay deterministic.

AoW damage materialization evaluates ordered legal routes. Added base attack,
fixed and motion components, status motion values, weapon-buff timing, action-level
stamina, and adaptive Standard/Strike/Slash/Pierce attributes are resolved per hit.
Conditional replacement effects remain explicit warnings rather than guessed
damage/status.

Desktop jobs use exact job IDs plus request generations/signatures. Polling is
single-flight with adaptive 200-1000 ms delay, cancellation reaches search
preparation/enumeration/nested analyses, and shared caches evict rejected or fully
abandoned in-flight work.

## Release Flow

CI validates Rust, DTO, both data profiles, frontend build, and e2e contract
coverage. Tag pushes matching `v<app version>` run the release workflow, which
builds the Tauri package and publishes an MSI, self-contained standalone
executable, convenience ZIP, SHA-256 checksums, and build provenance. Manual
workflow runs build artifacts but never publish a GitHub release. Release notes
come from `docs/release-notes`.
