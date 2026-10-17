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
concurrent manifest access, and asynchronous cancellation. `startupMs` runs from
the launch attempt that succeeded, so a retried WebView2 start does not inflate it,
until the model and manifest are ready; it is outside measured work. A local EXE
does not need to be a published or signed MSI.

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

## Desktop responsiveness records

The v0.16.0 and v0.17.0 measurements, with their tables, machines and limits, stay
in the tagged guide: the [review follow-up](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.17.2/docs/performance.md#review-follow-up-measurements-2026-09-29)
(Affinity Watch payloads, catalog build, Compare first result), the
[responsiveness work](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.17.2/docs/performance.md#responsiveness-work-2026-10-01) (optimizer
preparation, bleed searches, cancellation and polling), the
[motion budget](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.17.2/docs/performance.md#motion-budget), [click hitches](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.17.2/docs/performance.md#click-hitches) and
[memory](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.17.2/docs/performance.md#memory).

These rendering rules came out of that work and still apply:

- No per-row motion. A re-rank animates the rows group as one layer.
- Keep the rows group composited (`will-change: opacity`), so starting an
  animation neither re-lays out the rows nor turns sticky rank cells into layers.
- Read layout in the next animation frame, never inside a React commit.
- Animate transform and opacity only.
- No motion on keyboard-summoned surfaces such as the Ctrl+K palette.
- Fade `<details>` content by opacity; never transition its visibility.
- Reserve scrollbar gutters (`scrollbar-gutter: stable`) on scroll containers.
- Sort ranking rows by offsetting them, not by moving DOM nodes or using CSS `order`.
- Hide an offstage workspace with non-inherited properties. Toggling inherited
  `visibility` on Rankings restyled its ~1,500 rows' elements on every tab switch.
- Reveal workspaces in a deferred render (`useDeferredValue`), so a tab key press
  is not followed by an 11-13 ms task.
- Do not put `content-visibility: auto` on ranking rows: rendering rows as they
  scroll in raised wheel-scroll p95 from 4.3 ms to 12.5-16.7 ms.
- Never run an animation inside an element that is itself animating or transitioning:
  Chromium then ticks the inner one on the main thread every vsync.
- No explicit `inherit` on the cells of rows that move: one inherited child makes every
  change to the row restyle all of its children.

Measure interactions with GPU compositing on: the packaged smoke harness passes
`--disable-gpu`. Launch the release exe with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
set to a remote-debugging port and `WEBVIEW2_USER_DATA_FOLDER` set to a scratch
folder instead.

### Frame pacing (2026-10-17)

Target: 120 fps, and 144 fps on 144 Hz displays. A scenario passes when its frame
intervals have p95 ≤ 6.94 ms and p99 ≤ 8.33 ms and no interval spans more than
three 144 Hz frames.

Method: release exe (`npm run tauri -- build --no-bundle`) launched normally with
the variables above, then Playwright `connectOverCDP`. A `requestAnimationFrame`
loop records frame times during each scenario while a CDP trace (`devtools.timeline`,
`cc`, `blink` and their `disabled-by-default` timeline categories) attributes every
main-thread task longer than 6.94 ms to script, style, layout, paint or compositing;
invalidation-tracking traces name the elements. Key latency is the time from the
`keydown` timestamp to the start of the frame after the one that handled it. Click
targets are resolved before a scenario and clicked by coordinates: Playwright locators run
accessibility queries and hit-target checks on the page's main thread, which otherwise
land inside the measured frames. Reuse
one `WEBVIEW2_USER_DATA_FOLDER` and clear only its `Local Storage`, `Session Storage`
and `IndexedDB` between launches: a fresh folder recompiles GPU shaders, and Affinity
Watch's first chart then stalls the GPU process for about 63 ms, which a returning
player never sees.

Machine: Ryzen 7 7800X3D (16 threads), Radeon RX 7800 XT, 2560x1440 at 240 Hz, so an
unbroken run reads 4.2 ms; window 1650x950; Vanilla level 192 with 50 rows. Before is
`a7fe55c` (built from a clean worktree), after adds the four frontend commits of this
work with the same core; medians of three runs each. Other agents' builds kept the CPU
60-95% busy during most runs, so single maxima vary by one or two 4.2 ms frames. "Busy" scenarios run during an
AoW first hit search over every weapon (longer than 40 s on this core).

| Scenario (Vanilla) | Before p50 / p95 / p99 / max, ms | After | Key p95, ms |
| --- | --- | --- | --- |
| Wheel and scrollbar drag, sorted and unsorted | 4.2 / 4.3 / 4.3 / 4.4 | 4.2 / 4.3 / 4.3 / 4.4 | - |
| Sort header clicks | 4.2 / 4.3 / 4.3 / 25.0 | 4.2 / 4.3 / 4.3 / 12.5 | - |
| Ctrl+1..4 cycling | 4.2 / 4.3 / 4.4 / 29.2 | 4.2 / 4.3 / 4.4 / 16.7 | 24.6 → 7.6 |
| First Compare open with 4 pins | 4.2 / 4.3 / 4.3 / 29.1 | 4.2 / 4.3 / 4.3 / 12.5 | 30.1 → 5.7 |
| Tab switches during Affinity Watch | 4.2 / 4.3 / 4.3 / 12.6 | 4.2 / 4.3 / 4.3 / 8.4 | 16.1 → 8.1 |
| Shortcuts dialog open and close | 4.2 / 4.3 / 4.4 / 24.9 | 4.2 / 4.3 / 4.4 / 16.8 | 15.9 → 15.4 |
| Popovers, palette, searchable select, stat typing | 4.2 / 4.3 / ≤ 4.3 / ≤ 12.6 | 4.2 / 4.3 / ≤ 8.3 / ≤ 8.4 | 7.9-9.9 |
| Row selection with Build Detail | 4.2 / 4.3 / 8.3 / 8.4 | 4.2 / 4.3 / 8.4 / 12.5 | - |
| Paths, Affinity Watch, Compare scroll | 4.2 / 4.3 / ≤ 4.4 / ≤ 8.2 | 4.2 / 4.3 / 4.3 / ≤ 8.3 | - |
| Window resize (code unchanged) | 4.2 / 12.5 / 12.6 / 20.9 | 4.2 / 12.5 / 16.6 / 20.9 | - |
| Busy: idle, wheel, tabs, palette, typing | 4.2 / ≤ 4.3 / 4.3 / ≤ 8.4 | 4.2 / 4.3 / 4.3 / ≤ 8.4 | 8.0-8.6 |
| Busy: popovers | 4.2 / 4.3 / 4.3 / 16.6 | 4.2 / 4.3 / 4.3 / 12.5 | 7.8 → 7.4 |
| Busy: window resize | 4.2 / 16.7 / 20.8 / 25.0 | 4.2 / 16.7 / 20.8 / 29.2 | - |

During that search, main-thread frames fall from 240 to 63-98 a second (267 to 91-119 ms
of main-thread time per second) once the forge bar's sheen runs outside its gliding fill.
Sort clicks restyle about 50 elements in 0.7 ms instead of about 400 in 8.5-9 ms, and the
total style time of the row-selection scenario falls from 106-168 ms to 33-62 ms.

Convergence matches Vanilla except that it has no Compare, Paths or Affinity Watch
and no objective that keeps a search running, so its busy scenarios follow a short
search. Search status payloads are at most 182 KB for 50 rows and IPC replies take
under 5 ms, so native transfer is not a frame-rate bottleneck.

Remaining misses, measured:

- Window resize cannot reach 120 fps without a visual change. Each width change costs
  4-5 ms of style and layout, 5-7 ms of paint, about 2 ms of pre-paint and 1 ms of
  layerizing; the ranking rows are about 90% of it and their stat token grids about half
  of that. Collapsing each token to one element, flex instead of grid, or inline blocks
  measured no gain; only showing the tokens as plain text (a different look) brought
  style and layout to about 2 ms, still above budget with paint. Crossing the 1500 px
  breakpoint also restyles all ~2,800 elements once (8-10 ms).
- The shortcuts dialog keeps its modal semantics. `showModal()` costs 8-13 ms: two
  full-document restyles of about 2,900 elements as the page becomes inert. Removing the
  backdrop styling changed nothing; a popover with an `inert` app root still restyles
  (9.8 ms); only a non-modal `show()` is cheap (1.6 ms).
- Returning to Rankings is one 8.6-9.9 ms task (its rows repaint after
  `content-visibility`, about 4 ms, plus pre-paint and layerizing), so a tab cycle still
  shows one 12-17 ms interval. Revealing the other workspaces now takes at most 5 ms.
- A sort click is now a 7-12 ms task (forced layout to measure rows, 2-3 ms of
  pre-paint) followed by a 4-5 ms repaint, within the pass rule in all three runs.
- Selecting a row is a 7-11 ms task (about 3 ms of script and 2-3 ms of layout for Build
  Detail), at the edge of the p99 limit.
- During a search, opening a popover can take a 9-15 ms task (5-8 ms of script).

## Release compiler settings

Both Cargo packages set `lto = "thin"` and `codegen-units = 1` for release builds.
The desktop package needs its own settings because Cargo does not inherit a
dependency's build profile. Local development builds and the baseline CPU target
are unchanged. CI selects test-profile optimization level 1 for core tests through
workflow environment variables, retaining debug assertions and overflow checks.
Backend tests keep the default test profile. These settings do not alter the
shipping release profile.

CI job selection and test profiles are described in the
[release guide](releasing.md#package-and-signing-checks). Cargo timing reports are
retained as CI artifacts; compare compilation, execution, cache transfer and total
job time separately, on matching commits and runner images. A fresh cache and a
warm cache are distinct baselines.

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
| `benchmark_optimizer_phases` | Twenty-six release-mode optimizer cases spanning Max AR, Max Physical AR, Bleed then AR, AoW first hit, and AoW full sequence, including weapon/loadout grouping at K=25 and K=500, locked-stat route cases, two-handing, levels 9 and 713, open-weapon skill objectives, the locked non-additive Lifesteal Fist solve, and Convergence's all-locked Custom stats ranking. It reports preparation, scoring, materialization, medians, samples, row counts, equivalent combinations, profile/model identity, and complete request/result fingerprints. |
| `benchmark_workflows` | Vanilla Paths in No-respec and Best per level (`optimum_envelope`) modes at 10, 50, and 200 levels with one and two distinct lanes; Affinity Watch at the same horizons; and the standard 26-point upgrade series. The Rust tests perform one warmup and retain best, median, worst, and every sample. |
| `benchmark_level_range` | The documented command measures Uchigatana at level 80 plus offsets 0, 10, 50, and 200 for Keen, Blood, and Occult. The CLI defaults to 10, 50, and 200. It compares independent per-level optimization with shared range evaluation and rejects any difference in complete ordered results. Use `--all-affinities` for every available Uchigatana affinity. |
| `native_command_benchmark` | Ignored release test in the desktop backend: Vanilla open searches (RL46 K=25 and loadout K=500, all upgrades RL93, bleed RL93, AoW full sequence RL150), Convergence Custom stats, two solves and an AR/bleed frontier, each through DTO normalization, the job worker and JSON encoding of the terminal status. Reports samples, payload bytes and the complete payload. Run `cargo test --release --locked --manifest-path apps/desktop/src-tauri/Cargo.toml native_command_benchmark -- --ignored --nocapture --test-threads=1` with `ER_BENCH_REPEATS` and optionally `ER_BENCH_CASE`; the `workflow_benchmark` runner does not select it. |
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
`compiler_variant_fingerprint` separately identifies the compiler inputs: `cargo -vV`
and `rustc -vV` output, the manifest's `[profile]` tables, the bytes of every Cargo
configuration file Cargo would read (`config` or `config.toml` in Cargo home and in
the working directory and its ancestors), compiler-affecting `CARGO_PROFILE_*`,
`CARGO_BUILD_*`, `CARGO_TARGET_*`, `CARGO_ENCODED_*` and `CARGO_INCREMENTAL`
variables, `RUSTFLAGS`-style and native compiler variables, and the Cargo arguments.
Changing LTO, codegen units or Rust flags at the same commit therefore produces a
different compiler identity. The collector hashes these inputs and leaves profile
resolution and precedence to Cargo, so it does not report an effective profile.
Configuration files are hashed, never copied, and other environment variables
(including registry tokens and proxies) are never recorded.
The invocation retains the command and the output-location variables
(`CARGO_TARGET_DIR`, `CARGO_BUILD_TARGET_DIR`, `CARGO_BUILD_BUILD_DIR`,
`CARGO_BUILD_JOBS`); changing the repeat count or target directory alone does not
define a new compiler variant. Raw samples and complete result fingerprints remain
in their existing case records.
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
cannot establish parity. Affinity Watch reports retain the complete payload;
upgrade-series timings still need their separate equivalence and smoke checks.

The Paths timer accepts already-solved requests. Fixture searches, request cloning,
serialization and repeat comparisons are outside the measured interval. The
level-80 Samurai fixtures use exact upgrades: Keen Uchigatana/Unsheathe at +25,
and Standard Bloodhound's Fang/Bloodhound's Finesse at +10. One lane uses Uchigatana;
two lanes calculate these different loadouts sequentially. Reports retain the full
input requests and original sample order. This measures native path calculation,
not job scheduling, IPC, rendering or cancellation responsiveness; use the separate
native probe for those observations.

### Search pruning pass (2026-10-17)

Release builds of `a7fe55c` and of this pass ran interleaved (baseline and new
alternating, one warmup and one sample per round, five rounds) on a 16-thread
Windows machine where other work held 4 to 9 cores, so ranges are wide. Every
row's complete ranked result or payload matched the baseline. Medians, with the
sample range, in milliseconds at the default thread count:

| Workload | Before | After |
| --- | --- | --- |
| Open AoW full sequence, RL150, K=5 | 6462 [6056, 13427] | 94 [82, 954] |
| Open AoW first hit, RL150, K=5 | 6270 [5403, 6454] | 87 [85, 94] |
| All-upgrades bleed, RL93, K=25 | 2375 [2251, 2891] | 86 [73, 93] |
| Same, Convergence | 1254 [1222, 1258] | 171 [142, 310] |
| Open bleed, RL93, K=25 | 177 [165, 229] | 58 [55, 62] |
| All-upgrades Max AR, RL93, K=25 | 278 [256, 366] | 114 [101, 136] |
| Open Max AR, RL46, K=5 | 103 [101, 115] | 59 [57, 85] |
| Open Max AR, RL9 / RL713 | 108 / 365 | 31 / 58 |
| Convergence Custom stats ranking, K=25 | 565 [543, 601] | 72 [71, 83] |
| Native command: open AoW full sequence, K=25 | 9908 [9351, 11000] | 99 [94, 114] |
| Native command: open K=500 loadouts (1.49 MB JSON) | 124 [117, 129] | 84 [77, 92] |
| Affinity Watch, Uchigatana +0, any Ash, 200 levels | 2452 [2189, 2569] | 332 [319, 342] |

With one Rayon thread, open Max AR fell from 1330 to 475 ms, all-upgrades Max AR
from 2132 to 671 ms and the open AoW full sequence from 30.1 s to 0.63 s. Affinity
Watch with one thread changed little (2049 to 1724 ms), because its gain is
concurrency. Paths stay under 16 ms at 200 levels, and solves, frontiers and the
upgrade series under 3 ms. The skill-objective search had been spending most of
its time enumerating coupled Lifesteal-style routes for every weapon; the exact
route bound now skips them. Bounds and their proofs are in the
[optimizer math](design/optimizer-math.md#bounds-and-scheduling).

Parity covered all phase cases in both profiles at both thread counts, 400 seeded
random requests (both profiles, all objectives, levels 9 to 713, handedness,
upgrade modes, grouping, K, locks and floors) with level ranges, the level-range
benchmark, and every workflow and native command payload.

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
250 ms on the reference development machine. Core enumeration tests check that
every search poll reads the cancellation signal independent of the progress
throttle, including with progress disabled, and that enumeration stops at the
first cancelled poll; wall-clock latency is left to the native probes. Backend
workflow tests check propagation through nested evaluators and fail-closed results
without partial payloads.

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
