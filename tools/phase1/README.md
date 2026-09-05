# Phase 1 Dump Tooling

This folder intentionally excludes bundled WitchyBND binaries to keep the repository lean and publishable.

`phase1_dump.py` is the supported profile-aware snapshot entry point. It extracts numeric
PARAM relationships, profile-supported attacks and routes, effects, passives, and
coverage diagnostics into a sibling staging directory; validates hashes and file
sets; then promotes files with `manifest.json` last. A runtime loader can never
accept a partially promoted snapshot.

## Snapshot contract

The current storage schema is **4**. It requires explicit Ash mounting permission
in `weapons.csv.can_change_aow` and compatible affinity/type lists in `aow.csv`.
The extractor no longer emits `aow_weapon_compat.csv`; legality is derived from
those compact fields. Runtime manifests list 12 required tables, and diagnostic
Ash/affinity summaries are computed in memory.

Regenerate older snapshots with this extractor. Both runtime and Python validation
reject schema 3 before attempting to use its incompatible tables. Do not merely
edit an old manifest's version number. Dataset/game versions do not change when
only the storage contract changes; source hashes remain tied to the original inputs.

## Required Inputs

- A profile-specific `regulation.bin`
- Local WitchyBND executable path
- Convergence: base and DLC1 `WeaponName`/`ArtsName` FMG XML tables. Authoritative
  FMG names are merged by ID; conflicts fail extraction. Guarded PARAM labels fill
  only mod-added player rows that have no FMG entry, including alternate forms.
- Convergence: the tracked `data/reference/convergence-3.0.0.1-weapons.json`
  availability/model reference. Normal extraction fails closed if it is absent or
  bound to a different profile version.

## Example

```powershell
python tools/phase1/phase1_dump.py `
  --profile vanilla `
  --regulation data/raw/Vanilla/regulation.bin `
  --witchybnd data/raw/WitchyBND-3.0.1.0-win-x64/WitchyBND.exe `
  --output data/phase1
```

```powershell
python tools/phase1/phase1_dump.py `
  --profile convergence `
  --regulation data/raw/Conv/regulation.bin `
  --weapon-name-xml data/raw/Conv/WeaponName.fmg.xml `
  --weapon-name-xml data/raw/Conv/WeaponName_dlc01.fmg.xml `
  --arts-name-xml data/raw/Conv/ArtsName.fmg.xml `
  --arts-name-xml data/raw/Conv/ArtsName_dlc01.fmg.xml `
  --profile-version-file data/raw/Conv/version.txt `
  --witchybnd data/raw/WitchyBND-3.0.1.0-win-x64/WitchyBND.exe `
  --output data/profiles/convergence
```

Raw regulation, WitchyBND, and FMG inputs stay under ignored `data/raw/`. Only the
derived CSV snapshots and their source hashes are committed. Convergence currently
declares AoW hit and route damage unsupported, producing schema-only files for those
tables so runtime code cannot mix in Vanilla motion data.

The profile definition also owns gameplay rules that cannot be inferred safely from
Vanilla defaults. Convergence 3.0.0.1 uses one +15 reinforcement cap, has no
Scadutree scaling, uses extended `S+`/`S++` grades, and treats attack-element row 0
as applying each weapon's declared nonzero scaling stats to its nonzero damage
components. Its weapon status values do not use Vanilla stat scaling. Extraction
materializes these rules explicitly, filters exact legal configuration IDs, and
validates all common modeled fields and status families against the offline
version-bound reference.

`--allow-unverified-weapons` is a maintainer-only bootstrap switch for rebuilding
the reference candidate set. It must never be used to produce a release snapshot;
release validation requires the exact tracked reference match.

See `tools/phase1/PHASE1_CONVENTIONS.md` for normalization/indexing conventions.
