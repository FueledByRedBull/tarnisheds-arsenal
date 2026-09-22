# Changelog

## Unreleased

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
