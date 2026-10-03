# Changelog

## [v0.17.1](docs/release-notes/v0.17.1.md)

- Show in Build Detail how far #1 leads #2, how many legal setups the exact
  ranking covers, and which entered stats the build changes; keep its header in
  view while the details scroll.
- Fold Saved Builds, the reproduction report and Compare's build cards until
  opened, and show only inflicted statuses.
- Say beside the tab bar why a workspace is disabled, count Compare's pins, and
  keep each profile's query for when the player switches back.
- Announce each ranked row's AR or score, pin or lock the focused row with P and
  L, keep notices in one live region, and raise field borders to 3:1 contrast.
- Open the Ctrl+K palette without motion, plan levels in Paths with `level N`,
  and unify lock wording as stat locks.

## [v0.17.0](docs/release-notes/v0.17.0.md)

- Replace the command rail with an editable query line over the results, add a
  Ctrl+K palette that edits anything by typing, and show active Compare filters as
  removable chips.
- Undo and redo query changes, show how each rank moved after a search, sort
  result columns without renumbering ranks, and add keyboard shortcuts.
- Add motion that explains changes and searching progress, respecting reduced
  motion. Bring the empty Rankings grid and Compare matrix to WCAG A/AA.
- Prepare searches in parallel per thread and visit upgrades highest first:
  open searches prepare about five times faster with identical results, and bleed
  searches lose their 25 to 44 second outliers. Poll native jobs sooner.
- Finish every click within about one 120 Hz frame: reserve scrollbar gutters,
  sort rows in place, keep Rankings rendered while hidden, and stop tab switches
  from re-rendering the query line and Build Detail.
- Store attack-element corrections sparsely, cutting idle memory from about
  1.1 GB to 180 MB, and embed the runtime tables compressed, shrinking the
  executable from 17.6 MB to 12.5 MB.

## [v0.16.0](docs/release-notes/v0.16.0.md)

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
- Run the native desktop smoke test for every change, and let browser tests use
  any free local port.

## [v0.15.0](docs/release-notes/v0.15.0.md)

## [v0.14.1](docs/release-notes/v0.14.1.md)

## [v0.14.0](docs/release-notes/v0.14.0.md)

Versioned changes, verification and downloads are maintained in the linked notes.
See [all release notes](docs/release-notes/README.md) for the full version index,
or return to the [project overview](README.md).
