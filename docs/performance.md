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
  remembered `contain-intrinsic-height`) while the window is resized and while Rankings
  is hidden, and wake them twelve per frame afterwards. A width step and a return to the
  tab then render about 15 rows instead of 50, and nothing on screen moves.
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
work with the same core; medians of three runs each. Other builds kept the CPU
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

Convergence matched Vanilla, except that at the time it had no Compare, Paths or
Affinity Watch, and it has no objective that keeps a search running, so its busy
scenarios follow a short search. Search status payloads are at most 182 KB for 50 rows and IPC replies take
under 5 ms, so native transfer is not a frame-rate bottleneck.

A follow-up pass the same day sorts ranking rows with flex `order` instead of measured
offsets and opens the shortcuts dialog without re-rendering the query strip. Before is
`c2c3870`, after adds those two changes; both are release builds measured as above,
with three interleaved runs each. Frame intervals were recorded without a trace, since
the trace categories above doubled style times (the inert restyle below reads 12-14 ms
traced); attribution comes from separate runs that trace `toplevel` and
`devtools.timeline` only. A VirtualBox VM (about 3 cores), a browser and other builds
ran throughout, so maxima vary by two or three 4.2 ms frames and resize by more. This
core finishes the open AoW first hit search in 0.7 s, so busy scenarios give the
optimizer one thread (`RAYON_NUM_THREADS=1`) while 15 other threads spin at normal
priority, with the harness and the resize driver at high priority. Window resize steps
the width by 5 px every 8 ms between 1650 and 1350 px.

| Scenario (Vanilla) | Before p50 / p95 / p99 / max, ms | After | Key p50 / p95, ms |
| --- | --- | --- | --- |
| Wheel and scrollbar drag, sorted and unsorted | 4.2 / 4.3 / 4.3 / ≤ 4.4 | 4.2 / 4.3 / 4.3 / ≤ 4.4 | - |
| Sort header clicks | 4.2 / 4.3 / 4.3 / 12.6 | 4.2 / 4.3 / 4.3 / 8.5 | - |
| Row selection with Build Detail | 4.2 / 4.3 / 4.3 / 4.3 | 4.2 / 4.3 / 4.3 / 4.3 | - |
| Ctrl+1..4 cycling | 4.2 / 4.3 / 4.3 / 20.9 | 4.2 / 4.3 / 4.3 / 20.8 | 6.2 / 8.2 → 6.6 / 7.9 |
| Shortcuts dialog open and close | 4.2 / 4.3 / 4.4 / 25.0 | 4.2 / 4.3 / 8.4 / 20.8 | 19.8 / 26.1 → 15.5 / 22.4 |
| Popovers | 4.2 / 4.3 / 4.3 / 4.4 | 4.2 / 4.3 / 4.3 / 4.3 | 5.5 / 8.1 → 5.8 / 7.3 |
| Palette and typing | 4.2 / 4.3 / 4.3 / 8.3 | 4.2 / 4.3 / 4.3 / 8.4 | 6.3 / 8.2 → 6.3 / 8.2 |
| Window resize | 4.2 / 20.8 / 29.2 / 29.2 | 4.2 / 20.6 / 25.0 / 33.3 | - |
| Busy: window resize | 4.2 / 29.2 / 45.8 / 58.2 | 4.2 / 33.3 / 45.9 / 54.2 | - |
| Busy: wheel | 4.2 / 4.3 / 12.5 / 16.6 | 4.2 / 4.3 / 8.4 / 12.6 | - |

In-page timings, medians of 30 to 60 repeats: a sort click costs 2.4-2.7 ms of script,
style and layout before paint with offsets and 2.4-2.5 ms with `order`. Toggling `inert`
on the app shell restyles all 3,157 elements in 7.1-8.3 ms, about 2.3 µs each; the 50
ranking rows hold 2,412 of them (48 per row). Dropping the 500 `b` wrappers of token
values saves 1.2 ms of that, and matching the row cells by class instead of `span`
position saves at most 1 ms. `content-visibility: auto` on the rows was retried with a
fitted `contain-intrinsic-size`, without sticky cells and in groups of 5 and 10 rows;
the rendering rules above record why none of it helped.

Remaining misses, measured:

- Window resize: each width change cost a 6-7.5 ms layout task and a 9-13 ms paint
  task (4-5 ms of paint, 3-4 ms of pre-paint, 1.5-2 ms of layerizing), about 90% of it
  the ranking rows, and crossing the 1500 px breakpoint restyled all ~2,800 elements
  once (8-10 ms).
- The shortcuts dialog: `showModal()` restyled the whole document inside the key press
  as the page became inert, and closing restyled it again, 7-12 ms untraced each way.
  Opening and closing no longer re-render the query strip, which saves 1.4 ms and
  0.8 ms of script.
- Returning to Rankings was one 11-12 ms traced task (5.4-6 ms of paint, 2.1-2.4 ms of
  pre-paint, 1.5-1.8 ms of layerizing): `content-visibility: hidden` drops the hidden
  panel's paint, so all of its rows repainted.
- The next pass fixes these three; see below.
- A sort click no longer measures rows: in a traced run of six clicks its only long
  task was one 8.5 ms repaint (5.3 ms of paint, 1.9 ms of layerizing).
- Selecting a row is a 7-11 ms task (about 3 ms of script and 2-3 ms of layout for Build
  Detail), at the edge of the p99 limit.
- During a search, opening a popover can take a 9-15 ms task (5-8 ms of script).

### Resize, shortcut list and Rankings return (2026-10-17)

Same machine and method as above: release builds launched normally over CDP, frame
intervals recorded untraced, Vanilla level 192 with 50 rows. Before is the follow-up
pass above (flex `order` sorting and the ref-opened dialog), after adds this pass; four
interleaved runs each, busy scenarios three. The VirtualBox VM kept the CPU 60-80% busy,
so whole runs occasionally show 25-90 ms spikes in every scenario; medians are reported.
Each resize run now checks that the window really changed width (at least 35 resize
events), since some of the driver's sweeps silently left the window alone. Attribution
comes from traces of `toplevel`, `devtools.timeline` and its `disabled-by-default`
category.

| Scenario (Vanilla) | Before p50 / p95 / p99 / max, ms | After | Key p50 / p95, ms |
| --- | --- | --- | --- |
| Window resize, 1650 to 1350 px and back | 4.2 / 16.6 / 20.8 / 25.1 | 4.2 / 4.3 / 8.4 / 12.5 | - |
| Window resize below the 1500 px breakpoint | 4.2 / 16.6 / 20.8 / 20.9 | 4.2 / 4.3 / 8.4 / 12.6 | - |
| Busy: window resize | 4.2 / 29.2 / 41.7 / 66.6 | 4.2 / 8.4 / 12.5 / 20.8 | - |
| Shortcuts list open and close | 4.2 / 4.3 / 4.3 / 20.7 | 4.2 / 4.3 / 4.3 / 4.3 | 13.8 / 23.8 → 5.4 / 8.1 |
| Ctrl+2 then Ctrl+1, six times | 4.2 / 4.3 / 4.3 / 8.4 | 4.2 / 4.3 / 4.3 / 4.4 | 5.4 / 8.0 → 5.6 / 8.2 |
| Wheel, drag, sorts, selection, popovers, palette | 4.2 / 4.3 / 4.3 / ≤ 12.5 | 4.2 / 4.3 / 4.3 / ≤ 8.4 | 5.6-6.3 / 7.7-8.0 → 4.7-6.0 / 7.3-7.6 |

Traced, a resize sweep of 220 width steps leaves one task over 6.94 ms (7.7 ms) instead
of one 17-19 ms layout-plus-paint pair per step; each return to Rankings was a 10.4-12.1 ms
task and now none exceeds 6.94 ms; opening or closing the shortcuts list no longer has a
long task. Every board screenshot (unsorted, sorted by AR and by skill damage, selected,
hovered, focused, scrolled, returned to, after a resize, at 1480 and 1300 px, the open
shortcut list and the whole window, under Max AR and AoW First Hit) matches the before
build pixel for pixel, apart from spots that also differ between two captures of the
before build. The layer count stays at 15.

What changed:

- Resting rows. An `IntersectionObserver` on the board tracks rows within two rows of its
  view. When a resize starts or Rankings is hidden, the other rows (about 35 of 50) get
  `content-visibility: hidden`, keeping their remembered height, so a width step lays
  out and paints about 15 rows. They wake twelve per frame 150 ms after the last resize
  event, or from the second frame after Rankings is shown; a row that comes into view
  wakes at once. Rest and wake cost 0.4-0.7 ms of style and layout.
- Breakpoints in their own adopted sheet. With the media rules inside the main sheet,
  crossing 1500 px was a 9-14 ms traced task (5-6 ms of style); in its own sheet it no
  longer shows up.
- The shortcuts list is a popover beside the app root. `aria-modal` alone left 32
  controls of the page exposed to assistive technology, so the app shell takes
  `aria-hidden` once focus is in the list, which leaves the same single control exposed
  as the modal dialog did. Tab now stays on the close button (the modal dialog let it
  reach the browser), Escape closes the list natively and focus returns to where it was.
- Token values in ranking rows lose their overflow clip; the values fit at the minimum
  window width.

Options measured and not shipped:

- Fewer elements per row. Without the token grids, a width step's traced layout fell
  from 6.8 to 2.2 ms and the whole step from 18 to 8 ms, but each token's visible value
  must stay hidden from screen readers under its labelled list item, which takes the
  `b` element. Static instead of sticky row cells saved about 2.8 ms of 18 and hiding
  the action icons about the same, but static cells move weapon names by a pixel at some
  widths, and the icons need `currentColor`. Dropping the clip on AR and skill numbers
  clipped glyph edges differently.
- Fixed token column tracks: traced layout per step fell from 6.8 to 2.4 ms, but the
  moved cells still repaint (4.8 ms of paint) and the frames did not improve, besides
  changing the columns' proportions.
- Keeping the hidden panel's raster: `opacity: 0` on its own layer under the other
  workspaces, with `inert` for focus, find and assistive technology. Toggling `inert` on
  its 2,498 elements costs 9 ms by itself, and each tab switch became a 19-27 ms traced
  task.
- Virtualizing the rows for good: find in page and screen-reader browsing would lose the
  rows outside the view. Resting only during a resize or while hidden keeps both.

Still short of one 240 Hz frame per step:

- A resize's first width step lays out every row before its resize event lets the far
  rows rest (a 6-8 ms traced layout), and a step over the remaining 15 rows takes 3-5 ms
  traced, so a few percent of frames span two 240 Hz refreshes (p95 4.3 ms, p99 8.4 ms)
  and the longest spans three (12.5 ms). That is within 120 fps.
- During a search, resize frames reach p95 8.4 ms and p99 12.5 ms: the optimizer's
  threads compete with the renderer for the CPU.

### Visual refinement pass (2026-10-18)

The pass that unified folds, empty states and chart frames, made searchable lists and
Build Detail's heading opaque, and let ranked rows' action buttons rest without a box
was measured the same way: release builds launched normally over CDP, frame intervals
recorded untraced, Vanilla Samurai level 192 with 50 rows in a 1650x950 window. Before
is `102db2f`, after adds the pass; four interleaved runs each, eight for the busy
scenario, and the table gives the median of each run's statistic. Every build ran with
`RAYON_NUM_THREADS=1`. The busy scenario starts an AoW First Hit search 0.3 s before a
1650 to 1350 px resize sweep and a wheel scroll; the search lands during the sweep in
every run, so its result render is included. Other work shared the CPU.

| Scenario (Vanilla) | Before p50 / p95 / p99 / max, ms | After | Key p50 / p95, ms |
| --- | --- | --- | --- |
| Wheel and sort header clicks | 4.2 / 4.3 / 4.3 / ≤ 8.4 | 4.2 / 4.3 / 4.3 / ≤ 8.4 | - |
| Row selection with Build Detail | 4.2 / 4.3 / 4.3 / 4.3 | 4.2 / 4.3 / 4.3 / 4.3 | - |
| Ctrl+2 then Ctrl+1, six times | 4.2 / 4.3 / 4.3 / 20.9 | 4.2 / 4.3 / 4.3 / 16.6 | 6.8 / 8.0 → 6.5 / 8.1 |
| Shortcut list open and close | 4.2 / 4.3 / 4.3 / 4.3 | 4.2 / 4.3 / 4.3 / 4.4 | 5.8 / 8.3 → 5.8 / 8.2 |
| Editor popovers | 4.2 / 4.3 / 4.3 / 4.3 | 4.2 / 4.3 / 4.3 / 4.3 | 6.7 / 7.0 → 6.8 / 7.7 |
| Palette and typing | 4.2 / 4.3 / 4.3 / 6.3 | 4.2 / 4.3 / 4.3 / 6.4 | 6.3 / 9.0 → 6.5 / 9.0 |
| Window resize, 1650 to 1350 px and back | 4.2 / 4.3 / 8.4 / 12.6 | 4.2 / 4.3 / 8.4 / 12.5 | - |
| Busy: resize and wheel as a search lands | 4.2 / 8.3 / 10.5 / 25.0 | 4.2 / 8.4 / 12.4 / 25.1 | - |

The pass adds no element, layer or animation to a ranked row: the action buttons change
only their resting border and background, and the other changes are static styles
outside the board. Per-run busy p99 values spread from 8.4 to 16.6 ms in both builds. One
after run of the busy scenario reached 62.4 ms where the results landed; the other seven
after runs and all eight before runs peaked at 20.7 to 25.1 ms, so the landing render,
which this pass does not change, stays the longest frame of that scenario.

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

A second pass against `db3dc75`, measured the same way, targeted the slowest
remaining cases. Level ranges now reuse exact DP tables when no Ash is pinned
(one table set per stat shape, Ash and route, released with the range; peak
memory did not grow). Route bounds reuse their curve maxima within a work unit.
Weapon preparation builds the Ash lists, attack rows and handled routes once per
weapon kind in parallel and shares the routes with scoring, groups choices in
parallel, and checks cancellation every 64 weapons per thread on any pool size.

| Workload (default threads unless noted) | Before | After |
| --- | --- | --- |
| Affinity Watch, any Ash, 200 levels | 381 to 479 | 163 to 228 |
| Same, one thread | 2368 [2285, 2523] | 714 [645, 729] |
| Open Max AR, RL150 / two-handed | 66 / 92 | 39 / 51 |
| Open K=25 loadouts, RL46 | 61 [58, 69] | 34 [29, 44] |
| All-upgrades Max AR, RL93 | 120 [103, 132] | 77 [70, 87] |
| Open AoW full sequence, RL150 | 104 [98, 117] | 64 [56, 68] |
| Convergence open Max AR / Custom stats | 51 / 55 | 26 / 30 |
| Native command: open RL46 K=25, one thread | 327 [309, 390] | 197 [186, 205] |
| Open Max AR, RL46, one thread | 357 [330, 361] | 210 [202, 247] |

Paths, solves and frontiers stayed within noise (under 13 ms). Every phase case
in both profiles at both thread counts, the 400 seeded requests, and every
Affinity Watch, Paths and native command payload matched the baseline.

### Coupled routes and upgrade dominance (2026-10-17)

A third pass targeted the searches the sweep below found slowest: Lifesteal Fist,
whose coupled routes were enumerated with arbitrary-precision rationals, and
all-upgrades skill searches, which solved every lower upgrade although it almost
never scored more. Coupled routes now compare on integer keys, and upgrades that
cannot outrank a higher one are skipped
([coupled routes](design/optimizer-math.md#5-compiled-routes-oracle-and-fallback),
[upgrade dominance](design/optimizer-math.md#upgrade-dominance)).

Release builds of `7399abe` and of this pass ran interleaved, five rounds at the
default thread count (the slowest old case once), on the same 16-thread machine
while builds and browser tests held several cores, so ranges are wide. Medians,
with the sample range, in milliseconds; levels use the Samurai fixture with K=25
unless noted:

| Workload | Before | After |
| --- | --- | --- |
| Lifesteal Fist, all upgrades, first hit, RL200 | 182972 | 151 |
| Lifesteal Fist, exact upgrade, full sequence, RL150 | 27115 [14852, 29568] | 133 [102, 178] |
| Barbaric Roar, all upgrades, first hit, RL200 | 1356 [922, 1504] | 134 [86, 189] |
| Open AoW first hit, all upgrades, RL150 | 976 [696, 1133] | 168 [116, 239] |
| Open AoW full sequence, all upgrades, RL60 | 989 [710, 1266] | 155 [92, 160] |
| Same, loadout grouping, RL100 | 978 [699, 1100] | 197 [130, 206] |
| Open AoW first hit, all upgrades, loadouts K=500, RL150 | 1252 [936, 1509] | 482 [350, 514] |
| Open AoW first hit, all upgrades, levels 80 to 130 | 63211 [42808, 78712] | 5618 [4852, 7319] |
| All-upgrades Max AR / bleed, RL93 | 107 / 80 | 74 / 77 |
| Convergence all-upgrades Max AR, RL150 | 134 [94, 205] | 61 [59, 69] |

Exact-upgrade open searches without a coupled route and open Max AR loadouts
stayed within noise. A sweep of every Ash, plus no Ash, for Max AR, bleed and both
skill objectives, with exact and all upgrades at levels 60, 100, 150 and 200 in both
profiles (1,904 searches), fell from 743 s to 39 s in total; its slowest search
fell from 162 s to 0.24 s. The Vanilla sweep at levels 60 and 150 with loadout
grouping (944 searches) fell from 352 s to 16 s at K=5 and from 1510 s to 26 s at
K=100, where the slowest search fell from 709 s to 1.7 s.

With one thread, `benchmark_workflows.py` against the same baseline matched every
Paths, Affinity Watch and catalog output; Paths and the upgrade series stayed within
noise, and Affinity Watch over 200 levels with any Ash took 596 ms instead of 739 ms.
Its catalog case now carries the timing fields and catalog snapshot the runner
compares; the runner previously stopped at that case.

A level range repeats the open search at each level, so the 51-level case above
still takes several seconds; with one thread it costs about as much per level as
one open search (0.67 s against 0.75 s).

Parity: every search of both sweeps (weapon grouping, and loadout grouping at K=5
and K=100), 1,400 seeded random requests (both profiles, every objective,
classes, levels 9 to 713, locks, floors, two-handing, Scadutree blessing, weapon,
affinity and Ash pins, grouping, K and level ranges) at the default and one
thread, and every case above returned the same complete ranked results.

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
