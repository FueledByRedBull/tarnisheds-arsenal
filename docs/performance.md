# Performance regression workflow

Performance work is measured in release mode with one Rayon thread by default so algorithm changes are visible without scheduler noise.

Broad Search, Paths, and Affinity Watch cancellation has a 250 ms latency target on the reference development machine. Core enumeration checks this with a synchronized broad-search regression test; workflow tests separately prove cancellation propagates through their nested evaluators and returns no successful partial payload.

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

Run:

```powershell
python tools/phase4/benchmark_workflows.py --repeats 5 --output dist/benchmarks/workflows.json
```

This exercises Paths at 10, 50, and 200 levels with one and two lanes, a 13-affinity Affinity Watch at the same horizons, and the direct standard upgrade-series evaluator. The Rust harness performs one warmup before measured samples. The runner records release profile, Rust/Python versions, CPU/platform, Rayon thread count, commit, data/model identity, and best/median/worst samples. It supports the same advisory baseline and percentage-regression options.

## Optimizer phase attribution

Run:

```powershell
python tools/phase4/benchmark_optimizer_phases.py --warmups 1 --repeats 5 --output dist/benchmarks/optimizer-phases.json
```

The release-mode harness measures cold request preparation, candidate scoring/top-k retention, and final result materialization independently for all five objectives, including broad/open, high-level, exact-lock, and open-AoW cases. It records per-phase samples and medians, result counts, search-space size, build profile, Rayon thread count, dataset/model versions, commit, CPU, Rust version, and platform. Compare against a reviewed report with `--baseline`; comparisons report regressions but remain advisory unless a stable dedicated runner explicitly uses `--fail-on-regression`.

The phase suite includes both low-level and high-level open Max AR searches. AR
scoring uses an exact relevant-stat dynamic program and has a direct regression
against exhaustive enumeration. Reports retain the equivalent exhaustive
combination count so historical search-space comparisons remain meaningful.

The original phase cases use exact upgrade caps: +25/+10 for Vanilla and +15 for
Convergence. The application's default search covers every level from +0 through
the selected caps. The `all-upgrades-max-ar-high-level` case measures that broader
search at level 93 with 25 results. Keep these workloads separate when comparing
timings.

For that all-upgrade case, the optimizer caches a primary allocation only when
every feasible spend has one retained DP path and exact primary re-evaluation
produces a unique winner. Ambiguous paths and exact primary ties retain the
route-aware evaluation. This avoids repeating route scoring for allocations that
cannot win without relying on additive floating-point bounds.

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
benefit. Choices with supported per-hit attack-power effects keep the exhaustive
evaluator because their route values depend on the evolving stats. On the current
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
