# Differential Review: dirty worktree at `46457a5`

## Implementation follow-up

The original findings below are retained as the audit record. Follow-up changes now
introduce schema 4, remove the redundant matrix with exhaustive equivalence
fingerprints, share primary-score DP work while retaining every tie, and add mounted
selector regressions for both tabs. The performance comparison in
`docs/performance.md` records actual before/after medians and identical ranked rows;
Ash counts alone were not a latency multiplier. Native defaults apply on weapon
changes, while restored explicit Automatic selections are intentionally preserved.

Final verification results are recorded in `docs/validation/aow-review-resolution.md`.
The original release-hold verdict describes the reviewed pre-fix state, not the
completed follow-up. No commit or push is part of this work.

## Scope

- Baseline: `HEAD` / `origin/main` at `46457a5` (`Reject non-finite optimizer data`)
- Reviewed: every tracked modification and both untracked source files in the worktree
- Change size: 30 tracked files, 2 untracked source files, approximately 83,556 insertions and 18,627 deletions
- Main themes: exact Ash-of-War compatibility, native-skill handling, generated profile data, and a shared AoW selector

## Findings

### [P1] Ordinary AR searches now run the optimizer once for every compatible Ash

**Locations:**

- `core/er_optimizer_core/src/optimizer.rs:2615`
- `core/er_optimizer_core/src/optimizer.rs:1089`

`resolve_aow_choices` now retains every compatible Ash for every objective. `search_dp_work_unit` then calls `best_objective_allocation` separately for each retained Ash and upgrade. Before this worktree, Max AR, Max Physical AR, and Bleed searches pruned Ashes that could not affect the objective unless an AoW filter explicitly required them.

This is a large multiplicative regression, not a small constant cost. In the regenerated data, a compatible Vanilla weapon row has a median of 29 legal Ashes (P90 53, maximum 66); Convergence has a median of 53 (P90 66, maximum 69). Only six Vanilla Ash IDs currently contribute an attack-power buff. The release benchmark's broad estimates are now 289,383,237,587 combinations at level 96 and 2,912,387,233,342 at level 180.

The new test `open_ar_search_keeps_compatible_aows_for_ranking_ties` makes the expansion intentional, but computing a full DP for every irrelevant Ash is not required to preserve exact tie-breaking. Restore objective pruning for the primary optimization, retain candidates tied on the primary key, then evaluate AoW route metrics only for those ties. The existing lazy tie-completion path in `optimizer/ranking.rs` is the natural place to finish hidden metrics.

### [P1] Required CSV schema changes still advertise schema version 3

**Locations:**

- `tools/phase1/snapshot_manifest.py:20`
- `core/er_optimizer_core/src/snapshot.rs:8`
- `core/er_optimizer_core/src/data.rs:691`

The worktree makes `weapons.csv.can_change_aow` and `aow.csv.valid_affinities` required runtime columns, but both regenerated manifests still declare `schemaVersion: 3`, and the runtime still accepts version 3 as current. An older schema-3 snapshot lacks those required columns and now fails to load even though its declared schema is accepted.

Bump the snapshot schema and update the runtime constant, mocks, and metadata tests together. `modelVersion` and `extractorVersion` describe behavior and provenance; they do not replace the format compatibility contract.

### [P2] The exact compatibility matrix is now redundant generated state

**Locations:**

- `core/er_optimizer_core/src/model.rs:584`
- `tools/phase4/validate_phase4.py:79`
- `tools/phase1/derive_phase1_raw_extras.py:325`

The runtime now checks all three source predicates directly: `can_change_aow`, `valid_affinities`, and `valid_weapon_types`. It additionally requires the pair to exist in `exact_aow_compat`, but that CSV is generated from those same three predicates and the new validator merely reconstructs the same set and compares it.

The duplicate representation contains 235,080 rows and occupies 16,714,885 bytes across the two profiles. It also adds CSV parsing, a large runtime `HashSet`, manifest entries, generator code, and validation code without adding independent information. Delete `aow_weapon_compat.csv` and derive legality from the compact fields, or make the matrix the sole source of truth; keeping both is the most complex option.

### [P3] The shared selector's defaulting test does not exercise its mount behavior

**Locations:**

- `apps/desktop/src/lib/AowSelect.tsx:34`
- `apps/desktop/src/lib/AowSelect.tsx:54`
- `apps/desktop/src/lib/AowSelect.test.ts:11`

The test says a Buckler defaults to Buckler Parry, but it tests only `resolveAowSelection(..., weaponChanged: true)`. The component initializes `previousWeapon` to the already-selected weapon, so mounting with a restored/pre-populated Buckler and a null AoW passes `weaponChanged: false` and leaves the selection on Automatic. The asynchronous component behavior, abort handling, and parent `onChange` interaction are untested.

Either initialize the previous key to null so first load counts as a weapon transition, or rename the test/behavior to state that defaulting occurs only after an in-session weapon change. One small component-level regression test is sufficient.

## What looks correct

- `gemMountType` is correctly separated from `disable_gem_attr`.
- XML defaults are applied before compatibility extraction, including Convergence's extended affinity slots.
- Native-only placeholder gems 117, 223, and 303 are removed from transferable Ash data while native Firebreather attack rows remain available.
- Runtime compatibility now fails closed on mounting permission, affinity, weapon type, and the exact pair.
- Standard native skills remain available without pretending they are applied Ashes; incompatible infused native skills are rejected.
- The shared `AowSelect` removes duplicated loading code from Rankings and Compare.
- Generated manifests match their files, compatibility pairs contain no duplicates, and profile validation reports zero warnings.

## Verification

- `python -m unittest tools.phase1.test_phase1_dump tools.phase4.test_validate_phase4` — 11 passed
- `python tools/phase4/validate_phase4.py --diagnostic` — passed, 0 warnings
- `cargo test --manifest-path core/er_optimizer_core/Cargo.toml -q` — 133 passed, 2 ignored
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -q` — 22 passed, 3 ignored
- `npm test -- --run` — 56 passed
- `npx tsc --noEmit` — passed
- `cargo fmt --all --manifest-path core/er_optimizer_core/Cargo.toml -- --check` — passed
- `git diff --check` — no whitespace errors
- Release estimate benchmark — passed; figures cited in P1

## Worktree hygiene

- `apps/desktop/src-tauri/Cargo.toml` is a stat-only false positive: its worktree blob hash exactly matches `HEAD`.
- `.codex-tmp/` is locally excluded through `.git/info/exclude` and is not part of the dirty diff, but it contains 420 files totaling 210,283,504 bytes, including copied `regulation.bin` files, unpacked PARAM XML, executables, PDBs, and earlier audit output. Remove it when this audit evidence is no longer needed.
- No branches or extra worktrees are involved: only `main` exists locally, tracks `origin/main`, and is not ahead or behind.
- No credential-like values were found in the changed source.

## Verdict

Do not commit or release this worktree yet. The compatibility correction is well supported, but the schema contract must be bumped and the all-Ash search expansion should be redesigned or benchmarked against an explicit release budget. The redundant compatibility matrix is safe cleanup for the same follow-up and materially reduces repository and runtime bloat.
