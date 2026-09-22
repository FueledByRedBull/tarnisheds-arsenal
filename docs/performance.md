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

The phase runner accepts `--threads=1`, `--threads=2`, or another positive count;
`--threads=default` explicitly clears `RAYON_NUM_THREADS`. Without that option it
preserves an existing environment setting and otherwise selects one thread. The
report records the actual Rayon pool size, and the harness checks complete ordered
results for every warmup as well as every measured repeat.

From `apps/desktop/`, with the same thread setting, probe a packaged executable:

```powershell
node scripts/native-responsiveness.mjs src-tauri/target/release/tarnisheds-arsenal-desktop.exe --warmups=1 --repeats=3 --output=../../dist/benchmarks/native-responsiveness.json
node scripts/native-responsiveness.mjs src-tauri/target/release/tarnisheds-arsenal-desktop.exe --suite=production --threads=1 --warmups=1 --repeats=3 --output=../../dist/benchmarks/native-production-one.json
node scripts/native-responsiveness.mjs src-tauri/target/release/tarnisheds-arsenal-desktop.exe --suite=production --threads=default --warmups=1 --repeats=3 --output=../../dist/benchmarks/native-production-default.json
```

The native probe requires an existing local release executable and an output
directory. It launches through the packaged smoke harness with an isolated WebView2
profile, then measures eight uncached sequential build solves, an upgrade series,
concurrent manifest access, and asynchronous cancellation. `startupMs` ends when
the model and manifest are ready and is outside measured work. A local EXE does not
need to be a published or signed MSI. `--mode=sync` supports older binaries with
synchronous commands and omits cancellation.

`--suite=production` uses native asynchronous jobs for a locked, non-additive
Lifesteal Fist solve, a 500-row search, both 50-level Paths modes with two distinct
loadouts, and cancellation of Search, Solve and both Paths modes. Fixtures are
prepared before timing. `--threads=default` unsets `RAYON_NUM_THREADS`; the report
does not claim an instrumented native pool size. `--case=<name>` selects one case.
Reports retain warmups, all samples, complete request/result fingerprints and
count/min/median/max. Cancellation contributes a timing only when accepted and
observed terminally cancelled without errors or partial output.

These are job-start-to-observed-terminal IPC timings, including scheduling,
serialization and 25 ms status polling. They are not core calculation timings,
frame latency or exact worker-termination timestamps. Keep builds, tests and other
native probes out of measured runs. Production binaries built directly with Cargo
need `--features tauri/custom-protocol` to embed assets and select production CSP.

On 2026-09-21, an unchanged production EXE (`d102f063152f7c59...`), built with
Rust 1.97 and Node 22.23.1, ran this Vanilla suite on a Ryzen 7 7800X3D/16 logical
CPUs. Each policy used one warmup and three repeats in separate fresh native
sessions, without competing builds/tests. Full requests and results matched all
64 samples including warmups; all 32 cancellation samples were conclusive.
Values below are median [min–max] milliseconds; cancellation rows start at the
cancel request, while other rows start at the calculation request.

| Native case | One Rayon thread | Default thread policy |
| --- | ---: | ---: |
| Locked non-additive solve | 6.35 [6.27–8.47] | 6.62 [6.40–7.02] |
| Search, K=500 | 1655.15 [1592.13–1687.00] | 3248.96 [3147.26–3334.13] |
| No-respec, 50 levels, two lanes | 57.19 [51.47–58.90] | 48.98 [47.37–58.52] |
| Optimum Envelope, 50 levels, two lanes | 60.31 [46.84–62.17] | 47.15 [43.25–48.80] |
| Search cancellation | 4.38 [4.16–4.64] | 4.12 [3.87–4.13] |
| Solve cancellation | 45.02 [43.34–45.79] | 44.18 [43.65–44.58] |
| No-respec cancellation | 130.56 [102.58–136.57] | 132.59 [131.55–133.19] |
| Envelope cancellation | 133.06 [131.81–134.32] | 134.39 [102.38–136.21] |

Concurrent manifest probes completed before the calculation/cancellation terminal
observation in every sample. Default-thread search was slower for this selected
low-level K=500 workload; this small sequential comparison does not establish a
general threading policy. The measurements predate the subsequent math experiments
and do not establish Convergence performance or a cancellation latency guarantee.

### Rayon policy investigation

On 2026-09-22, the existing phase harness was rebuilt from core source at `e34f300`
with warmup-result verification added, Rust 1.97.0, ThinLTO and one codegen unit.
The same release executable (`c6ffc35a8b57ae50...`) ran on the Ryzen 7 7800X3D,
without CPU affinity pinning or competing builds/tests. The observed default pool
contained 16 threads. Each policy used one warmup and three measured repeats in
each of two blocks, ordered 1/2/4/default and default/4/2/1. Completed affordable
cases were reused when the broader experiment was narrowed; expensive exploratory
cases occurred between some first-block cases, and their actual execution order
is retained. This is a bounded local comparison, not a randomized experiment.

All five requests and complete ordered results matched across 120 measured samples
and 40 warmups. Core/data inputs, compiler configuration and the executable hash
were unchanged. Values below are total core-phase median [min–max] milliseconds
across six measured samples per cell:

| Case | 1 thread | 2 threads | 4 threads | Default (16) |
| --- | ---: | ---: | ---: | ---: |
| Vanilla, RL46 Max AR, weapon grouping, K=500 | 724.19 [680.74–774.58] | 620.64 [616.62–667.22] | 522.48 [514.67–528.96] | 488.74 [462.77–510.72] |
| Convergence, same harness case | 489.29 [473.11–519.01] | 323.52 [315.60–326.22] | 211.48 [209.19–212.27] | 159.77 [157.56–164.22] |
| Vanilla, Katana Bleed then AR | 10.32 [9.95–10.76] | 8.70 [8.33–8.88] | 7.36 [7.14–8.43] | 6.58 [6.15–6.69] |
| Convergence, Katana Bleed then AR | 9.36 [9.10–12.23] | 6.25 [6.03–6.54] | 4.42 [4.21–4.55] | 3.44 [3.22–3.57] |
| Vanilla, locked War Cry, K=500 | 21.63 [20.60–22.42] | 21.92 [20.29–22.55] | 21.55 [20.30–22.43] | 22.38 [20.76–23.34] |

The first four cases use exact profile upgrade caps; the locked War Cry case
searches all upgrades. Convergence uses the harness's class-based budget, not the
desktop's fixed Custom-stats workflow. The historical native K=500 measurement
above used loadout grouping and IPC/job timing, so it is a different workload.

Exploratory higher-budget searches exposed the opposite tradeoff. In one block
with three samples per policy, Vanilla RL150 exact-upgrade Max AR took 630.79
[622.95–633.87] ms at one thread, 611.02 [596.47–620.81] at two, 9883.76
[9868.52–9899.87] at four, and 6308.05 [6108.58–6727.84] at the default 16.
Scoring accounted for the regression: its median rose from 49.71 ms at one thread
to 9472.71 at four. Complete results matched every policy. The corresponding
Convergence case improved from 422.77 to 176.00 ms at one/default threads.

The RL93 all-upgrade case took 1495.06/6680.49 ms in Vanilla and
1367.86/2347.71 ms in Convergence at one/two threads, with complete result parity
for those samples. The Vanilla four-thread process exceeded its 120-second cap
while attempting one warmup and three repeats. That is a censored process timeout,
not an individual sample duration or a parity pass. This broader matrix was
stopped and narrowed; its partial results and failure remain recorded.

The runtime policy remains unchanged. A universal one-thread cap would regress
the completed low-budget searches, while a universal four-thread cap would regress
the measured high-budget Vanilla case. Search currently enables parallel scoring
at one million combinations and two work units; one-thread preparation also
precomputes scalar AoW routes. These are useful profiling targets, not sufficient
evidence for a new threshold. Investigate expensive scoring/work partitioning
before proposing request-specific pool selection, then verify native latency,
Paths and cancellation on both profiles. Raw samples, full fingerprints, source
and compiler identity, execution order, scripts and timeout evidence remain under
`.codex-tmp/frontend-maintenance/threads/` (`bounded/summary.json` and
`exploratory-summary.json`).

## Release compiler settings

Both Cargo packages set `lto = "thin"` and `codegen-units = 1` for release builds.
The desktop package needs its own settings because Cargo does not inherit a
dependency's build profile. Local development builds and the baseline CPU target
are unchanged. CI selects test-profile optimization level 1 for core tests through
workflow environment variables, retaining debug assertions and overflow checks.
Backend tests keep the default test profile. These settings do not alter the
shipping release profile.

Windows test and release checks run concurrently. Within each job, both crates
share a Cargo target directory; packaging keeps its existing output paths.
CI links the library tests and the level-range example's assertions explicitly,
runs documentation tests, and checks the other targets without linking extra
release executables. When adding integration-test targets, include them in the
CI test commands. Cargo timing reports are retained as CI artifacts; compare
compilation, execution, cache transfer and total job time separately, on matching
commits and runner images. A fresh cache and a warm cache are distinct baselines.

Pull requests select checks from the actual merge diff. Documentation-only changes
run metadata validation and the required aggregate; frontend-only changes also run
the React lint, Rust–TypeScript contract, frontend unit, build and browser checks. Rust, data, tooling, dependency,
workflow and unknown paths run every check. Deletions and both sides of renames
are included. The aggregate rejects failed routing and unexpected skipped jobs.
Every push to `main` runs the complete suite, preserving exact-commit release
validation. Keep this conservative allowlist aligned with new cross-layer inputs.

A local comparison on 2026-09-22 used Rust 1.97.0, a Ryzen 7 7800X3D and four
build, test-harness and Rayon threads. The unchanged core suite plus benchmark
assertion test passed 222 tests (four ignored) at every optimization level.
One cold/warm run per level took 287.21/246.66 seconds at level 0,
254.98/33.69 at level 1, and 279.61/33.46 at level 2. Level 1 had the lowest cold
total despite longer compilation; these single samples do not establish hosted
runner speedups or a benefit for backend tests.

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

The original AR/bleed frontier reused ordinary exact optimization at each feasible
ARC, then removed dominated pairs. Six fixed Uchigatana/Seppuku cases (Keen, Blood,
Occult at levels 80 and 200, +25) took 0.57–8.53 ms including preparation, with one
warmup and five repeats on the same pinned thread. They returned 1–21 points with
identical repeat fingerprints. Small-budget regressions independently enumerate
all five combat stats, including floors, locks, nonmonotone curves and flat AR
ties. The shortlist, threshold, and plot do no further optimizer work.

The current implementation reuses one exact other-stat DP per additive route across
ARC slices, retaining each slice's feasible spend interval and complete tie order.
Coupled routes keep independent locked-ARC solves. A same-executable comparison
against the original solver used reversed baseline/candidate/candidate/baseline
blocks, one warmup and seven measured repeats per block (14 samples per variant),
both one Rayon thread pinned to logical CPU 6 and the machine's default pool.
The three Vanilla Uchigatana/Seppuku +25 requests retained identical complete
per-ARC winners and frontiers:

| Request | One-thread before / after median (ms) | Default before / after median (ms) |
| --- | ---: | ---: |
| Blood, RL80 | 0.540 / 0.287 | 0.685 / 0.274 |
| Blood, RL200 | 8.541 / 0.784 | 9.142 / 0.781 |
| Keen, RL80 | 0.351 / 0.108 | 0.390 / 0.110 |

Candidate ranges were below baseline ranges in all six comparisons. The bounded
request-local storage is one route DP with at most 397 spend slots and at most 100
ARC winners. Process peak working set was snapshot-dominated near 540 MB; it did
not establish precise algorithm allocation cost. Selected core cancellation probes
overshot their requested deadline by 0.0009–0.0048 ms; this is not a native latency
guarantee. Independent five-stat enumeration tests cover both profiles, locks,
requirements, decreasing curves, flat ties and large budgets. These are exact-model
and selected-workload results, not independent gameplay or general desktop speed
claims. Raw samples, hashes and the retained executable are local evidence under
`.codex-tmp/math-experiments/m01/`.

The proposed tied-primary traversal for coupled routes was rejected after bounded
profiling. Across 240 compatible Lifesteal Fist Blood/Occult requests, all 160
certified AR/bleed primary optima were unique and already used the existing shortcut.
The 80 AoW-primary cases lacked the required certified additive prefix. Complete
scored results matched unrestricted enumeration; no new traversal or speed claim
was justified. Each request had at most 300 allocations, so this does not establish
uniqueness for other budgets or stat constraints. The retained source patch and
profile records are under `.codex-tmp/math-experiments/m03/`.

### Full-route coefficient aggregation

Certified additive full-route formulas now combine identical stat/curve terms,
separating raw and effective Strength handling. First-positive-hit evaluation
retains the original hit order; coupled or floored terms are not combined.
A same-executable release comparison against the exact previous per-hit loops
used 400 stat inputs per route to evaluate first/full damage and all five stat
delta tables. It used one warmup and seven repeats per block in the same reversed
four-block order and one/default thread policies described above:

| Route | One-thread before / after median (ms) | Default before / after median (ms) |
| --- | ---: | ---: |
| Wild Strikes | 1.052 / 0.369 | 1.051 / 0.372 |
| War Cry | 0.837 / 0.703 | 0.842 / 0.711 |
| Storm Blade | 7.481 / 3.141 | 7.579 / 3.226 |
| Unsheathe, one hit | 0.491 / 0.488 | 0.492 / 0.494 |

All exact first/full values and delta tables matched across variants and policies.
Multi-hit candidate ranges stayed below baseline ranges; the one-hit ranges
overlapped (-0.57%/+0.49% median change). Complete formula compilation was measured
separately at 0.0017–0.01075 ms median, below the multi-hit savings in these workloads.
This supports the retained aggregation at formula/table scope, not a whole-search
speed claim. Real-route differential tests separately cover full ranked winners,
route IDs and detailed outputs, both Strength transforms through effective STR148,
zero multipliers and non-additive rejection. Existing normalization and checked
overflow tests remain green. Raw samples, source/compiler hashes and the retained
executable are under `.codex-tmp/math-experiments/m02/`.

### Workloads

| Harness | Workload |
| --- | --- |
| `benchmark_optimizer_phases` | Sixteen release-mode optimizer cases spanning Max AR, Max Physical AR, Bleed then AR, AoW first hit, and AoW full sequence, including weapon/loadout grouping at K=25 and K=500 and locked-stat route cases. It reports preparation, scoring, materialization, medians, samples, row counts, equivalent combinations, profile/model identity, and complete request/result fingerprints. |
| `benchmark_workflows` | Vanilla Paths in No-respec and Optimum Envelope modes at 10, 50, and 200 levels with one and two distinct lanes; Affinity Watch at the same horizons; and the standard 26-point upgrade series. The Rust tests perform one warmup and retain best, median, worst, and every sample. |
| `benchmark_level_range` | The documented command measures Uchigatana at level 80 plus offsets 0, 10, 50, and 200 for Keen, Blood, and Occult. The CLI defaults to 10, 50, and 200. It compares independent per-level optimization with shared range evaluation and rejects any difference in complete ordered results. Use `--all-affinities` for every available Uchigatana affinity. |
| `native-responsiveness.mjs` | Analysis suite: packaged solves, upgrade series, AR/bleed frontier and cancellation. Production suite: locks/non-additive routes, K=500, both Paths modes and cancellation. Both include a concurrent manifest probe and retain executable/manifest identity, requests, complete fingerprints, warmups, samples and timing ranges. |

Phase cases use profile rules for upgrade caps: Vanilla standard +25/Somber +10;
Convergence standard and Somber +15. Exact-upgrade and all-upgrades searches are
different workloads; `all-upgrades-max-ar-high-level` uses the broader level-93,
25-row case. Convergence damage-objective cases are skipped when the profile does
not declare AoW damage support and fail when selected explicitly. Convergence phase
cases use the harness's class-based budget, not the fixed Custom-stats UI workflow.

### Ordered result materialization

Scoring already returns grouped, bounded results in final rank order. Materialization
preserves the exact ranking keys and tie fields, so it now converts that batch in
order instead of inserting each hydrated result into another top-K list. Candidate,
route and final-return cancellation checks remain in place. Tests compare complete
outputs against the previous insertion algorithm for both grouping modes, K=0/1/25/500,
exact and displayed-score ties, and serial, parallel and reused scoring.

On 2026-09-21, four Vanilla low-level Max AR cases were measured in separate release
builds on a Ryzen 7 7800X3D, one Rayon thread pinned to logical CPU 6. Two blocks per
variant used one warmup and seven samples each, in baseline/candidate/candidate/baseline
order, without competing builds or tests. The same compiler configuration, dataset,
requests and complete ordered outputs were verified; only optimizer and test source
differed between the two dirty working-tree fingerprints.

| Grouping / K | Baseline materialization median (range), ms | Ordered conversion median (range), ms |
| --- | --- | --- |
| Weapon / 25 | 0.358 (0.348–0.401) | 0.345 (0.330–0.423) |
| Weapon / 500 | 12.233 (11.942–12.641) | 6.952 (6.775–7.045) |
| Loadout / 25 | 0.417 (0.396–0.447) | 0.396 (0.382–0.635) |
| Loadout / 500 | 13.189 (13.059–13.592) | 7.474 (7.381–7.889) |

K is the requested limit; the K=500 weapon case returned 477 distinct weapon
groups, and the loadout case returned 500 rows.

The simpler conversion is retained for its repeatable roughly 43% reduction in the
K=500 materialization phase. K=25 differences overlap timing variation. Preparation
dominates these searches; total medians changed by only 0.1–3.1%, which is not a
portable whole-search speed claim. These are single-machine core measurements,
not packaged desktop latency or Convergence performance evidence. Local raw samples,
source fingerprints, commands, compiler settings, executable hashes, patches and
parity checks are retained under `.codex-tmp/perf01/` (`run.py`, `verification.json`).

### Selected route details

The optimizer now asks the shared route evaluator to build action/hit records only
for the selected route. It still evaluates all shared rows and assignments and
projects every route's exact totals; filtering earlier could hide an error in an
unselected route. Public all-route evaluation is unchanged. Complete-payload tests
cover six real skills, buffs, warnings, status, poise, stamina, native skills and
ordering, plus errors from unselected rows, buff assignments and aggregate overflow.
Selected-route cancellation is checked at every observed checkpoint.

The 2026-09-21 A/B comparison uses locked level-101 Claymore requests with all five
combat stats at 20. Wild Strikes and War Cry search all allowed affinities and
upgrades; requested K=500 returns 338 loadouts. Flame Skewer at Fire +25 is a
single-route control. Builds use the same compiler configuration, Vanilla data and
complete requests, on one Rayon thread pinned to logical CPU 6 of the same Ryzen
7 7800X3D. All complete ordered outputs match.

Initial one-warmup/seven-sample blocks had substantial timing variation and were
retained as inconclusive. A confirmation using the same binaries ran five warmups
and 31 samples per block, twice per variant in baseline/candidate/candidate/baseline
order, with no competing builds or tests. The 62-sample confirmation results are:

| Case / returned rows | All-route details median (range), ms | Selected details median (range), ms |
| --- | --- | --- |
| Wild Strikes / 25 | 1.155 (1.111–1.301) | 1.132 (1.072–1.218) |
| Wild Strikes / 338 | 12.810 (12.536–14.070) | 12.397 (12.187–13.400) |
| War Cry / 338 | 13.137 (12.838–13.816) | 12.075 (11.883–12.463) |
| Flame Skewer / 1 | 0.0132 (0.0128–0.0214) | 0.0131 (0.0124–0.0206) |

Retained for the roughly 8% War Cry materialization reduction, which exceeded
observed variation in the confirmation. Wild Strikes medians improved 2–3.2% with
overlapping ranges; the control is effectively unchanged. Total-time changes range
from a 4.7% reduction to a 1.1% increase, so these measurements do not establish a
general whole-app speedup. Raw initial and confirmation samples, commands, source
patches/fingerprints, compiler settings, executable hashes and full-output checks
are under `.codex-tmp/perf02/`; `verification-blocks-3-4.json` records the confirmation.
Baseline compilation reused the preflight build cache; build durations are not a
clean-build comparison. Native latency, production thread counts and Convergence
performance remain separate measurements.

### Comparable inputs and result checks

Record the release/debug profile, source revision and dirty state, executable hash
when available, snapshot manifest schema/dataset/model identity, profile, Rust and
runner versions, host/CPU, Rayon thread count, normalized requests, and upgrade
policy. When comparing locally built binaries, use separate `CARGO_TARGET_DIR`
values so one build cannot overwrite the other. Keep warmups and repeated samples
consistent, reverse build order, and do not run builds or tests during timed samples.

The phase and workflow reports retain build provenance under `metadata.build`.
`source.fingerprint` identifies the commit and scoped working-tree file bytes,
including relevant untracked inputs and deleted tracked files. `source.dirty`
describes that scope; local audit evidence and generated output are excluded.
`compiler_variant_fingerprint` separately identifies the toolchain, Cargo release
profile and compiler configuration inputs. Changing LTO, codegen units or Rust
flags at the same commit therefore produces a different compiler identity.
The invocation retains the command and output directory; changing the repeat count
or target directory alone does not define a new compiler variant. Raw samples and
complete result fingerprints remain in their existing case records.

The collector follows Cargo's configuration search from the command's working
directory, including ancestor directories and Cargo home. It retains only relevant
compiler settings and environment overrides, never the general environment or
registry credentials. The resolved release-profile table is a Cargo configuration
record, not a reconstructed rustc command: Rust flags can override code generation,
target `cfg` conditions are retained without evaluating them, and test builds have
Cargo's test-specific behavior. Unsupported configuration forms fail explicitly
instead of silently reporting default settings. See Cargo's
[configuration precedence](https://doc.rust-lang.org/cargo/reference/config.html)
and [profile rules](https://doc.rust-lang.org/cargo/reference/profiles.html).
Both drivers recheck source and compiler fingerprints after execution and discard
measurements if those inputs changed during the run.

Compare medians only after checking the full ordered result fingerprint. Include
secondary metrics, stat allocations, and extra rows; a changed winner or fingerprint
requires a correctness review before accepting a faster timing. Range comparisons
must check independent and shared results for every sample. Historical f32 results
are context only because exact-v1 and earlier floating-point builds do not share a
gameplay and ranking contract. Paths reports retain the complete ordered path DTOs,
including solved-build routes and every step's level, stats, metric and requirement
gap. Every warmup and measured repeat must match. Baseline comparisons require
matching requests and complete results; older Paths reports without these fields
cannot establish parity. Affinity Watch and upgrade-series workflow timings still
need their separate equivalence and smoke checks.

The Paths timer accepts already-solved requests. Fixture searches, request cloning,
serialization and repeat comparisons are outside the measured interval. The
level-80 Samurai fixtures use exact upgrades: Keen Uchigatana/Unsheathe at +25,
and Standard Bloodhound's Fang/Bloodhound's Finesse at +10. One lane uses Uchigatana;
two lanes calculate these different loadouts sequentially. Reports retain the full
input requests and original sample order. This measures native path calculation,
not job scheduling, IPC, rendering or cancellation responsiveness; use the separate
native probe for those observations.

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
