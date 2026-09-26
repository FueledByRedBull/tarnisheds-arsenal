# Changelog

## Unreleased

- Correct Bloodfiend's Fork bleed-floor boundaries while preserving exact optimizer
  comparisons; retain all 32 formerly failing cases in external validation.
- Count source-marked repeated skill contacts, separate charge variants, preserve
  fixed stance damage, and correct five Scadutree multipliers from regulation data.
  Regenerate both snapshots as schema 6 with runtime model `aow-routes-effects-v8/exact-v2`.
- Block analyses and exports from stale selections, preserve partial saved stat
  locks, reject malformed saved poise values, and keep parallel progress ordered.
- Remove unused frontend helpers and duplicate workbook parsing; require lint and
  DTO validation evidence when verifying packages built with full source checks.
- Upgrade React and React DOM to 19.3, enforce React Hooks linting, and catch
  Rust/TypeScript DTO drift before builds. Keep late profile replies from updating
  the inspector after cancellation.
- Separate comparison persistence and shared build-tool styles from the growing
  frontend store and global stylesheet without changing saved formats.
- Add explicit thread-policy benchmark selection and verify warmup result parity;
  document measured workload tradeoffs while retaining the runtime thread policy.
- Recover damaged saved-build indexes without discarding original data, and add
  bulk backup previews and non-destructive restore as copies.
- Add previewable reproduction reports, calculated build explanations, and
  per-stat comparison differences.
- Exercise interrupted storage writes, malformed backups, and late job replies;
  keep the interface usable when saved-build storage access is denied.

- Enable ThinLTO and one codegen unit for release builds to reduce optimizer runtime
  without changing calculation results or requiring a newer CPU.
- Harden background-job failure and cancellation handling, snapshot loading, and
  saved-session recovery.
- Reduce repeated exact calculations for status scaling and multi-hit skill routes.
- Make Paths charts easier to read with labeled axes, sparse markers, and a
  level-inspection slider.
- Add the textured Arsenal brand mark and a simplified Windows icon.
- Restrict production connections, update affected dependencies, and verify the
  executable inside release ZIPs against the standalone portable binary.

## [v0.14.1](docs/release-notes/v0.14.1.md)

Correct Affinity Watch's chart scale, clarify the fixed-loadout AR sacrifice
baseline, and distinguish profile capabilities from weapon filters. Reject
ambiguous CSV data and malformed extraction numbers before they reach the model.

## [v0.14.0](docs/release-notes/v0.14.0.md)

Exact ranking arithmetic, corrected weapon and skill calculations, AR / bleed
tradeoffs, faster level-range calculations, and more reliable cancellation and
saved-build recovery. Portable ZIPs now exclude the separately available installer.

See [all release notes](docs/release-notes/README.md) for changes, verification,
and downloads by version, or return to the [project overview](README.md).
