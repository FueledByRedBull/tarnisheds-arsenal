# Changelog

## Unreleased

- Validate saved starting classes before activation and recalculate saved results
  on current data before displaying them. Preserve archives and contain rendering
  or storage failures without discarding saved builds.
- Fix selector commit ordering and stale weapon metadata; account for forced
  two-handing in displayed requirements. Give the starter search coherent filters.
- Preserve comparison pins, invalidate removed targets and show verified comparison
  lanes before optional charts. Show lower stamina costs as improvements.
- Reject unavailable exact upgrade levels, retain explicit +0-only pinned
  comparisons, and report missing Affinity Watch endpoints as unavailable.
- Reduce Affinity Watch payloads and repeated catalog compatibility work; require
  calculation CSV headers and remove unused native lookup interfaces.
- Keep weapon-restricted transferable skill attacks under their own skill IDs,
  separate numbered charge levels and near/far projectile phases, and retain
  Glintstone Dart's follow-up after its charged projectile.
- Restrict skill routes to the selected weapon handling before scoring and
  materialization, including reusable loadout analyses.
- Show missing per-build skill calculations as unavailable and leave their CSV
  values blank. Preserve saved comparison targets when reopening Compare and Paths.
- Regenerate schema 6 snapshots with model `aow-routes-effects-v9`; compiled
  scoring identity `exact-v3` invalidates results from the previous route policy.
- Verify saved comparison pins at the level budget their own stats require, so
  builds pinned from a higher-level search load instead of failing.
- Move profile selection into the command rail and show each result's loadout,
  scaling and combat stats in Rankings. Use semantic status colours, loading
  placeholders and reduced-motion-aware transitions.

## [v0.15.0](docs/release-notes/v0.15.0.md)

## [v0.14.1](docs/release-notes/v0.14.1.md)

## [v0.14.0](docs/release-notes/v0.14.0.md)

Versioned changes, verification and downloads are maintained in the linked notes.
See [all release notes](docs/release-notes/README.md) for the full version index,
or return to the [project overview](README.md).
