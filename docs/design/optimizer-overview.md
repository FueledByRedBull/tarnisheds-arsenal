# Optimizer design overview

[Home](../../README.md) · [Model reference](../model-reference.md) ·
[Mathematics](optimizer-math.md) · [Runtime invariants](../architecture/runtime-invariants.md) ·
[Performance](../performance.md)

This page maps the implementation and follows a request from the desktop to a
ranked result. Player workflows are described in the [interface guide](../../README.md#interface);
mechanic coverage and special cases live in the [model reference](../model-reference.md).

## Components

| Component | Responsibility | Source |
| --- | --- | --- |
| React + Zustand desktop | Build session, workspace selection, stale results, saved inputs | [`apps/desktop/src`](../../apps/desktop/src) |
| Native command layer | DTO validation, profile selection, worker ownership, progress and cancellation | [`apps/desktop/src-tauri/src`](../../apps/desktop/src-tauri/src) |
| Rust optimizer | Prepare legal candidates, optimize stats, rank and materialize results | [`core/er_optimizer_core/src/optimizer.rs`](../../core/er_optimizer_core/src/optimizer.rs) |
| Calculation model | Exact formulas, scaling, skill routes, status and display projection | [`core/er_optimizer_core/src/math`](../../core/er_optimizer_core/src/math) |
| Snapshot loader | Validate the manifest and load one complete profile | [`core/er_optimizer_core/src/data.rs`](../../core/er_optimizer_core/src/data.rs) |
| Extraction and validation | Produce runtime snapshots from local inputs and check their contracts | [`tools/phase1`](../../tools/phase1), [`tools/phase4`](../../tools/phase4) |

## Request to result

```text
Build session + selected profile
    -> validated request and native job
    -> legal weapons, upgrades, skills and stat bounds
    -> prepared formulas and stat optimization
    -> exact ranking and top-K selection
    -> detailed results and rounded display values
```

### Session and worker ownership

The frontend keeps one build session. Changing calculation inputs atomically marks
related results stale. Rankings rows carry the selected setup, combat stats, and
scaling; Build Detail shows the full breakdown. Compare requires fresh Rankings
and uses one current request for its baseline and alternatives. Responsive layouts
keep workspace content usable and respect reduced motion.

Native workers retain an `Arc` to the selected profile. Heavy calculations run on
bounded workers rather than the native main thread. Job IDs and frontend request
generations prevent obsolete replies from replacing current results. Search,
Paths, and Affinity Watch reuse the same queue implementation with separate
ownership. Cancellation is a request to stop: uncertain worker state retains its
job identity and blocks replacement. Bounded reconciliation either confirms
termination or reports the unresolved state.

Direct analyses share bounded frontend caches keyed by request and profile/model
identity. A cancelled subscriber does not stop a calculation another subscriber
still needs; the last cancellation reaches the worker. Failed batch siblings are
aborted together. See [runtime invariants](../architecture/runtime-invariants.md)
for the complete lifecycle, persistence, and cache contract.

### Profile and candidate preparation

Each manifest-bound snapshot loads all-or-nothing after file size/hash checks.
Profile capabilities govern supported objectives and mechanics. Commands, caches,
presets, and exports carry profile identity; data cannot be mixed across profiles.

`prepare_search_with_cancel` validates capabilities, budgets and constraints,
then resolves legal weapon/affinity/skill/upgrade choices. Requirements raise
combat-stat floors; exact locks and caps bound the remaining search. The chosen
upgrade policy uses each weapon's actual reinforcement series. Native skills and
transferable Ashes have distinct eligibility rules.

`RelevantStatSearch` identifies attributes that can change any ranking component.
It covers the full feasible active-stat spend interval and completes inactive
stats canonically to preserve the final stat-vector tie order. Its logical
candidate count describes the equivalent exhaustive domain, not the number of DP
transitions executed.

### Exact scoring

For separable formulas, the optimizer compiles per-stat contributions and uses a
lexicographic dynamic program. Checked coefficient bounds select `i128` when safe
and `BigInt` otherwise. Exact rational terminal keys compare loadouts with different
scales. Coupled routes use direct enumeration when no valid shortcut applies.
The [mathematical document](optimizer-math.md) defines the recurrence, numerical
contract, relevance obligations, and complete ordering.

Max AR, Max Physical AR, and Bleed searches reuse primary plans across Ashes with
identical primary effects at the same upgrade and stat bounds. Plans retain all
predecessors tied on objective score and total AR; route-specific scoring resolves
remaining skill, bleed, and stat ties. A unique primary winner can be reused only
when those later comparisons cannot change it. This cache is request-local and
released after each upgrade.

Medium and broad searches use Rayon when the estimated work justifies it. Skill-
damage work can split by Ash, while AR/Bleed work stays grouped to reuse primary
plans. Preparation, scoring, and materialization are measured separately by the
[performance harnesses](../performance.md).

### Ranking and materialization

Scoring retains lightweight exact keys through top-K selection. Only retained
candidates are materialized into full results: damage splits, chosen route/hits,
status, poise, stamina, and warnings. The public AR helper uses the same exact
calculation and projects once to display values; floating-point estimates serve
scheduling only.

Final ordering and de-duplication preserve the complete numeric and stat key.
Tied routes additionally compare route priority and ID. Display rounding cannot
change the selected winner. Unsupported mechanics stay explicit instead of being
filled with guessed values.

## Reuse across workspaces

Rankings discovers loadouts. Compare solves alternatives under the current
budget, while reusable evaluators support fixed-loadout and upgrade-series work.
Paths pins the selected loadout and clears discovery filters before evaluation;
Affinity Watch evaluates compatible affinity alternatives. Their stat-treatment
differences are explained once in the [interface guide](../../README.md#interface).
Reusable evaluators revalidate variable inputs against profile capabilities.

Fixed-loadout, exact-upgrade level ranges reuse primary formulas, stat contributions,
and terminal DP tables when stat bounds, active attributes, and scoring context match.
The preparation cache lives only for that range request. Each level still scans its own
feasible spend interval and fills inactive stats canonically. Reuse is limited to
top-one AR, physical AR, and bleed objectives; no-respec progression retains its
separate allocation rule.

Compare's AR/bleed frontier scans feasible ARC values with the ordinary evaluator,
then removes exactly dominated metric pairs. The shortlist and sacrifice control
query that completed result without recalculation. Point inspection preserves the
allocation; applying it uses the existing exact stat locks and Rankings actions.
See the [frontier contract](optimizer-math.md#8-fixed-loadout-ar--bleed-frontier).

## Where to verify a change

- **Recurrence, relevance, or tie order:** core regressions compare DP with exact
  exhaustive evaluation and separately check the active-stat reduction. Agreement
  with a reduced oracle alone cannot prove that omitted stats are irrelevant.
- **Game formulas or compatibility:** use the [model checks](../model-reference.md#checking-model-coverage)
  and snapshot validation. Production self-consistency, external-calculator
  agreement, and in-game fidelity are different claims.
- **Jobs, caches, or saved builds:** follow the [runtime invariants](../architecture/runtime-invariants.md)
  and their unit, native, and browser regressions.
- **Speed or responsiveness:** follow the [performance guide](../performance.md)
  using comparable requests and complete result fingerprints.
- **Packaging or publication:** the [release guide](../releasing.md) owns preview,
  exact-commit publication eligibility, signing, and retry procedures.

Dated verification campaigns and corrections belong in [release history](../release-notes/README.md).
