# Changelog

Release notes are maintained in [`docs/release-notes/`](docs/release-notes/README.md).

Use that index for per-version notes and GitHub release links.

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
