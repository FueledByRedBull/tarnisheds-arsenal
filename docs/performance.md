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

Bleed, then AR scoring uses a bleed-only calculation while candidates are broad.
Full AR and all seven status values are calculated only for tie-breaking and final
rows, with direct equivalence tests against the full status calculation. A measured
heap-based top-k rewrite was rejected: raising broad export retention from 5 to 500
cost only about 4.1 ms in scoring and 5.6 ms total, too little to justify more
complex deterministic grouping and tie handling.

## Review policy

- Compare medians, never a single sample.
- A greater-than-20% median change is the initial review threshold, not automatically a product failure. All benchmark runners are advisory by default; `--fail-on-regression` is an explicit dedicated-runner policy choice.
- Refresh a baseline only after result-equivalence tests pass and the change is understood.
- CI should enforce a timing budget only on a stable dedicated runner. Shared GitHub-hosted timing is advisory because machine variance can exceed the threshold.
- Local diagnostics remain opt-in; the application emits no telemetry or default timing logs.
