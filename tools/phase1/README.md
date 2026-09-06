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

## Normalization and indexing

- `weapons.csv` scaling columns (`str_scaling`, `dex_scaling`, `int_scaling`, `fai_scaling`, `arc_scaling`) are normalized to `0.0..1.0` by dividing raw weapon param values by `100.0`.
- `weapons.csv` includes AoW-filtering type fields:
  - `weapon_type_id`: raw `wepType` numeric value from `EquipParamWeapon`.
  - `weapon_type_name`: display name from Paramdex `WEP_TYPE` enum.
  - `weapon_type_keys`: pipe-delimited keys that directly match `aow.csv.valid_weapon_types` tokens.
  - `can_change_aow`: true only when raw `gemMountType` is 2, resolving the XML field default when omitted. Independent of `disable_gem_attr`, which restricts infusion, not Ash mounting.
- `calc_correct.csv` multipliers are normalized to `0.0..1.0` (`growth / 100.0`).
- `reinforce.csv` damage/scaling multipliers are emitted as raw multipliers from `ReinforceParamWeapon` (for example `1.058`), with no additional normalization.
- `calc_correct.csv` is pre-expanded through each curve's final stage point and at least effective Strength 148 (`floor(99 * 1.5)`):
  - `stat_value=0` is always written as `multiplier=0.0` and is reserved.
  - Runtime lookup convention is direct indexing: stat values map to matching indices.
  - No `-1` offset is used for curve lookup.
  - Authored curves that extend farther (currently through `150`) retain that full range.
  - Missing curve ids and missing stat cells remain missing at runtime; the loader rejects duplicate `(curve_id, stat_value)` rows rather than silently filling or overwriting them.
  - Expansion uses segmented exponent handling:
    - if `adjPt > 0`: `ratio_curve = ratio ** adjPt`
    - if `adjPt < 0`: `ratio_curve = 1 - (1 - ratio) ** (-adjPt)`
    - if `adjPt == 0`: `ratio_curve = ratio`
- `aow.csv` status extraction uses only passive effect fields (`spEffectId0`, `spEffectId1`).
  - `spEffectId_forAtk*` fields are ignored (they are attack-hit effects for active skill execution).
  - Native-only placeholder gems (missing real sort ID or icon) are excluded before collapsing duplicate rows per `swordArtsParamId`. Native weapon skills remain available separately.
  - Gem XML rows inherit their declared field defaults before compatibility extraction; defaults are profile-specific.
  - `valid_affinities` lists profile affinity names permitted by the gem's `configurableWepAttrNN` flags. Together with mounting permission and weapon types, this is the sole runtime compatibility rule; no per-pair matrix is generated or loaded.
  - Infused weapons may retain their native skill only when the skill is a compatible transferable Ash; native-only skills remain available on Standard weapons.
  - `valid_weapon_types` is pipe-delimited and intended to be matched against `weapon_type_keys`.
- Snapshot schema 4 requires `weapons.csv.can_change_aow` and `aow.csv.valid_affinities`, and removes the redundant compatibility matrix from the runtime file set. Old schema-3 snapshots must be regenerated; changing their version field alone is not a migration.
- Compact diagnostic summaries are derived in memory from the same permission fields. Regression fingerprints preserve all 87,879 Vanilla and 147,201 Convergence legal pairs from the pre-compaction snapshots.
- Generated CSV and manifest JSON files use canonical LF line endings so snapshots hash identically across supported hosts.

## Extraction work directories

Each invocation unpacks the supplied regulation and serializes its parameters with
the supplied WitchyBND into a fresh child of `--workdir`. Existing XML is never
reused: its presence does not establish source or tool provenance. `--keep-workdir`
retains that child for inspection; otherwise only that invocation's child is removed
after successful snapshot validation. Existing work files are left intact.
Manifest provenance hashes the copied regulation used by that invocation.
