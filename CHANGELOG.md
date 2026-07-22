# Changelog

Release notes are maintained in [`docs/release-notes/`](docs/release-notes/README.md).

Use that index for per-version notes and GitHub release links.

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
