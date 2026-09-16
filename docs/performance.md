# Performance regression workflow

Use this page to reproduce and interpret optimizer, native workflow, and
release-mode responsiveness measurements. Compare matching requests, data, build
profiles, and thread counts before treating a timing difference as meaningful.

**Navigation:** [Home](../README.md) · [Optimizer overview](design/optimizer-overview.md) ·
[Optimizer math](design/optimizer-math.md) · [Runtime invariants](architecture/runtime-invariants.md)

**Find a task:** [Run scoring measurements](#exact-scoring-measurements) ·
[Probe native responsiveness](#analysis-workflows) ·
[Compare level-range evaluators](#independent-versus-shared-level-range-evaluation) ·
[Attribute phases](#optimizer-phase-attribution) · [Review a result](#review-policy)

For an independent Vanilla AR and base-status comparison, run
`python tools/phase4/validate_external_calculator.py --count 100 --seed 20260918`.
The seed selects weapon names, affinities, stats, and legal unbuffed skills; each
selected configuration is checked at zero, random, and maximum upgrades with
both handling modes. The runner uses pinned T. Clark 1.17 source and data,
the installed frontend TypeScript compiler, and the current Rust optimizer.
Use another seed for fresh cases, or `--count 487` to cover every named weapon.
Optional `--report` and `--csv` paths retain results. External inputs are not
release dependencies. This checks weapon AR, base passives, and selected-skill
identity; it does not certify projectile formulas, temporary buffs, or in-game
damage after enemy defenses.

Performance work is measured in release mode with one Rayon thread by default so algorithm changes are visible without scheduler noise.

Broad Search, Paths, and Affinity Watch cancellation has a 250 ms latency target on the reference development machine. Core enumeration checks this with a synchronized broad-search regression test; workflow tests separately prove cancellation propagates through their nested evaluators and returns no successful partial payload.

## Exact scoring measurements

These timings predate the `aow-routes-effects-v6` gameplay corrections. The
corrected model intentionally changes affected skill routes, status buildup,
and weapon scaling; use the commands above for a current correctness comparison.

The September 2026 follow-up compared `41aeaa1` (f32), `93bf35a` (the previous
exact build), and this PR's reviewed exact implementation on Windows 11 with Rust
1.97 release builds and one Rayon thread. Each case had one warmup, three f32
repeats, and five exact repeats, with identical requests and upgrade policies.
These are different numerical contracts: former rounding-induced ties and status
floors can change. Complete ranked fingerprints, including exact keys and combat
stats, remained identical between the two exact implementations in all nine Vanilla
and seven supported Convergence search cases.

| Vanilla case | f32 median (ms) | Exact median (ms) | Exact min-max (ms) |
| --- | ---: | ---: | ---: |
| Open Max AR | 1,026.4 | 636.9 | 631.6-644.1 |
| Open physical AR | 1,025.0 | 619.8 | 612.8-623.4 |
| Max AR, 500-row export | 1,016.5 | 706.6 | 701.3-709.8 |
| High-level Max AR | 1,461.3 | 633.4 | 621.9-646.9 |
| High-level, all upgrades | 28,794.1 | 1,435.0 | 1,418.2-1,450.5 |
| Katana bleed | 13.19 | 11.10 | 11.09-11.25 |
| Katana bleed, 500 rows | 12.88 | 11.44 | 11.22-11.48 |
| Fixed AoW first hit | 0.169 | 0.064 | 0.063-0.070 |
| Fixed AoW sequence | 0.163 | 0.140 | 0.139-0.146 |

All nine medians improved in this run. Exact scoring uses inline `i128` rationals
for ordinary coefficients and checked integer DP keys, promoting to arbitrary
precision when needed. Shared primary plans retain every primary tie; unique
primary winners avoid unnecessary secondary work. Broad and all-upgrade searches
also benefit from exact bounds and evaluating promising configurations first.
Scheduling estimates do not prune candidates. These measurements do not establish
a speedup for every request, and the sub-millisecond cases show timing variability.

For loadouts without a skill route, the primary winner now avoids a second DP
when all remaining metrics are constant or already in the primary pair. The primary
pass keeps the canonical stat representative while retaining every tied predecessor.
This removed an existing Convergence bleed slowdown:

| Convergence case | f32 median (ms) | Previous exact (ms) | Reviewed exact (ms) | Exact min-max (ms) |
| --- | ---: | ---: | ---: | ---: |
| Katana bleed | 19.48 | 21.95 | 9.66 | 9.64-9.78 |
| Katana bleed, 500 rows | 19.57 | 22.43 | 9.84 | 9.71-9.92 |

These core fixtures retain the benchmark's class-based optimization budget; they
are not measurements of Convergence's fixed Custom-stats UI workflow. All seven
supported Convergence search medians were below their f32 references. A trial that
removed redundant DP columns and a range-local formula cache showed no useful
measured benefit and were discarded.

A four-thread, 500-row Max AR export check also preserved the complete serial result
fingerprint. Three-repeat medians moved from 565.9 to 551.0 ms for Vanilla and from
487.4 to 232.5 ms for Convergence; reviewed ranges were 546.9-557.7 and 228.3-234.4 ms.

Earlier exact-v1 workflow measurements used one warmup and three repeats. These
were not rerun as native application timings in the follow-up. Paths includes one-
and two-lane runs; Affinity Watch uses all 13 eligible affinities.

| Workflow | Previous median (ms) | Exact median (ms) | Exact min-max (ms) |
| --- | ---: | ---: | ---: |
| Affinity Watch, 10 levels | 23.152 | 36.193 | 35.890-36.233 |
| Affinity Watch, 50 levels | 185.962 | 240.301 | 240.242-241.122 |
| Affinity Watch, 200 levels | 2,971.433 | 2,619.789 | 2,608.413-2,629.535 |
| Upgrade series, 26 points | 0.405 | 0.605 | 0.578-0.686 |
| Paths, 10 levels, one lane | 0.749 | 0.892 | 0.887-0.909 |
| Paths, 10 levels, two lanes | 1.473 | 1.733 | 1.725-1.742 |
| Paths, 50 levels, one lane | 1.064 | 1.292 | 1.285-1.320 |
| Paths, 50 levels, two lanes | 2.131 | 2.593 | 2.582-2.604 |
| Paths, 200 levels, one lane | 4.118 | 5.436 | 5.436-5.479 |
| Paths, 200 levels, two lanes | 8.199 | 10.898 | 10.896-10.912 |

The remaining Paths/upgrade overhead is at most 2.7 ms in these cases. Shorter
Affinity Watch runs add 13-54 ms, while the 200-level run improves by 352 ms.
These are measured tradeoffs, not a claim of uniform performance parity. The
calculations run on cancellable native workers rather than the window thread.
The follow-up did rerun the separate three-affinity core level-range case at
horizons 0, 10, 50, and 200. Reviewed exact medians were 0.686, 2.203, 14.289, and
131.072 ms, versus previous exact medians of 0.696, 2.218, 14.458, and 131.370 ms.
The f32 references were 0.671, 1.523, 10.532, and 117.984 ms. Independent and shared
range evaluations retained identical complete result fingerprints. These ranges
still have pre-existing exact-arithmetic overhead; the search speedups above do
not establish universal f32 performance parity.

The rebuilt native release EXE completed an uncached eight-build solve batch in
29.44 ms and a 26-point upgrade series in 4.33 ms (three-repeat medians including
native communication). Concurrent manifest commands took 1.89 and 1.98 ms. Two
conclusive cancellation samples both took 3.65 ms; a third calculation completed
before cancellation took effect and was excluded from that median. The native
smoke flow passed both profiles, comparisons, Paths, Affinity Watch, and saved
builds. This was a local release EXE, not a newly published or signed MSI. These
checks establish application behavior, not independent in-game validation of
damage formulas.

## Stat-entry and search-space estimation

Editing a numeric character field is local UI draft state. A valid value commits on
blur, Enter, or after 700 ms idle, but it does not run the exact search-space
estimator. The command rail shows a constant-time scope summary; exact candidate
preparation begins only when Search is pressed. This keeps rapid multi-field edits
off the optimizer worker path and avoids stale CPU work.

The exact estimator is retained in the core as `estimate_search_space` and is
cancellable, but it has no command or frontend caller: since v0.10.0 nothing in
`apps/` or `tools/` invokes it, and it is exercised only by
`core/er_optimizer_core/src/optimizer/tests.rs`. Its
result must still equal the full prepared plan's weapon, stat-distribution, and
equivalent combination counts, which `estimate_search_space_uses_relevant_stat_counts`
enforces. Estimation omits scoring work-unit materialization, reuses distribution
counts with identical stat bounds, and checks weapon requirements with an arithmetic
feasibility test.

On the July 2026 reference snapshot, a one-thread release benchmark retained the
same exact counts while reducing representative estimates from 0.82-3.10 seconds to
0.046-0.117 seconds (roughly 17-27x). These local timings are diagnostic, not a CI
guarantee; correctness is enforced by result-equivalence tests.

## Analysis workflows

For a Windows native-command responsiveness probe, run from `apps/desktop/`:

```powershell
node scripts/native-responsiveness.mjs src-tauri/target/release/tarnisheds-arsenal-desktop.exe --warmups=1 --repeats=3 --output=../../dist/benchmarks/native-responsiveness.json
```

Create the output directory first. The probe uses the packaged smoke launcher and
an isolated WebView2 profile. It measures one batch of eight uncached sequential
loadout solves, an upgrade series, and direct cancellation. These are native-command
probes, not full comparison or migration UI measurements. Use `--mode=sync` only
for older binaries exposing the synchronous commands; that mode omits cancellation.

Reports retain the executable SHA-256, the binary's data manifest, host/Node details,
Rayon thread count, actual requests, complete result fingerprints, individual samples,
and medians. Rayon defaults to one thread; set `RAYON_NUM_THREADS` explicitly for
other counts. Compare identical requests, data/model identity and thread settings.
`startupMs` ends when the model and manifest are ready, before measured work begins.
Cancellation samples with `cancellationMeasured: false` are inconclusive: the work
finished before cancellation was observed. Do not treat their timings as cancellation
latency or a successful cancellation regression.

Run:

```powershell
python tools/phase4/benchmark_workflows.py --repeats 5 --output dist/benchmarks/workflows.json
```

This exercises Paths at 10, 50, and 200 levels with one and two lanes, a 13-affinity Affinity Watch at the same horizons, and the direct standard upgrade-series evaluator. The Rust harness performs one warmup before measured samples. The runner records release profile, Rust/Python versions, CPU/platform, Rayon thread count, commit, data/model identity, and best/median/worst samples. It supports the same advisory baseline and percentage-regression options.

## Independent versus shared level-range evaluation

From the repository root:

```powershell
$env:RAYON_NUM_THREADS = "1"
cargo run --release --locked --manifest-path core/er_optimizer_core/Cargo.toml --example benchmark_level_range -- --repeats=3 --horizons=10,50,200
```

Add `--all-affinities` to cover every Uchigatana affinity instead of Keen, Blood,
and Occult. This compares independent per-level optimization with shared range
preparation, not full desktop latency. Output is JSON Lines: metadata, each timed
sample, and per-horizon medians. Redirect stdout into the existing ignored
`dist/benchmarks/` directory when retaining a run; create that directory first.

Metadata records dataset/model identity, actual Rayon threads, OS/architecture,
logical CPU count, Windows CPU identifier when available, Rust version, checkout
revision/dirty status, executable SHA-256, and base requests. Horizons start at
level 80. Each sample retains the complete ordered results as a Rust Debug string
and rejects any difference between the two evaluators, including secondary metrics
and extra rows. Debug fingerprints are diagnostic, not a stable interchange format;
compare them with the same result types. Checkout metadata describes the checkout
at execution; use the binary hash to identify an executable copied from elsewhere.

## Optimizer phase attribution

Run:

```powershell
python tools/phase4/benchmark_optimizer_phases.py --warmups 1 --repeats 5 --output dist/benchmarks/optimizer-phases.json
```

The release-mode harness measures cold request preparation, candidate scoring/top-k retention, and final result materialization independently for all five objectives, including broad/open, high-level, exact-lock, and open-AoW cases. It records per-phase samples and medians, result counts, search-space size, build profile, Rayon thread count, dataset/model versions, commit, CPU, Rust version, and platform. Compare against a reviewed report with `--baseline`; comparisons report regressions but remain advisory unless a stable dedicated runner explicitly uses `--fail-on-regression`.

The phase suite includes both low-level and high-level open Max AR searches. AR
scoring uses exact integer DP coefficients and exact rational terminal keys.
The proof still requires separability and sound active-stat selection
(see [the numerical contract](design/optimizer-math.md#numerical-contract)).
Comparisons with older `f32` builds may legitimately change near-tie winners;
review those changes separately from same-contract parity. Reports retain the equivalent exhaustive
combination count so historical search-space comparisons remain meaningful.

The original phase cases use exact upgrade caps: +25/+10 for Vanilla and +15 for
Convergence. The application's default search covers every level from +0 through
the selected caps. The `all-upgrades-max-ar-high-level` case measures that broader
search at level 93 with 25 results. Keep these workloads separate when comparing
timings.

For that all-upgrade case, the optimizer caches a primary allocation only when
the best primary rank has one terminal spend and one retained DP path.
Ambiguous winning paths and exact primary ties retain the
route-aware evaluation. This avoids repeating route scoring for allocations that
cannot win under the exact primary ordering.

The historical timings below are observations, not reusable regression baselines.
Their original dirty-worktree inputs and saved reports are not identified by
immutable artifacts here, so these notes alone cannot reproduce those comparisons.
Use freshly captured reports from identified builds for new performance claims.

On the reference host with two Rayon workers, the original single-run baseline
was 76,207.7 ms. An intermediate implementation's three-sample run measured
35,742.6, 37,952.7, and 70,213.6 ms (median 37,952.7 ms); after simplifying
the uniqueness check to direct backtracking, a final-source run measured
21,144.7 ms. Every run retained the complete 25-row fingerprint and
253,137,580,441 equivalent combinations. The variability and different sample
counts make these diagnostic measurements, not a fixed latency guarantee.

AR and Bleed scoring share primary weapon contributions and tied DP transitions
among compatible Ashes with identical primary effects. Route-specific metrics still
choose among all primary ties; final rows retain complete numeric comparisons.
The exhaustive path retains its bleed-only scoring optimization, with direct
equivalence tests against the full status calculation. A measured
heap-based top-k rewrite was rejected: raising broad export retention from 5 to 500
cost only about 4.1 ms in scoring and 5.6 ms total, too little to justify more
complex deterministic grouping and tie handling.

With a one-thread Rayon pool, searches prepare scalar AoW route templates once
per resolved choice and reuse them when the route is stat-independent. A pool
with multiple threads retains per-work-unit preparation because eager
compilation creates a serial preparation barrier without a measurable scoring
benefit. Choices with per-hit attack-power effects marked supported keep the exhaustive
evaluator, but that selection does not implement those effects: `PerHitAttackPower`
remains unsupported by the damage calculation. On the current
Windows 11 reference host,
one-thread release medians with one warmup and five samples changed open Max AR
from 3,690.073 ms to 3,066.442 ms (−16.90% total; scoring −22.86%) with an
identical ranked-result fingerprint. A three-sample high-level run changed
4,496.057 ms to 4,479.665 ms (−0.36%), also with an identical fingerprint; its
preparation is higher because the conditional fallback path preserves the full
evaluator. These are local diagnostic measurements, not timing guarantees.

At 16 Rayon threads, the saved Convergence v5 report was rejected until two
stale `is_somber` flags in its first rows were corrected to match the current
data. With only that data correction, the complete fingerprint matched and the
current parallel fallback measured 282.127 ms total versus 348.292 ms in the
saved report (−19.00%).

The optimizer also excludes partially modeled AoWs from first-hit and full
sequence damage ranking when a selected non-missing-FP attack row contains an
unsupported effect. Missing-FP rows are ignored because the route evaluator
does not evaluate them. This prevents an incomplete route from competing
numerically with fully modeled skills; AR and status objectives retain their
existing warning behavior.

## All-Ash compatibility regression

Ash counts and equivalent exhaustive-combination estimates are not measured latency.
The compact-schema/shared-primary change was compared with the corrected, unoptimized
worktree using release binaries, 16 Rayon threads, one warmup and three measured
samples per case. These are local diagnostic medians, not universal timing promises:

| Profile / case | Before (ms) | After (ms) |
| --- | ---: | ---: |
| Vanilla / open Max AR | 863.63 | 814.18 |
| Vanilla / open Physical AR | 866.19 | 816.06 |
| Vanilla / high-level Max AR | 1223.07 | 1068.95 |
| Vanilla / Katana Bleed | 11.15 | 11.07 |
| Convergence / open Max AR | 458.72 | 348.29 |
| Convergence / open Physical AR | 467.91 | 350.17 |
| Convergence / high-level Max AR | 737.46 | 457.22 |
| Convergence / Katana Bleed | 8.07 | 6.09 |

Every case retained identical ranked rows, stats, and numeric metrics. All broad-case
medians remain below the local 1.5-second review budget. The small Katana timing
change should not be interpreted as a meaningful speedup. The reference is the
pre-optimization dirty worktree, not an assertion about historical HEAD latency.

Run a profile/case through the existing runner (one thread by default):

```powershell
python tools/phase4/benchmark_optimizer_phases.py --profile convergence --case open-ranking-max-ar-high-level --warmups 1 --repeats 5 --output dist/benchmarks/convergence-ar.json
```

Set `RAYON_NUM_THREADS` explicitly when comparing another thread count. Unsupported
Convergence damage-objective cases are excluded from the default profile suite;
requesting one explicitly fails. New reports include ranked-result fingerprints;
a baseline comparison with changed results fails rather than accepting faster,
incorrect answers. Older reports without fingerprints still require the equivalence
tests before performance conclusions are drawn.

## Review policy

- Compare medians, never a single sample.
- A greater-than-20% median change is the initial review threshold, not automatically a product failure. All benchmark runners are advisory by default; `--fail-on-regression` is an explicit dedicated-runner policy choice.
- Refresh a baseline only after result-equivalence tests pass and the change is understood.
- CI should enforce a timing budget only on a stable dedicated runner. Shared GitHub-hosted timing is advisory because machine variance can exceed the threshold.
- Local diagnostics remain opt-in; the application emits no telemetry or default timing logs.
