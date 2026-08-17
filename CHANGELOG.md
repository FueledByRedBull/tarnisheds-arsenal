# Changelog

Release notes are maintained in [`docs/release-notes/`](docs/release-notes/README.md).

Use that index for per-version notes and GitHub release links.

## v0.10.1 — 2026-08-17

- Preserve the historical Windows Installer upgrade identity across product-name
  typography changes and verify it in both source metadata and the built MSI.
- Keep the configured window title and Windows product name consistent.
- Test exact snapshot promotion, including stale-CSV removal and fail-before-write
  handling for unsafe manifest paths.
- Verify the optimized bleed-only scoring path against full status calculation for
  both scaling and non-scaling game profiles.
- Normalize the documented `-99999` missing-status sentinel to zero and reject all
  other negative or non-finite status buildup at the runtime-data boundary.
- Rename the internal bleed objective to match its `Bleed, then AR` behavior while
  preserving existing request and saved-build wire IDs.
- Replace the long project README with a concise user-first guide and current app
  screenshot; document why final AR is recomputed after dynamic-program selection.

## v0.10.0 — 2026-08-17

- Remove duplicate synchronous frontend/native search paths and make browser mocks
  exercise the same start/status/cancel job contract as the desktop backend.
- Consolidate polling and search orchestration while preserving request-generation,
  cancellation, and stale-result protection.
- Remove the internal PyO3/Maturin bridge, duplicate Python runtime validator,
  unused APIs/helpers/tests/dependency, and four reproducible diagnostic CSVs;
  retain native Rust tests, benchmarks, and an external-calculator probe.
- Canonicalize weapon-type normalization and the Kinetic Forge stylesheet.
- Score Bleed-then-AR candidates with an exact bleed-only path before calculating
  full AR/status for tie-breaks and retained rows.
- Replace release source-text assertions with executable artifact, provenance,
  checksum, signing, and clean-source gates.
- Store the Windows product name with a typographic apostrophe so Explorer no
  longer displays an unintended escape character.

## v0.9.3 — 2026-08-16 (not published separately; included in v0.10.0)

- Keep long Paths horizons inside the chart panel and scale both analysis charts to
  their observed metric range instead of flattening positive values against zero.
- Give Affinity Watch a distinct responsive crossover-line chart with clear series,
  crossover markers, bounds, and an accessible data table.
- Keep every damage, element, and status token equal-width and centered across
  ranking rows.
- Give populated and empty podium cards consistent internal spacing.

## v0.9.2 — 2026-07-23

- Keep numeric stat editing responsive by committing local drafts after idle,
  blur, or Enter and preparing the exact search only when Search is pressed.
- Speed up exact search-space estimation by reusing equivalent distribution
  counts, omitting scoring work allocation, and replacing full requirement-count
  probes with arithmetic feasibility checks while preserving exact counts.
- Show centered, non-truncated STR/DEX/INT/FAI/ARC scaling and all seven passive
  status families across Rankings, Compare, and Build Detail.
- Show Physical, Magic, Fire, Lightning, and Holy AR components in ranking rows,
  and center/wrap every ranking header for clear alignment at narrow widths.
- Retry native release bundling after transient packaging-tool download failures.
- Carry Sleep, Madness, and Death Blight through Rust, Python, desktop DTOs, saved
  build migration, mock data, and CSV exports.
- Reuse effective scaling already present on solved rows instead of issuing a
  separate backend request for every ranking and comparison row.

## v0.9.1 — 2026-07-22

- Fix Vanilla Rallying Standard/Commander's Standard searches and CSV exports that
  could fail on an explicitly unsupported non-damaging chained effect.
- Remove legacy leaked smoke presets, isolate future packaged smoke-test
  WebView/storage data, and delete its temporary saved build before shutdown.
- Exclude Convergence ammunition weapons from ranking until projectile/ammo AR is
  modeled, while retaining their extracted source rows for future support.
- Add configurable 25/100/500/2,000-row CSV export, result reuse, and
  weapon type, requirements, effective scaling/grade, and upgrade-path columns.
- Replace exhaustive Max AR and Max Physical AR stat enumeration with an exact
  relevant-stat dynamic program and add exhaustive-equivalence plus high-level
  phase-benchmark coverage.
- Show horizontal ranking controls only when additional columns exist and clearly
  describe where CSV files are downloaded.
