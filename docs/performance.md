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

The phase and workflow runners also accept `--baseline <report>`. They refuse a
baseline recorded with a different profile, dataset, model, build profile or thread
count; a setting the baseline did not record is not checked. Otherwise comparisons are
advisory unless a stable dedicated runner opts into `--fail-on-regression`.

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
calculation-check scope and the packaged-check boundaries.

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
- No motion on keyboard-summoned surfaces such as the Ctrl+K palette and the
  shortcut list.
- A closing popover fades with discrete `display` and `overlay` transitions and
  `pointer-events: none`. Give reduced motion the whole transition list: overriding
  only `transition-property` shifts `allow-discrete` onto the wrong properties.
- Fade `<details>` content by opacity; never transition its visibility.
- Reserve scrollbar gutters (`scrollbar-gutter: stable`) on scroll containers.
- Sort ranking rows with flex `order` in a `reading-flow: flex-visual` group. Moving
  their DOM nodes restyles and lays out every row (~15 ms), and rendering each place's
  row into it rewrites every row's contents (3 ms of script and 5 ms of layout for 50
  rows); `order` costs what measured offsets did (about 2.5 ms before paint) without
  measuring any row.
- Hide an offstage workspace with non-inherited properties. Toggling inherited
  `visibility` on Rankings restyled its ~1,500 rows' elements on every tab switch.
- Reveal workspaces in a deferred render (`useDeferredValue`), so a tab key press
  is not followed by an 11-13 ms task.
- Do not put `content-visibility: auto` on ranking rows or groups of them: rendering
  rows as they scroll in raised wheel-scroll p95 from 4.3 ms to 12.5-16.7 ms. WebView2
  renders `auto` content within about 1.5 viewports, which is 43 of 50 rows at the top
  of the board, so little is skipped; and its paint containment inside a composited
  ancestor (the rows group, or Rankings while its entrance holds) splits the rows into
  about 45 layers, which every row entering or leaving that margin re-layerizes (7-11 ms).
- Rest ranking rows well outside the board's view (`content-visibility: hidden` with a
  remembered `contain-intrinsic-height`) while the window is resized, while Rankings
  is hidden and as new results land, and wake them twelve per frame afterwards. A width
  step, a return to the tab and a landing then render about 15 rows instead of 50, and
  nothing on screen moves.
- Render new results in a deferred render (`useDeferredValue`), with everything the rows
  show beside them, so React splits the work instead of one 7-10 ms task.
- Keep width media queries in their own sheet (`breakpoints.css`, adopted after the
  others). Crossing one inside the main sheet restyled the whole page (5-6 ms traced).
- Open keyboard-summoned modal lists as popovers, not with `showModal()`: making the page
  inert restyles every element as the list opens and again as it closes. Keep the modal
  semantics by hand (focus trap, focus return, `aria-hidden` on the app shell once focus
  is inside; for `aria-modal` alone, WebView2 still exposes the page).
- No overflow clip on values that always fit: each clip is a paint property node, and the
  board's 500 token clips cost about 15% of every resize and repaint of it.
- Never run an animation inside an element that is itself animating or transitioning:
  Chromium then ticks the inner one on the main thread every vsync.
- No explicit `inherit` on the cells of rows that move: one inherited child makes every
  change to the row restyle all of its children.

Measure interactions with GPU compositing on. The packaged smoke harness sets
`TARNISHEDS_ARSENAL_SMOKE_DEBUG_PORT`, with which the app adds `--disable-gpu` to its
WebView2 arguments. Instead, launch the release exe without that variable, with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` set to a remote-debugging port and
`WEBVIEW2_USER_DATA_FOLDER` set to a scratch folder.

The v0.18.0 records of [frame pacing](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.18.0/docs/performance.md#frame-pacing-2026-10-17), the
[resize, shortcut list and Rankings return](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.18.0/docs/performance.md#resize-shortcut-list-and-rankings-return-2026-10-17)
and the [visual refinement pass](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.18.0/docs/performance.md#visual-refinement-pass-2026-10-18) keep their
scenarios, traces and per-run results; the rendering rules above are what still applies.

### Result landing (2026-10-21)

Scenario: Vanilla Samurai at level 192 (VIG60 MND20 END40 STR40 DEX40 INT9 FAI8 ARC54),
open weapon and Ash, all upgrades, 50 rows, 1650x950 window. Each landing changes DEX
between 40 and 41 through the palette, waits 1.2 s, presses Ctrl+Enter and records
`requestAnimationFrame` intervals from 50 ms before until 600 ms after the commit that
ends the search look. Release exes (`npm run tauri -- build --no-bundle`) of `bcb1574`
and of this pass were launched normally over CDP as above, untraced, alternating builds
per launch; the first landing of each launch, which mounts the rows, is reported apart.

Traced, a landing was two tasks: React's render and commit of the new rows, Build
Detail and the query strip (7-10 ms; 7.4 ms rendering and 4.2 ms committing in a sampled
profile of an unminified build, mostly reconciling and updating the 50 rows' elements),
then one frame of style, layout, pre-paint and paint for all 50 rows (13-16 ms, half of
it paint).

Frame intervals on a 240 Hz display (4.2 ms frames), 10 landings per launch after the
first; the Max AR rows pool three launches per build:

| Scenario | Landing worst frame, before | After | First landing, before | After |
| --- | --- | --- | --- | --- |
| Max AR (30 landings) | 12.4-16.8 ms | 8.2-8.5 ms | 37.5-41.7 ms | 12.5-16.7 ms |
| AoW First Hit | 8.2-12.6 ms | 4.3-8.4 ms | 33.4 ms | 16.7 ms |
| Max AR sorted by AR | 12.4-16.7 ms | 8.2-8.4 ms | 33.3 ms | 16.8 ms |
| Max AR, reduced motion | 8.2-12.6 ms | 4.3-8.4 ms | 41.6 ms | 16.6 ms |

Median frame intervals stayed at 4.2 ms in both builds. A landing now spans at most two
240 Hz frames, against three or four before.

What changed:

- Rankings renders new results through `useDeferredValue`, so React splits their render
  into short tasks; the board keeps its searching look, selection and movement baseline
  until the rows commit, and the commit then carries everything as before.
- In that commit, rows outside the view rest as they do during a resize: with the same
  row count, the rows that will be drawn where the board shows rows now stay awake
  (a sort can reorder them); otherwise as many rows as the measured view can hold. The
  others wake twelve per frame from the frame after the landing, or when a resize
  settles. A landing then lays out and paints about 15 rows.

Every board and window screenshot after a landing (Max AR, AoW First Hit, sorted by AR,
reduced motion) matched the before build pixel for pixel. Reduced motion keeps its
plain fade.

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
| `benchmark_workflows` | Vanilla Paths in No-respec and Best per level (`optimum_envelope`) modes at 10, 50, and 200 levels with one and two distinct lanes; Affinity Watch at the same horizons; the standard 26-point upgrade series; and the catalog index build for both profiles. The Rust tests perform one warmup and retain best, median, worst, and every sample. |
| `benchmark_level_range` | The documented command measures Uchigatana at level 80 plus offsets 0, 10, 50, and 200 for Keen, Blood, and Occult. The CLI defaults to 10, 50, and 200. It compares independent per-level optimization with shared range evaluation and rejects any difference in complete ordered results. Use `--all-affinities` for every available Uchigatana affinity. |
| `native_command_benchmark` | Ignored release test in the desktop backend: Vanilla open searches (RL46 K=25 and loadout K=500, all upgrades RL93, bleed RL93, AoW full sequence RL150), Convergence Custom stats, two solves and an AR/bleed frontier, each through DTO normalization, the job worker and JSON encoding of the terminal status. Reports samples, payload bytes and the complete payload. Run `cargo test --release --locked --manifest-path apps/desktop/src-tauri/Cargo.toml native_command_benchmark -- --ignored --nocapture --test-threads=1` with `ER_BENCH_REPEATS` and optionally `ER_BENCH_CASE`; the `workflow_benchmark` runner does not select it. |
| `native-responsiveness.mjs` | Analysis suite: packaged solves, upgrade series, AR/bleed frontier and cancellation. Production suite: locks/non-additive routes, K=500, both Paths modes and cancellation. Both include a concurrent manifest probe and retain executable/manifest identity, requests, complete fingerprints, warmups, samples and timing ranges. |

Phase cases use profile rules for upgrade caps: Vanilla standard +25/Somber +10;
Convergence standard and Somber +15. Exact-upgrade and all-upgrades searches are
different workloads; `all-upgrades-max-ar-high-level` uses the broader level-93,
25-row case. Convergence damage-objective cases are skipped when the profile does
not declare AoW damage support and fail when selected explicitly. The other
Convergence cases run as Custom stats at the Vanilla case's level plus 79, without
extra locks, so they keep the Vanilla budget independent of the mod's classes.
`custom-stats-open-ranking-25` runs only on Convergence and locks every combat stat,
as the desktop does for Custom stats.

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
describes that scope; untracked files outside the source locations, generated
output and secret-like paths are excluded.
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

The v0.18.0 [search pruning pass](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.18.0/docs/performance.md#search-pruning-pass-2026-10-17) and
[coupled routes and upgrade dominance](https://github.com/FueledByRedBull/tarnisheds-arsenal/blob/v0.18.0/docs/performance.md#coupled-routes-and-upgrade-dominance-2026-10-17)
records keep their workloads, sweeps and parity runs.

### Shared level ranges (2026-10-21)

Workload: the 51-level range from the v0.18.0 records (Samurai, levels 80 to 130, VIG40 MND11 END20 STR12
DEX15 INT9 FAI8 ARC20, open weapon and Ash, AoW first hit, all upgrades to +25 and
Somber +10, K=25) through `optimize_level_range_with_progress`. Timer probes around the
phases of one-thread runs located the cost: each level rebuilt its plan (about 110 ms of
Ash grouping), evaluated about 68,000 exact route bounds (3.5 million for the range,
12.6 s of 21.4 s) to prune 99% of routes, and repeated the upgrade-dominance test in
every work unit (about 270,000 calls a level with one thread, 1.5 million with the
pool's per-Ash units). Only about 470 routes a level reached the exact DP.

What changed ([level ranges](design/optimizer-math.md#level-ranges)):

- The highest level runs first and records its route bounds, and for skill objectives
  each Ash's largest bound. Lower levels prune with them before computing their own, and
  skip a work unit whose Ashes all bound below its cutoff.
- Lower levels reuse the highest level's Ash groups with their own budget.
- Without the fixed-loadout DP cache, lower levels run in parallel batches of the pool's
  width; results and progress keep level order.
- The upgrade-dependent half of the dominance test runs once per prepared weapon, and
  its ARC half once per work unit, in every search.

Release builds of `bbf225b` and of this pass, 16 threads alternating builds (6 runs
each) and one thread (2 runs each):

| Threads | Before (median) | After (median) |
| --- | --- | --- |
| 16 | 3,370 ms (3,261-3,457) | 283 ms (271-323) |
| 1 | 20,597 ms (20,374-20,820) | 1,308 ms (1,296-1,320) |

The complete ranked rows of the range are identical at both thread counts and in both
builds (SHA-256 of their Debug output `0b7a8bab…7226`).

Parity: 1,400 seeded random requests (982 Vanilla, 418 Convergence; every objective,
classes, levels, locks, floors, two-handing, Scadutree blessing, weapon, affinity and Ash
pins, grouping and K, with 643 level ranges of 1 to 51 levels in steps of up to 4)
returned the same complete results as `bbf225b` at the default thread count, the 165
refused requests included. Run one build at a time, the sweep's single searches took
11.5 s in both builds and its level ranges 120.4 s before and 44.1 s after. The core tests compare derived plans with plans built for
each level, and skill-objective ranges with independent searches at one thread and at
the pool's width.

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
