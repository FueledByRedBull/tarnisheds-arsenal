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

Build Detail reports actual PvE stance/poise damage for R1, R2, charged R2, jumping
R1, and jumping R2 attacks. When a selected AoW route is mapped, it also reports
every hit and the full-route poise total using the workbook's weapon base poise and
AoW poise multiplier. These values do not use the selected build's attack rating.

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
Max AR, Max Physical AR, Bleed then AR, AoW First Hit, and AoW Full Sequence use one
lexicographic dynamic program over relevant stats. Legal AoW routes are compiled
once and optimized independently with compact scalar evaluators.
Requirements are folded into minimum floors, and inactive stats are filled only
through one canonical completion for each feasible active-stat spend. This preserves
the final stat-vector tie-break without enumerating equivalent inactive distributions.

[`optimizer-math.md`](optimizer-math.md) states the model formally: the point budget,
the attack-rating formula, the conditions that make the recurrence exact in exact
arithmetic, its cost bounds, and the implementation's floating-point limitation.

`RelevantStatSearch` owns the active mask, bounded stat domain, logical candidate count,
and canonical enumeration retained for the exhaustive oracle/fallback. The DP compares
the full feasible active-spend interval; searches with the same bounds and mask share
the cached distribution count. Focused regressions cover interior optima, canonical
inactive fill, arbitrary decreasing curves, and every objective family. This reduced
exhaustive path shares the active mask; matching it alone cannot validate relevance.

The dynamic program's accumulated `f32` totals select a stat allocation only. Terminal
allocations are recomputed directly before comparison, ranking, or display; accumulated
totals never become user-visible results. Reevaluation cannot recover a preferred
allocation discarded at an earlier state. States retain objective score, total AR, AoW
full sequence, AoW first hit, bleed, and the stat vector under candidate ranking order.

For Max AR, Max Physical AR, and Bleed then AR, each work unit builds one primary
plan per upgrade and identical primary-effect signature. The signature includes
attack buffs, bleed additions/correction, and status-driven bleed rounding. Plans
cache scalar weapon contributions and every DP predecessor tied on objective score
and total AR. Route-specific work visits those tied predecessors and compares the
remaining skill, bleed, and stat-vector fields. All legal Ash choices remain visible,
including unbuffed skills that can win secondary ties. The cache is request-local
and released after each upgrade; it does not retain cross-request data.

`shared_primary_frontiers_match_independent_dp_including_all_ties` compares shared
and independent DP allocations across both profiles, buffs/routes, upgrades, and
zero/small/larger budgets. This preserves the existing DP's floating-point semantics;
it does not claim to remove the numerical limitation documented below.

Progress counts the logical candidate domain covered, not DP transitions or individual
allocations evaluated. For active capacities $c_i$, that count is

$$N=\sum_{p=p_{\min}}^{p_{\max}}[z^p]\prod_{i\in A}(1+z+\cdots+z^{c_i}).$$

Medium and broad searches use Rayon when the estimated combination count and
work-unit count justify parallel execution. Damage-objective work is split by individual
Ash choices. AR and Bleed work uses bounded chunks of eight Ashes so primary-score
plans can be shared without serializing an entire weapon search. Candidate ranking uses a lightweight
score-only buffer first, then materializes full result rows only after local
top-K pruning.

Final ordering and de-duplication still use the full result comparison logic so
tie handling, same-loadout replacement, cancellation, and progress reporting
stay deterministic.

Within one loadout, tied routes compare the numeric objective key, combat stats,
route priority, then route ID. Public materialization uses a weaker display comparator;
equal rows retain their existing deterministic scored-candidate order.

AoW search evaluates compiled scalar routes without constructing display objects for
discarded allocations. Final AoW materialization evaluates the retained ordered route. Added base attack,
fixed and motion components, status motion values, weapon-buff timing, action-level
stamina, and adaptive Standard/Strike/Slash/Pierce attributes are resolved per hit.
Conditional replacement effects remain explicit warnings rather than guessed
damage/status.

Desktop jobs use exact job IDs plus request generations/signatures. Polling is
single-flight with adaptive 200-1000 ms delay, cancellation reaches search
preparation/enumeration/nested analyses, and shared caches evict rejected or fully
abandoned in-flight work.

## Numerical evidence and decision

The follow-up in `optimizer/tests.rs` separates three questions:

- **Relevance:** an independent recursive enumerator searches all five bounded stats
  without `RelevantStatSearch::visit`, its mask, count, or inactive-fill helper. Small
  budgets cover both profiles, all supported objectives, buffs, branching skills,
  locks/floors, paired weapons, bows, and Strength near the effective-stat cap. Its
  direct numeric and stat-vector winners match reduced exhaustive search. Omitted
  stats are also swept through their bounds to check numeric invariance. Bounds and
  metric formulas still come from production code; this is an independent enumeration,
  not an independent game model or proof over every loadout.
- **Arithmetic:** sampled DP winners match all numeric fields exactly, without an
  epsilon comparator. A real Convergence Mystic Uchigatana +15 case with starting
  STR/DEX/INT/FAI/ARC `67/40/35/35/35`, two-handing, and three free points returns
  `67/40/35/36/37`; exhaustive evaluation prefers `67/40/35/35/38`. Both have the same
  numeric key (AR approximately 791.3289, bleed 50, no AoW damage). The test records
  this known limitation, not successful canonical tie-breaking. A separate one-ULP
  example shows a common rounded completion collapsing a strict score difference
  into a tie whose preferred stat vector was already discarded.
- **Routes:** scalar and materialized first/full metrics are checked together by route
  ID. Per-hit reconstruction from single-stat deltas covers branching finishers,
  repeat hits, fixed/projectile damage, low/max upgrades, both handling modes,
  mixed allocations, and stat sweeps through 99. The first positive hit's identity
  is checked throughout. A separate synthetic zero-damage activation followed by a
  buffed hit exercises activation timing without claiming that timing for the real
  skill. All loaded bases, curves, scaling/override coefficients, and buff powers
  are checked nonnegative. Route reconstruction allows `32 * f32::EPSILON` relative
  error (absolute below magnitude 1) for different summation orders; this is a test
  tolerance, not an optimizer tie rule or universal error bound.

**Decision:** retain the current evaluator and `f32` selection in this follow-up.
The checks demonstrate a stat-vector tie discrepancy, not a numeric metric loss in
the sampled searches. They do not rule out numeric losses elsewhere. Merely promoting
already-rounded deltas to `f64` cannot recover their rounding, and higher precision
alone does not establish translation-invariant lexicographic pruning. No heuristic
epsilon tie-break, arithmetic-contract change, or release rollback is introduced.
Strict canonical tie equivalence remains a documented follow-up: first choose the
metric contract, then use exact contribution arithmetic or conservatively resolve
ambiguous states against that contract, verified by this independent oracle.

## Release Flow

CI validates Rust, DTO, both data profiles, frontend build, and e2e contract
coverage. Tag pushes matching `v<app version>` run the release workflow, which
builds the Tauri package and publishes an MSI, self-contained standalone
executable, convenience ZIP, SHA-256 checksums, and build provenance. Manual
workflow runs build artifacts but never publish a GitHub release. Release notes
come from `docs/release-notes`.
