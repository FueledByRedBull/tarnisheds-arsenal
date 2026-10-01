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

Historical native timings, including sample ranges, cancellation coverage and
measurement limits, remain in the [September 21 record](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#analysis-workflows).
They describe that executable and workload, not a current performance baseline.

### Rayon policy investigation

The [September 22 investigation](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#rayon-policy-investigation) retains
the one/two/four/default-thread matrix, complete-result parity, the censored
timeout and local evidence paths. Its workload-dependent results do not justify
a universal thread cap; the runtime policy remains unchanged.

### Cross-worker exact top-K cutoff

The [optimizer overview](design/optimizer-overview.md#exact-scoring) describes
the shared exact cutoff and tie-preserving pruning. The
[September 23 record](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#cross-worker-exact-top-k-cutoff) preserves
before/after core and native measurements, outliers, result fingerprints,
calculation-audit scope and the packaged-check boundaries.

## Correctness follow-up (2026-09-26)

The [archived correction record](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#correctness-follow-up-2026-09-26)
retains the original external comparisons, raw-data checks, failing-case
coverage and evidence paths. Current calculation semantics and known limits
live in the [model reference](model-reference.md#known-reference-differences).
Timings made before a model correction are not same-contract comparisons.

## Review follow-up measurements (2026-09-29)

These check the v0.16.0 changes the prioritised review asked to measure. The
baseline is the `v0.15.0` tag (`f1cdefd`) with the v0.16.0 catalog, loader and
Affinity Watch byte-count benchmarks copied in unchanged; the candidate is
`2660f35`. Both ran with `RAYON_NUM_THREADS=1`, `ER_BENCH_REPEATS=5`, Rust
1.97.0 and release test builds, one after the other on an AMD Ryzen 7 7800X3D
desktop with no builds running. That is not a dedicated runner, and the two
versions load different snapshots (`v8` and `v9`), so timings are indicative;
byte counts and compatibility-check counts are exact.

| Measurement | v0.15.0 | v0.16.0 |
| --- | --- | --- |
| Affinity Watch payload, 10 / 50 / 200 levels | 599,192 / 2,592,239 / 10,062,342 bytes | 55,442 / 71,919 / 135,007 bytes |
| Affinity Watch serialization, 200 levels (median) | 13.99 ms | 0.22 ms |
| Affinity Watch calculation, 200 levels (median, range) | 2,202 ms (1,915–2,323) | 2,142 ms (1,951–2,300) |
| Catalog compatibility checks, Vanilla / Convergence | 764,440 / 770,064 | 382,220 / 385,032 |
| Catalog build, Vanilla (median, range) | 124.6 ms (123.4–128.6) | 99.9 ms (93.0–123.9) |
| Catalog build, Convergence (median, range) | 246.1 ms (225.0–336.7) | 182.4 ms (144.0–228.9) |
| Cold snapshot load, Vanilla / Convergence (median of 6) | 165.2 / 102.2 ms | 155.4 / 99.2 ms |

Affinity Watch points now carry only a level and metric, which removes about 99%
of the maximum-horizon payload without changing calculation time. The catalog
makes one compatibility decision per weapon and Ash instead of two. Snapshot load
differences are within the sample ranges and are not claimed as an improvement.
Paths and upgrade-series medians stayed below 16 ms in both versions.

Compare was timed in the release desktop build from clicking **Compare** to the
first verified comparison and to all upgrade charts, with three pins (four lanes),
one warmup and five fresh launches driven through the packaged-smoke harness. The
first comparison appeared after a median 69 ms (62–82) and all charts after 97 ms
(80–106). v0.15.0 showed nothing until every chart finished, so its first result
matched the all-charts time. Charts share one native queue, so the gap should grow
with more lanes; only the three-pin case was measured.

## Responsiveness work (2026-10-01)

Measured on the same AMD Ryzen 7 7800X3D desktop with no builds running, release builds,
default Rayon threads (16) unless stated. Every optimizer change kept complete ordered
results: the phase runner reported identical results for all 16 cases before and after.

| Phase-runner case (median total) | Before | After |
| --- | --- | --- |
| `open-ranking-max-ar` | 432.5 ms | 83.1 ms |
| `open-ranking-max-ar-high-level` | 907.3 ms | 91.7 ms |
| `open-ranking-max-ar-export-500` | 785.2 ms | 121.5 ms |
| `all-upgrades-max-ar-high-level` | 885.0 ms | 214.1 ms |
| `locked-war-cry-export-500` | 37.0 ms | 18.9 ms |

- **Preparation** was 94% of an open search. Weapons now prepare in parallel chunks of
  64 per Rayon thread, and each Ash's attack rows are selected once per weapon type. One
  thread prepares the high-level case in 383 ms instead of about 880 ms.
- **Bleed searches** sometimes took 25 to 44 s instead of about 1.4 s: one weapon with
  18 Ashes was solved at every upgrade before a cutoff existed. Units now visit upgrades
  highest first, and per-weapon grouping skips setups strictly below the weapon's best
  found so far. A level-129 bleed top-50 search hit 2 slow runs in 12 before; after, 60
  runs stayed at or under 1.61 s with one result hash, matching the unchanged scorer.
- **Cancellation** during preparation took 92 to 193 ms on one thread with 1,024-weapon
  chunks; per-thread chunks bring it to 5 to 20 ms (4 to 23 ms on 16 threads). A request
  landing in the last ~100 ms of a search is observed at completion, as before.
- **Polling** now starts at 8 ms and caps at 50 ms (25 ms after progress). A 1.5 s bleed
  search used 24 status calls per second with no measurable slowdown against the old
  5 per second; polling every millisecond (318 per second) slowed the same search by 19%.

Desktop interactions were timed as the longest renderer main-thread task, 8 repeats at
1650x950 with 50 ranked rows (a 120 Hz frame is 8.3 ms):

| Interaction (median / worst) | Before | After |
| --- | --- | --- |
| Reverse the sort | 33.0 / 42.3 ms | 18.3 / 19.4 ms |
| Select a row | 12.8 / 14.0 ms | 6.3 / 8.1 ms |
| Open and close the palette | 24.0 / 27.0 ms | 2.5 / 7.0 ms |
| Return to Rankings | 34.4 / 36.3 ms | 16.7 / 18.6 ms |
| Open Compare | 21.1 / 27.6 ms | 14.9 / 19.0 ms |

The palette is a popover rather than a modal (a modal restyled ~2,900 elements to make
the page inert), rows are memoised, workspaces stay mounted while hidden, the board uses
a media query instead of a container query, and metric tokens are blocks instead of
grids. The first Ctrl+K fell from 335 ms to about 10 ms by mounting the palette at idle.

### Motion budget

Motion was held to the rendering work of the build before it. Timings swing by 30% between
runs on one machine, so each change was also checked by counting work per interaction:
elements restyled, objects laid out and paints in the trace. Layout counts match the
earlier build (opening Compare: 854 objects against 846), and interleaved timings stayed
within run-to-run noise. The added paints come from the intended colour animations:
selection, undo flashes and the search's landing flare. Rules found along the way:

- **No per-row motion.** Sliding each moved row (FLIP) doubled a sort to 37 ms and cost
  ~30 ms on the next frame. Every moving row became a layer, and so did its sticky cells.
  A re-rank now animates the rows group as one layer.
- **Keep the rows group composited.** Starting an animation on it changed its stacking
  context and laid out all 50 rows again (+7 ms). Without a permanent layer, the 50 sticky
  rank cells became overlap layers, costing ~6 ms of layerization on every frame.
  `will-change: opacity` avoids both.
- **Read layout in the next frame, not in a commit.** Measuring row offsets or the tab
  pill inside React's commit forced a layout. After results or a tab switch, effects
  changed the page again, so it was laid out twice: 55 ms instead of 29 ms, or 1,428
  objects instead of 854. Such reads now run in `requestAnimationFrame`.
- **Transform and opacity only.** Progress fills, the tab pill and skeleton shimmers move
  by transform. A width transition laid out on every frame, and a `background-position`
  shimmer repainted every placeholder on every frame.

### Click hitches

Measured on the app as users run it: GPU compositing on, at the display's 240 Hz. The
packaged smoke harness passes `--disable-gpu`, so its frames use the software compositor.
Launch the release exe normally with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` set to a
remote-debugging port and `WEBVIEW2_USER_DATA_FOLDER` set to a scratch folder instead.
Headless Chromium also misleads: its overlay scrollbars hide the first cause below.

| Click (longest main-thread task, median) | Before | After |
| --- | --- | --- |
| Reverse a column sort | 18.5 ms | 7.7 ms |
| Select a row | 7.5 ms | 6.6 ms |
| Return to Rankings | 18.3 ms | 5.5 ms |
| Open Compare | 20.9 ms | 8.5 ms |
| Open Paths | 6.5 ms | 3.7 ms |

- **Reserve scrollbar gutters.** With classic Windows scrollbars, laying out a container
  whose scrollbar might change laid out all of its contents again: 6.5 ms for a sort that
  dirtied 17 objects, against 0.2 ms with `scrollbar-gutter: stable`.
- **Sort without moving rows.** Moving 50 row nodes restyled ~3,000 elements and laid out
  every row. Rows now stay in rank order and a sort offsets them with `top` and sets
  `reading-order` for keyboard and screen readers. CSS `order` re-laid out every row,
  and transforms gave each sticky rank cell its own layer (60 layers instead of 15).
- **Keep Rankings rendered while hidden.** Activity hides workspaces with
  `display: none`, so returning re-laid out all 50 rows. Rankings now sits in the same grid
  cell as the others and uses `content-visibility: hidden`; it cancels exports on leaving
  itself, as the runtime invariants require.
- **Re-render only what a switch changes.** The tab bar, notices and stage subscribe to
  the active workspace, so a switch no longer re-renders the query strip and Build
  Detail. Compare skips its comparison when its inputs match the last finished one.
  `compactNumber` reuses one `Intl.NumberFormat`; it was constructed 1,000 times per 50
  rows.

Results arriving still take ~29 ms: building 50 new rows restyles ~3,200 elements.

### Memory

Private working set of the whole process tree (the app and every WebView2 process), with
GPU compositing on:

| Stage | Before | After |
| --- | --- | --- |
| Idle after start | 1,139 MB | 182 MB |
| After a 50-row search | 1,196 MB | 246 MB |
| After Compare, Paths and Affinity Watch | 1,252 MB | 282 MB |
| After 10 more searches | 1,334 MB | 344 MB |

The native process held ~1,000 MB: each profile kept its attack-element corrections in a
table indexed by param id. Ids reach ~20 million but only ~190 exist, so each profile
allocated 482 MB. They are now a sorted list searched by id, which returned the same 50 rows
for an identical search at the same speed, and launch-to-loaded time fell from ~1.0 s
to ~0.85 s. WebView2's renderer grows during repeated searches because discarded rows
are collected lazily: a forced collection returned it from 176 MB to 101 MB.

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

The [compiler experiment record](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#release-compiler-settings) preserves
the local test-profile, ThinLTO/codegen-unit and PGO comparisons, compiler/CPU
identity, full-result parity and limitations. PGO is not enabled for releases;
those single-machine measurements do not establish hosted-CI or whole-app gains.

See [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) and
the [Rust PGO workflow](https://doc.rust-lang.org/rustc/profile-guided-optimization.html).

## Exact scoring measurements

The [numerical contract and identity](model-reference.md#numerical-contract-and-identity)
defines the loaded data and ranking semantics. The bounded exact DP may skip
unreachable destinations and additions, but must retain sparse allowed choices,
unused budget, the full feasible stat-spend interval, and the complete tie order.

Recorded bounded-DP and range improvements, with their matching-contract limits,
are in the [v0.14.0 verification record](release-notes/v0.14.0.md#verification)
and the [original scoring measurements](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#exact-scoring-measurements).

### Fixed-loadout reuse and frontier experiments

Current reuse eligibility and frontier construction are described in the
[optimizer overview](design/optimizer-overview.md#reuse-across-workspaces). The
[experiment record](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#fixed-loadout-reuse-and-frontier-experiments) retains
all timing tables, parity checks, rejected alternatives, storage bounds and
local evidence paths; these are selected-workload results, not gameplay proof.

### Full-route coefficient aggregation

See the [current aggregation rules](design/optimizer-overview.md#exact-scoring)
and the [recorded formula/table comparison](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#full-route-coefficient-aggregation),
which preserves exact-value parity, compilation cost and timing variability.

### Workloads

| Harness | Workload |
| --- | --- |
| `benchmark_optimizer_phases` | Sixteen release-mode optimizer cases spanning Max AR, Max Physical AR, Bleed then AR, AoW first hit, and AoW full sequence, including weapon/loadout grouping at K=25 and K=500 and locked-stat route cases. It reports preparation, scoring, materialization, medians, samples, row counts, equivalent combinations, profile/model identity, and complete request/result fingerprints. |
| `benchmark_workflows` | Vanilla Paths in No-respec and Best per level (`optimum_envelope`) modes at 10, 50, and 200 levels with one and two distinct lanes; Affinity Watch at the same horizons; and the standard 26-point upgrade series. The Rust tests perform one warmup and retain best, median, worst, and every sample. |
| `benchmark_level_range` | The documented command measures Uchigatana at level 80 plus offsets 0, 10, 50, and 200 for Keen, Blood, and Occult. The CLI defaults to 10, 50, and 200. It compares independent per-level optimization with shared range evaluation and rejects any difference in complete ordered results. Use `--all-affinities` for every available Uchigatana affinity. |
| `native-responsiveness.mjs` | Analysis suite: packaged solves, upgrade series, AR/bleed frontier and cancellation. Production suite: locks/non-additive routes, K=500, both Paths modes and cancellation. Both include a concurrent manifest probe and retain executable/manifest identity, requests, complete fingerprints, warmups, samples and timing ranges. |

Phase cases use profile rules for upgrade caps: Vanilla standard +25/Somber +10;
Convergence standard and Somber +15. Exact-upgrade and all-upgrades searches are
different workloads; `all-upgrades-max-ar-high-level` uses the broader level-93,
25-row case. Convergence damage-objective cases are skipped when the profile does
not declare AoW damage support and fail when selected explicitly. Convergence phase
cases use the harness's class-based budget, not the fixed Custom-stats UI workflow.

### Ordered result materialization

The [materialization design](design/optimizer-overview.md#ranking-and-materialization)
preserves rank order and cancellation. Its
[September 21 comparison](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#ordered-result-materialization) retains
K/grouping-specific measurements and complete-output checks.

### Selected route details

The [selected-route contract](design/optimizer-overview.md#ranking-and-materialization)
preserves errors from unselected routes while limiting detailed output. The
[measurement record](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/f1cdefd913caad2ea960194129d44a7de85054bf/docs/performance.md#selected-route-details) retains both the
inconclusive initial samples and the larger confirmation, with full-output
parity and whole-search limits.

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
