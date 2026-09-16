# Optimizer design overview

Use this page to understand how a desktop request moves through profile-bound
data, the optimizer, and release validation. It is the current-state design
reference for Tarnished's Arsenal and does not describe historical plans.

**Navigation:** [Home](../../README.md) · [Optimizer math](optimizer-math.md) ·
[Performance](../performance.md) · [Runtime invariants](../architecture/runtime-invariants.md)

**Find a topic:** [App shape](#product-shape) · [Search behavior](#search-behavior) ·
[Optimizer core](#optimization-core) · [Evidence](#numerical-evidence-and-decision) ·
[Release flow](#release-flow)

## Product shape

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

## Desktop interface

The desktop shell uses a three-region composition: continuously visible session
controls, the active workspace, and an always-visible Build Detail panel. The
Rankings workspace uses the same selection contract for mouse and keyboard
activation. Selecting a row updates Build Detail; separate row actions pin a
comparison or apply its stats as search locks.

Compact ranking rows show weapon, affinity/Ash setup, upgrade, combat stats,
weapon scaling, AR, raw skill damage, and the active objective where applicable.
Detailed damage splits, status, route actions/hits, stamina, buff timing, and
warnings live in Build Detail. The active-query strip exposes the current search
assumptions, including whether upgrades are exact or searched from zero to the caps.

Build Detail reports actual PvE stance/poise damage for R1, R2, charged R2, jumping
R1, and jumping R2 attacks. When a selected AoW route is mapped, it also reports
every hit and the full-route poise total using the workbook's weapon base poise and
AoW poise multiplier. These values do not use the selected build's attack rating.

The visual system is intentionally lightweight: CSS perspective, short
state-driven transitions, and a 19 KB low-contrast WebP texture provide depth
without WebGL or a runtime animation library. `prefers-reduced-motion` collapses
decorative animation to effectively zero duration. Responsive states prioritize
the ranking table and let it scroll within the workspace; there is no podium.
The page itself does not scroll horizontally.

Frontend state lives in one Zustand store, while each native analysis registry
holds one active job slot and workers retain an `Arc` to the selected profile.
Frontend actions invalidate related workspaces atomically. Native registries keep
job ownership until completion is observed or a finished job is replaced.

## Data model

Runtime data is committed as separate manifest-bound Vanilla (`data/phase1`) and
Convergence (`data/profiles/convergence`) snapshots generated from local game/mod
data. Every runtime file is size/hash checked and each profile loads all-or-nothing.
The manifest exposes profile-specific capabilities: Vanilla includes supported AoW
attack/route tables, while Convergence currently exposes melee weapon AR, affinities,
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

The v6 snapshot model keeps unique-weapon attacks separate from transferable Ashes,
applies weapon attack-element overwrite/influence rates, and corrects passive-status
scaling. Chilling Mist and Poisonous Mist retain separate weapon/on-hit effect
records, but their overlapping status increments are unmodeled. They carry warnings
in AR results and are excluded from skill-damage objectives.

## Search behavior

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

## Optimization core

The Rust core narrows stat work per weapon, affinity, Ash of War, and objective.
Max AR, Max Physical AR, Bleed then AR, AoW First Hit, and AoW Full Sequence use one
lexicographic dynamic program over relevant stats. Legal AoW routes are compiled
once and optimized independently with compact scalar evaluators.
Requirements are folded into minimum floors, and inactive stats are filled only
through one canonical completion for each feasible active-stat spend. This preserves
the final stat-vector tie-break without enumerating equivalent inactive distributions.

[`optimizer-math.md`](optimizer-math.md) states the model formally: the point budget,
the attack-rating formula, the conditions that make the recurrence exact in exact
arithmetic, its cost bounds, and the exact scoring contract.

`RelevantStatSearch` owns the active mask, bounded stat domain, logical candidate count,
and canonical enumeration retained for the exhaustive oracle/fallback. The DP compares
the full feasible active-spend interval; searches with the same bounds and mask share
the cached distribution count. Focused regressions cover interior optima, canonical
inactive fill, arbitrary decreasing curves, and every objective family. This reduced
exhaustive path shares the active mask; matching it alone cannot validate relevance.

The dynamic program uses exact scaled integer coefficients. A checked bound selects
`i128` when safe, otherwise `BigInt`. Terminal keys remain exact rationals across
loadouts with different scales. Floating-point fields are generated for display
after selection. Prefix probes retain only the one or two key components they compare;
the final scoring pass retains objective score, total AR, AoW full sequence, AoW first
hit, bleed, and the stat vector under candidate ranking order.

For Max AR, Max Physical AR, and Bleed then AR, each work unit builds one primary
plan per upgrade and identical primary-effect signature. The signature includes
attack buffs, bleed additions/correction, and status-driven bleed rounding. Plans
cache scalar weapon contributions and every DP predecessor tied on objective score
and total AR. Route-specific work visits those tied predecessors and compares the
remaining skill, bleed, and stat-vector fields. All legal Ash choices remain visible,
including unbuffed skills that can win secondary ties. The cache is request-local
and released after each upgrade; it does not retain cross-request data. Retained
increments use one byte each, with per-state vectors. Weapon-grouped output can
skip strictly dominated flat buffs with identical bleed effects; primary ties remain
eligible for route comparison.

When the best primary rank has one terminal spend and one retained predecessor
path, the plan can cache a unique winner after direct primary reevaluation and
inactive-stat completion. Without a skill route, a canonical primary winner is also
reused when the remaining metrics are constant or already in the primary pair.
Other ties on winning paths use route-specific DP. Both shortcuts preserve the full
numeric and stat order.

`shared_primary_frontiers_match_independent_dp_including_all_ties` compares shared
and independent DP allocations across both profiles, buffs/routes, upgrades, and
zero/small/larger budgets. These sampled comparisons check behavioral equivalence
under the exact scoring contract; the proof obligations still apply to newly added
mechanics and data dependencies.

Progress counts the logical candidate domain covered, not DP transitions or individual
allocations evaluated. For active capacities $c_i$, that count is

$$N=\sum_{p=p_{\min}}^{p_{\max}}[z^p]\prod_{i\in A}(1+z+\cdots+z^{c_i}).$$

Medium and broad searches use Rayon when the estimated combination count and
work-unit count justify parallel execution. Damage-objective work is split by individual
Ash choices. AR and Bleed work keeps each weapon/stat-bound group together so its
primary plans are reused across all compatible Ashes. Candidate ranking uses a lightweight
exact-key buffer first, then materializes full result rows only after local
top-K pruning.

Final ordering and de-duplication still use the full result comparison logic so
tie handling, same-loadout replacement, cancellation, and progress reporting
stay deterministic.

Within one loadout, tied routes compare the numeric objective key, combat stats,
route priority, then route ID. Materialized rows retain a private exact key and use
the same complete numeric and stat ordering.

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

The regression suite and external comparison runner separate four questions:

- **Relevance:** an independent recursive enumerator searches all five bounded stats
  without `RelevantStatSearch::visit`, its mask, count, or inactive-fill helper. Small
  budgets cover both profiles, all supported objectives, buffs, branching skills,
  locks/floors, paired weapons, bows, and Strength near the effective-stat cap. Its
  direct numeric and stat-vector winners match reduced exhaustive search. Omitted
  stats are also swept through their bounds to check numeric invariance. Bounds and
  metric formulas still come from production code; this is an independent enumeration,
  not an independent game model or proof over every loadout.
- **Arithmetic:** DP is compared with exhaustive evaluation using exact numeric keys
  and canonical stat vectors. The former Convergence Mystic Uchigatana counterexample
  and strict-difference/completion case are regressions for exact pruning. Integer
  backend checks cover both the `i128` path and `BigInt` fallback. Agreement is under
  the new numerical contract, not a promise to retain former rounded winners.
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
- **External reference:** seeded Vanilla weapon AR and base-status comparisons run
  pinned T. Clark 1.17 code across affinities, upgrades, stats, and both handling
  modes. AR is compared within 0.001 per component and base status after integer
  flooring. This checks one independent calculator; it does not certify complex
  skill formulas or in-game damage. See the [comparison command](../performance.md).

The `exact-v1` contract removes intermediate rounding from ranking formulas while
preserving the positions of gameplay floors. Exactly evaluated operands can change
integer buildup near those boundaries. The v6 gameplay corrections are separate
from this arithmetic contract; neither certifies the profile data against the game.
Source `f32` coefficients remain the model inputs; arbitrary source precision is
not reconstructed. See the
[numerical contract](optimizer-math.md#numerical-contract) for the precise scope.

## Release flow

CI validates Rust, DTO, both data profiles, frontend build, and e2e contract
coverage. The release workflow requires successful main-branch push CI for the exact
source commit before packaging. Tag pushes matching `v<app version>` publish an MSI,
portable executable, convenience ZIP, SHA-256 checksums, and build provenance.
The portable executable requires WebView2; the MSI can install that prerequisite.

Manual runs default to build-only. With `publish=true`, a run from the default branch
builds and verifies the packages before creating the version tag and publishing the
release. Release notes and download links are committed beforehand, so publication
does not require a follow-up documentation push. Notes come from `docs/release-notes`.
