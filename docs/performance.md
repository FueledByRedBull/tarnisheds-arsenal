# Performance regression workflow

Use this page to reproduce and interpret optimizer, native workflow, and
release-mode responsiveness measurements. A timing comparison is meaningful only
when the requests, profile, data/model identity, upgrade policy, build profile,
and Rayon thread count match.

**Navigation:** [Home](../README.md) · [Optimizer overview](design/optimizer-overview.md) ·
[Optimizer math](design/optimizer-math.md) · [Runtime invariants](architecture/runtime-invariants.md)

**Find a task:** [Run scoring measurements](#exact-scoring-measurements) ·
[Run workflow probes](#analysis-workflows) ·
[Compare level-range evaluators](#independent-versus-shared-level-range-evaluation) ·
[Review a result](#review-policy)

For an independent Vanilla AR and base-status comparison, see the [model coverage
guide](model-reference.md#checking-model-coverage). It documents the supported
scope and known discrepancies.

## Analysis workflows

Run release-mode measurements with one Rayon thread unless a comparison requires
another fixed count. From the repository root:

```powershell
New-Item -ItemType Directory -Force dist/benchmarks | Out-Null
$env:RAYON_NUM_THREADS = "1"

python tools/phase4/benchmark_optimizer_phases.py --warmups 1 --repeats 5 --output dist/benchmarks/optimizer-phases.json
python tools/phase4/benchmark_optimizer_phases.py --profile convergence --case open-ranking-max-ar-high-level --warmups 1 --repeats 5 --output dist/benchmarks/convergence-ar.json
python tools/phase4/benchmark_workflows.py --repeats 5 --output dist/benchmarks/workflows.json
cargo run --release --locked --manifest-path core/er_optimizer_core/Cargo.toml --example benchmark_level_range -- --repeats=5 --horizons=0,10,50,200 > dist/benchmarks/level-range.json
```

The phase and workflow runners also accept `--baseline <report>`. Baseline
comparisons are advisory unless a stable dedicated runner opts into
`--fail-on-regression`.

From `apps/desktop/`, with the same thread setting, probe a packaged executable:

```powershell
node scripts/native-responsiveness.mjs src-tauri/target/release/tarnisheds-arsenal-desktop.exe --warmups=1 --repeats=3 --output=../../dist/benchmarks/native-responsiveness.json
```

The native probe requires an existing local release executable and an output
directory. It launches through the packaged smoke harness with an isolated WebView2
profile, then measures eight uncached sequential build solves, an upgrade series,
concurrent manifest access, and asynchronous cancellation. `startupMs` ends when
the model and manifest are ready and is outside measured work. A local EXE does not
need to be a published or signed MSI. `--mode=sync` supports older binaries with
synchronous commands and omits cancellation.

## Release compiler settings

Both Cargo packages set `lto = "thin"` and `codegen-units = 1` for release builds.
The desktop package needs its own settings because Cargo does not inherit a
dependency's build profile. Debug builds and the baseline CPU target are unchanged.

On 2026-09-21, compiler experiments used v0.14.1 (`ee05408`), Rust 1.97.0,
Windows, and a Ryzen 7 7800X3D. One Rayon thread was pinned to logical processor 6.
Two reversed-order blocks used one warmup and three measured repeats per case,
with no concurrent builds or tests. The combined settings reduced median runtime
by 3.9–10.3% across 16 search cases and four level-range cases; complete ordered
results matched every variant and repeat. Six Uchigatana/Seppuku frontier cases
(Keen, Blood, Occult; levels 80 and 200) used seven repeats per block and retained
identical complete frontiers, with median reductions of 1.6–7.8%.

| Workload | Default release (ms) | ThinLTO + one unit (ms) |
| --- | ---: | ---: |
| Vanilla open Max AR | 650.102 | 603.387 |
| Vanilla all-upgrade Max AR | 1494.587 | 1437.017 |
| Convergence open Max AR | 404.924 | 378.484 |
| Convergence all-upgrade Max AR | 1379.433 | 1322.531 |

Either setting alone produced mixed results, as did CPU-native targeting. These
are core measurements on one machine, not guaranteed whole-app improvements.
More cross-crate optimization can increase build time; the release-mode core
suite passed 205 tests, including exact-arithmetic promotion and DP parity checks.

Profile-guided optimization (PGO), added to the combined settings, reduced broad
search medians by 22–39% versus defaults in a separate reversed-order comparison,
including named cases excluded from training. It is not enabled for releases:
transferring the benchmark profiles to a separate frontier client produced many
missing-function-profile warnings. A shipping PGO pipeline needs training and
verification against the actual desktop build. No allocator, BigInt replacement,
or explicit SIMD dependency was justified by these experiments.

See [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) and
the [Rust PGO workflow](https://doc.rust-lang.org/rustc/profile-guided-optimization.html).

## Exact scoring measurements

The [numerical contract and identity](model-reference.md#numerical-contract-and-identity)
defines the loaded data and ranking semantics. The bounded exact DP may skip
unreachable destinations and additions, but must retain sparse allowed choices,
unused budget, the full feasible stat-spend interval, and the complete tie order.

On 2026-09-17, all 16 search fingerprints matched across the bounded and unbounded
builds and repeats; broad-search medians were mostly unchanged; and the
three-affinity 200-level range changed from 129.597 ms to 108.722 ms (16.1%).
At that point, short exact ranges cost about 0.6-3.7 ms more than historical f32
measurements under a different gameplay and rounding contract. Detailed current
measurements are in the [v0.14.0 verification record](release-notes/v0.14.0.md#verification).

### Fixed-loadout reuse and frontier experiments

The subsequent comparison against `0374693` retained request-local terminal DP
reuse for fixed-loadout, exact-upgrade level ranges. On the same Ryzen 7 7800X3D,
Rust 1.97 release builds used one Rayon thread pinned to logical processor 6,
separate target directories, one warmup, and five measured range repeats:

| Additional levels | Previous median (ms) | Reused median (ms) | Reused min–max (ms) |
| --- | ---: | ---: | ---: |
| 0 | 0.681 | 0.684 | 0.681–0.690 |
| 10 | 2.166 | 1.746 | 1.740–1.770 |
| 50 | 13.605 | 10.486 | 10.435–10.533 |
| 200 | 108.591 | 42.705 | 42.618–43.116 |

The three-affinity range retained identical complete results across builds and
independent/shared evaluation. Earlier candidate repeats also put the 200-level
case near 42 ms. All 16 ordinary-search fingerprints matched. Reversed-order
blocks used one warmup and three repeats; ordinary searches stayed close to the
baseline. The retained build's broad-search median differences ranged from about
-2.0% to +1.2%; the short first-hit case added roughly 0.002 ms. This is a measured
range improvement, not a universal no-slowdown or f32-equivalence claim.

A follow-up against `1a759e2` moved primary formula and contribution preparation
behind the same request-local cache lookup. Eligibility is unchanged; each level
still selects its own winner, feasible spend interval, and complete stat tie order.
Two blocks per build used the same pinned CPU and release settings above, with
one warmup and five repeats per range block (ten samples pooled below):

| Additional levels | Before preparation reuse (ms) | After (ms) | After min–max (ms) |
| --- | ---: | ---: | ---: |
| 0 | 0.711 | 0.702 | 0.690–0.720 |
| 10 | 1.761 | 0.895 | 0.880–0.929 |
| 50 | 10.492 | 1.738 | 1.706–1.781 |
| 200 | 42.859 | 5.126 | 5.076–5.187 |

Every complete range fingerprint matched both builds and independent evaluation.
All 16 ordinary-search fingerprints also matched in two three-repeat blocks per
build. Their pooled median differences were -1.5% to +1.3%, except the short
first-hit case (+0.004 ms, 5.5%). The first candidate retained formulas throughout
ordinary route scoring and showed roughly 3% scoring regressions in several
Vanilla cases; the retained version releases those formulas after primary winner
selection unless the range cache owns them. This trades request-local memory for
less repeated preparation, without a persistent cache or expanded search domain.

The exact Lagrangian-bound experiment also preserved all 16 fingerprints, but its
extra work cost more than it saved: Convergence export/all-upgrade cases regressed
about 11%/9% in the final comparison. It was discarded. Instrumenting the full
five-metric DP in these workloads found no active dimension with all-zero deltas;
no additional dependency-mask machinery was retained.

The AR/bleed frontier reuses ordinary exact optimization at each feasible ARC,
then removes dominated pairs. Six fixed Uchigatana/Seppuku cases (Keen, Blood,
Occult at levels 80 and 200, +25) took 0.57–8.53 ms including preparation, with one
warmup and five repeats on the same pinned thread. They returned 1–21 points with
identical repeat fingerprints. Small-budget regressions independently enumerate
all five combat stats, including floors, locks, nonmonotone curves and flat AR
ties. The shortlist, threshold, and plot do no further optimizer work.

### Workloads

| Harness | Workload |
| --- | --- |
| `benchmark_optimizer_phases` | Nine release-mode optimizer cases spanning Max AR, Max Physical AR, Bleed then AR, AoW first hit, and AoW full sequence. It reports preparation, scoring, materialization, medians, samples, row counts, equivalent combinations, profile/model identity, and result fingerprints. |
| `benchmark_workflows` | Vanilla Paths at 10, 50, and 200 levels with one and two lanes; Affinity Watch at the same horizons; and the standard 26-point upgrade series. The Rust tests perform one warmup and retain best, median, worst, and every sample. |
| `benchmark_level_range` | The documented command measures Uchigatana at level 80 plus offsets 0, 10, 50, and 200 for Keen, Blood, and Occult. The CLI defaults to 10, 50, and 200. It compares independent per-level optimization with shared range evaluation and rejects any difference in complete ordered results. Use `--all-affinities` for every available Uchigatana affinity. |
| `native-responsiveness.mjs` | Packaged solves, upgrade series, AR/bleed frontier, cancellation, and a concurrent manifest probe. Reports retain executable and manifest identity, requests, fingerprints, samples, and medians. |

Phase cases use profile rules for upgrade caps: Vanilla standard +25/Somber +10;
Convergence standard and Somber +15. Exact-upgrade and all-upgrades searches are
different workloads; `all-upgrades-max-ar-high-level` uses the broader level-93,
25-row case. Convergence damage-objective cases are skipped when the profile does
not declare AoW damage support and fail when selected explicitly. Convergence phase
cases use the harness's class-based budget, not the fixed Custom-stats UI workflow.

### Comparable inputs and result checks

Record the release/debug profile, source revision and dirty state, executable hash
when available, snapshot manifest schema/dataset/model identity, profile, Rust and
runner versions, host/CPU, Rayon thread count, normalized requests, and upgrade
policy. When comparing locally built binaries, use separate `CARGO_TARGET_DIR`
values so one build cannot overwrite the other. Keep warmups and repeated samples
consistent, reverse build order, and do not run builds or tests during timed samples.

Compare medians only after checking the full ordered result fingerprint. Include
secondary metrics, stat allocations, and extra rows; a changed winner or fingerprint
requires a correctness review before accepting a faster timing. Range comparisons
must check independent and shared results for every sample. Historical f32 results
are context only because exact-v1 and earlier floating-point builds do not share a
gameplay and ranking contract. Workflow reports currently carry timing and model
metadata but no ranked-result fingerprint, so pair them with their equivalence and
smoke tests rather than treating a faster median as proof of correctness.

## Independent versus shared level-range evaluation

The range command above starts at level 80 and measures independent optimization
for each level against shared range preparation. It is a core evaluator comparison,
not a full desktop latency measurement. The three-affinity workload keeps the
result fingerprint for every horizon; use the same affinity set, horizons, profile,
manifest, and thread count when comparing builds. Output is JSON Lines with
metadata, timed samples, and per-horizon medians; its Rust Debug fingerprints are
diagnostic and should only be compared with the same result types.

## Optimizer phase attribution

The phase runner separates cold request preparation, candidate scoring/top-k
retention, and final result materialization. It covers the five objectives while
retaining exact integer DP coefficients and exact rational terminal ranking keys.
The optimizer proof still requires separability and sound active-stat selection;
performance comparisons do not justify changing those correctness conditions.

## Native cancellation

The reference cancellation target for Broad Search, Paths, and Affinity Watch is
250 ms on the reference development machine. Core enumeration tests check the
target, and backend workflow tests check propagation through nested evaluators and
fail-closed results without partial payloads.

In the native report, `cancellationMeasured: false` means the calculation finished
before cancellation took effect, so that sample provides no cancellation latency.
A successful cancel request or a failed status request also does not prove that a
native worker stopped; preserve native ownership and reconcile its status before
starting replacement work. Native probes are not full UI or migration measurements
and do not establish in-game damage accuracy.

## All-Ash compatibility regression

The historical compact-schema/shared-primary compatibility comparison required
identical ranked rows, stats, and numeric metrics, but used a corrected unoptimized
worktree and 16 Rayon threads, so its timings are local diagnostics rather than a
current baseline. The full historical table remains available in the [immutable
source entry](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/0374693639cb66a835be5b4ac0dacb8970c4c2d7/docs/performance.md#all-ash-compatibility-regression).
Counts and equivalent exhaustive-combination estimates are not latency metrics.

## Review policy

- Compare medians and sample ranges, never a single sample.
- Treat a greater-than-20% median change as an initial review threshold, not an
  automatic product failure. Runners are advisory by default.
- Refresh a baseline only after result-equivalence checks pass and the change is
  understood. Use `--fail-on-regression` only on a stable dedicated runner.
- Local diagnostics are opt-in; the application emits no telemetry or default
  timing logs.
