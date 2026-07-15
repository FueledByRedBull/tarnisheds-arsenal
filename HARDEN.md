# UI/UX redesign — kinetic forge

The correctness, extraction, snapshot, async, performance, validation, and v0.7.0
release hardening work remains recorded in the locally ignored `TODO.md`. This
document records the completed v0.7.0 product-design sprint and its evidence.

## Direction

Build a cleaner, more intuitive desktop calculator with the energy and spatial
depth of a premium motorsport/editorial site, without turning a data tool into a
heavy 3D demo. Use native React and CSS motion, restrained perspective, layered
light, an original low-contrast texture, and decisive typography. Keep expert
features immediately reachable instead of burying normal workflows in dropdowns,
modal chains, or repeated clicks.

## P0 — one clear interaction model

- [x] Replace “Focus” with one universal “Select build” behavior for podium cards,
  ranking rows, keyboard activation, Compare, Paths, Affinity Watch, and Build
  Detail.
- [x] Make every result row and podium card directly selectable with an unmistakable
  selected state; keep Lock as the only separate result action.
- [x] Preserve selection for every rank, not only the top three, with mouse,
  keyboard, and packaged-app regression coverage.
- [x] Remove duplicated navigation wording where the active workspace tab and a
  Build Detail action communicate the same thing.

## P0 — calm the information hierarchy

- [x] Replace the 19-column default ranking grid with a readable primary table:
  rank, weapon, setup, upgrade, scaling summary, AR/status, raw skill result, score,
  and Lock.
- [x] Keep full AR split, combat stats, route damage, status, stamina, warnings, and
  effect details in the always-visible Build Detail panel after one selection.
- [x] Reduce the top-three cards to a compact podium and remove that redundant strip
  at narrower supported widths so ranks 4+ do not appear secondary.
- [x] Add a persistent one-line query summary above results: objective, level,
  upgrade policy, handedness, game scope, dataset, and active constraint count.
- [x] Make open upgrade policy explicit so low-level +25/+10 results never look
  accidental.

## P1 — simplify controls without hiding capability

- [x] Recompose the left rail into clear Character, Loadout, Objective, Fine tuning,
  and persistent Search bands with stronger labels and less equal-weight chrome.
- [x] Keep common controls and specialist model options continuously visible rather
  than nesting them behind an Advanced toggle.
- [x] Replace cryptic assumption chips such as `1H`, `Open Stats`, upgrade ranges,
  and `Base Game` with concise plain-language states.
- [x] Add a clear current-progression shortcut: entered Standard/Somber reinforcement
  levels can be switched between exact levels and an explored `+0..cap` range with
  one visible action.
- [x] Keep Search and cancellation persistently reachable and make current search
  scope obvious before execution.

## P1 — visual system and motion

- [x] Establish a distinctive kinetic-forge system: editorial display type, compact
  technical body type, charcoal/obsidian surfaces, warm alloy highlights, and a
  cool spectral accent for active telemetry.
- [x] Add restrained spatial depth with layered panels, subtle CSS perspective,
  directional light, a 19 KB original texture, and hover lift while keeping data
  sharper than decor.
- [x] Use an orchestrated workspace entrance and purposeful transitions for search,
  selection, progress, and result interaction.
- [x] Respect `prefers-reduced-motion`; Playwright proves decorative animation is
  reduced to effectively zero duration.
- [x] Avoid continuous high-cost effects, layout-thrashing animation, GPU-heavy 3D,
  WebGL, and new runtime animation dependencies.
- [x] Preserve strong focus rings, keyboard navigation, readable contrast, and
  semantic selection state.

## P1 — responsive composition

- [x] Give the workspace priority while keeping controls and selected details
  continuously reachable at normal laptop widths.
- [x] Use intentional responsive states: compact columns and a hidden redundant
  podium at 1200/1280 widths, full composition at 1440/1920 widths.
- [x] Eliminate horizontal page overflow; only purpose-built data regions may scroll.
- [x] Verify 1200×720, 1280×720, 1440×900, and 1920×1080 layouts.

## P2 — onboarding and communication

- [x] Make the empty state explain the first useful action and what open loadout
  fields mean.
- [x] Use plain-language microcopy for objective, lock, upgrade, dataset, and model
  assumptions.
- [x] Distinguish raw AR, raw skill damage, status buildup, and enemy-adjusted damage
  wherever users could confuse them.
- [x] Keep model limitations and unsupported-effect warnings visible near affected
  route details without dominating unaffected builds.

## Acceptance gates

- [x] Search, Lock, Compare, Paths, Affinity Watch, presets, export, cancellation,
  and shared-session behavior remain intact.
- [x] All 13 Vitest tests, the TypeScript/Vite production build, and all seven
  Playwright contracts pass.
- [x] Playwright covers mouse selection below rank three, exact Build Detail update,
  keyboard selection, direct podium selection, and reduced-motion behavior.
- [x] A release-mode packaged-app smoke covers startup, real Rust-backed Search,
  rank-four selection, Compare, a 40-level Paths calculation, Affinity Watch, and
  clean shutdown.
- [x] Final screenshots cover every target viewport and were reviewed for hierarchy,
  truncation, contrast, selected state, empty state, and page overflow.
- [x] Production assets remain small: 19.03 KB texture, 29.70 KB CSS, and 222.10 KB
  JavaScript before gzip; no new runtime dependency was added.
- [x] README, optimizer design documentation, v0.7.0 release notes, and the ignored
  historical `TODO.md` describe the completed redesign.
- [x] No push, tag, publication, or superseded-asset deletion occurs without explicit
  user approval.

## Release-grade evidence

- `python tools/phase4/validate_release_metadata.py --tag v0.7.0`
- `python -m ruff check tools`
- `python -m pyright tools`
- Rust formatting and strict Clippy for the core and Tauri manifests
- 75 core tests and 13 Tauri tests, including packaged-snapshot command and
  cancellation integration tests
- Fresh release-mode Python binding plus `validate_phase4.py`: 0 warnings
- 13 Vitest tests and 7 Playwright tests
- Fresh `tauri build`: optimized v0.7.0 executable and MSI produced successfully

The final release packager intentionally requires a clean committed tree. After
commit approval, run it without `--skip-validation`; then verify checksums and push
only with explicit approval.
