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

The September 17, 2026 follow-up isolates the reachable-state DP bounds from
gameplay corrections. Both builds use the current schema-5, `aow-routes-effects-v7`
snapshots and `exact-v1` scoring; the baseline replaces only `exact_dp.rs` with
the unbounded recurrence from `831ca38`. This measures the loop change, not the
entire PR against f32 or an older gameplay model.

Rust 1.97 release builds ran on a Ryzen 7 7800X3D, Windows 11, with one Rayon
thread pinned to logical processor 6. Separate Cargo target directories prevent
one build from overwriting the other executable. Each search used two reversed-
order blocks, one warmup and three samples per block (six samples per build).
No builds or tests ran during measurement; ordinary desktop activity remained.
Raw samples, binary/snapshot hashes, requests and full ranked fingerprints were
retained locally. All 16 fingerprints match across builds and repeats.

| Profile / case | Unbounded (ms) | Bounded (ms) | Bounded min-max (ms) |
| --- | ---: | ---: | ---: |
| Vanilla / Open Max AR | 649.142 | 654.037 | 646.286-662.260 |
| Vanilla / Open physical AR | 642.310 | 639.076 | 634.798-646.688 |
| Vanilla / Max AR, 500 rows | 728.593 | 730.768 | 726.974-734.139 |
| Vanilla / High-level Max AR | 660.386 | 659.715 | 653.699-664.901 |
| Vanilla / High-level, all upgrades | 1,453.430 | 1,464.898 | 1,443.102-1,486.263 |
| Vanilla / Katana bleed | 11.047 | 10.752 | 10.615-10.909 |
| Vanilla / Katana bleed, 500 rows | 11.143 | 11.066 | 10.974-11.208 |
| Vanilla / Fixed AoW first hit | 0.072 | 0.074 | 0.070-0.075 |
| Vanilla / Fixed AoW sequence | 0.153 | 0.142 | 0.138-0.151 |
| Convergence / Open Max AR | 408.472 | 402.848 | 401.730-409.201 |
| Convergence / Open physical AR | 385.334 | 383.654 | 381.723-386.569 |
| Convergence / Max AR, 500 rows | 525.175 | 519.052 | 516.820-522.817 |
| Convergence / High-level Max AR | 450.544 | 448.388 | 445.798-470.858 |
| Convergence / High-level, all upgrades | 1,400.188 | 1,345.984 | 1,326.785-1,372.496 |
| Convergence / Katana bleed | 10.090 | 9.888 | 9.860-9.933 |
| Convergence / Katana bleed, 500 rows | 10.127 | 10.069 | 10.007-10.168 |

Broad-search medians are mostly unchanged: Vanilla's largest increase is 0.8%,
within overlapping sample ranges. Convergence's all-upgrade median improves
3.9%, although its baseline has a noisy upper sample. Fixed first-hit work adds
about 0.002 ms; the sequence median improves 7.2%. These measurements support
the small loop change, not a universal speedup claim.

The three-affinity level-range case used one warmup and five samples per block
(ten per build). Full results match across builds; independent and shared range
evaluation also agree within each build.

| Additional levels | Unbounded (ms) | Bounded (ms) | Bounded min-max (ms) |
| --- | ---: | ---: | ---: |
| 0 | 0.692 | 0.674 | 0.668-0.704 |
| 10 | 2.235 | 2.171 | 2.154-2.197 |
| 50 | 14.322 | 13.568 | 13.510-13.785 |
| 200 | 129.597 | 108.722 | 108.266-113.157 |

The 200-level case improves 16.1%, while 10 and 50 levels improve 2.9% and 5.3%.
Bounds skip only unreachable DP destinations/additions; sparse allowed choices,
unused budget and the complete tie order remain intact.

Reproduce searches with `benchmark_optimizer_phases --warmups=1 --repeats=3`
and ranges with `benchmark_level_range --repeats=5 --horizons=0,10,50,200` below.
Reverse build order and retain full outputs rather than comparing only winners.
Convergence search benchmarks use the harness's class-based budget, not the
fixed Custom-stats UI workflow.

### Historical f32 comparison

The earlier same-snapshot v6 comparison of `41aeaa1` (f32), `6dc4aba` (previous
exact) and `831ca38` found all 16 search medians faster than f32. Four broad
searches were 2.7-4.7% slower than the previous exact build in 14-sample
confirmation runs. Those builds have different gameplay/rounding contracts;
their full fingerprints do not all match. They are historical context, not a
same-contract regression baseline for the current corrected model.

Historical f32 three-affinity range medians were 0.652, 1.539, 9.848 and 117.697 ms
at horizons 0, 10, 50 and 200. The bounded exact implementation closes part of
the short-range gap and beats that historical 200-level timing, but short ranges
still cost roughly 0.6-3.7 ms more. This does **not** establish universal f32
performance parity. Retain exact ordering rather than weakening correctness to
remove those milliseconds.

### Native responsiveness

The rebuilt local release EXE completed an uncached eight-build solve batch in
29.66 ms and a 26-point upgrade series in 4.43 ms (three-repeat medians after
one warmup, including native communication). Concurrent manifest commands took
1.89 and 2.06 ms and completed before the heavy work in every sample.
All three cancellation probes finished their calculation before cancellation
took effect, so this run establishes no cancellation latency. The synchronized
core/backend cancellation regressions provide separate behavioral coverage.

Native smoke passed both profiles, comparisons, Paths, Affinity Watch and saved
builds. This was a local release EXE, not a newly published or signed MSI, and
does not establish in-game damage accuracy.

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
