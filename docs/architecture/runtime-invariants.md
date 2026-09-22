# Runtime invariants

Use this page as the contract for cache identity, asynchronous jobs, result
selection, snapshot loading, and changes that cross those boundaries.

Status: accepted. These rules describe contracts that tests and future refactors must preserve.

**Navigation:** [Home](../../README.md) · [Optimizer overview](../design/optimizer-overview.md) ·
[Optimizer math](../design/optimizer-math.md) · [Performance](../performance.md) ·
[Extracting game data](../../tools/phase1/README.md)

**Find a contract:** [Cache identity](#cache-identity-and-versioning) ·
[Job lifecycle](#job-lifecycle) · [Result identity](#result-identity) ·
[Data snapshots](#data-snapshots) · [Change checklist](#change-checklist)

## Cache identity and versioning

- Analysis cache keys include the profile ID, every behavior-affecting input, and the dataset schema, dataset version, and model version.
- The active snapshot and scoring identities are recorded in the [model reference](../model-reference.md).
  Storage, dataset, and calculation identities are independent; incompatible snapshots
  or persisted results fail closed rather than being relabeled as a migration.
- A solved-build key uses the stable result fingerprint: weapon ID/name, affinity,
  AoW name, upgrade, and all five combat stats. Reinforcement type belongs to the
  profile-bound weapon identity rather than a separate fingerprint field.
- Caches are bounded. Eviction may reduce performance but must never alter results.
- An aborted subscriber cannot populate a cache entry. When the last subscriber leaves, the pending entry is evicted immediately; backend work is also cancelled when that command exposes cancellation.

## Frontend and native types

- `npm run test:contracts` in `apps/desktop` compares public DTO declarations in
  `src-tauri/src/dto.rs` with `src/lib/types.ts`, including nested fields, arrays,
  nullability and native enum variants. API request construction uses those named
  interfaces. New Rust/serde syntax must be explicitly supported by the checker.
- Deliberate differences are narrowly checked: the frontend sends explicit
  upgrade-policy values and omits older upgrade inputs; older persisted builds
  may lack three optional display fields. Other missing/optional fields fail.
- This source check is not a serializer, numeric-range check or command-registry
  verifier. Rust validators and serialization tests remain required. Native String
  fields can have narrower frontend literal types; permitted values still belong
  to native validation.
- React Hooks ordering and effect dependencies are lint errors. Async effects
  must retain generation/cancellation guards when dependencies change. Comparison
  persistence validation lives in `src/lib/compare-bench.ts`; moving it does not
  change the stored format or the store's job ownership.

## Job lifecycle

- Search, Paths, and Affinity Watch each own a monotonically increasing frontend generation, an input signature containing the profile ID, and at most one backend job ID.
- Paths and Affinity Watch use separate native queues. Cancellation or status
  uncertainty in one queue retains that queue's native ownership without blocking
  the other queue.
- A response may update state only when generation, signature, and job ID all exactly match the active request.
- Input changes invalidate the active generation before dependent state is changed.
- Cancellation is cooperative and fail-closed. A cancelled job cannot replace current rows or populate retained analysis caches.
- Broad running work targets cancellation within 250 ms on the reference machine. Cancelled multi-lane jobs publish no partial success payload.
- Polling has one in-flight request at a time and backs off while progress is unchanged.
- Rankings, custom comparisons, and CSV export share one search queue. The workflow owns polling through native completion, including cancellation; superseded queued requests never start. Rankings UI components do not run a second poller.
- Cancellation or status communication errors may end the caller's request, but do not establish that its native worker is idle. Retain uncertain job ownership, bound recovery attempts, and reconcile that worker before starting replacement work.
- Direct loadout and upgrade-series calculations run off the native main thread, with bounded concurrency and cooperative cancellation. Shared cache work is cancelled only when its last subscriber leaves or its profile/data identity is invalidated.
- A Compare operation requires fresh Rankings results. If one solve or upgrade
  sibling fails, the operation aborts its remaining siblings and reports the
  original error; aborting one subscriber does not cancel shared cached work needed
  by another subscriber.
- Numeric input edits do not launch exact optimizer preparation; the command rail shows a constant-time scope summary and exact candidate preparation begins only when Search is pressed. Search-space estimation has no job lifecycle to preserve: it is a cancellable core API with no command or frontend caller, so nothing can publish an estimate into frontend state. Reintroducing a user-facing estimate means giving it a generation, signature, and job ID like any other async request.
- A profile switch invalidates every job generation before changing inputs, requests cancellation for all active backend jobs, clears profile-bound results, and cannot accept a completion from the previous profile.
- CSV export owns a cancellable search until the backend reports completion, including after cancellation. Normal searches and comparison searches wait for that slot. Input/profile changes and leaving Rankings cancel export; late results cannot download or populate its cache.

## Result identity

- Selection follows the result fingerprint, not row index or visual rank.
- New rankings retain selection only when the exact fingerprint still exists.
- Results retained while inputs change are explicitly stale. Stale rows cannot launch
  Compare, Paths, or Affinity Watch; Compare clears its visible series and target until
  fresh Rankings results exist.
- Saved solved rows are trusted only when schema, dataset, and model versions match the active catalog; otherwise only normalized inputs are loaded.
- Presets have an explicit profile ID. Legacy presets migrate to Vanilla, and presets from another profile cannot be loaded or silently converted.
- Presets save effective stat locks; disabled locks are stored as null. Loadout solving and migration preserve requested locks. Comparison callers explicitly clear locks when reoptimizing a rival's stats.
- Loading a readable preset does not depend on persisting comparison pins. Optional comparison-storage failures leave session state usable and show a warning; essential preset writes still report failure. Obsolete migration requests cancel their native calculations as well as rejecting stale results.
- CSV exports include profile/data/model provenance. Unsupported values are blank,
  never numeric zero; unified upgrade profiles do not claim Standard/Somber
  identity. Export reruns and caches use the complete normalized request including
  profile and requested row count.
- Missing or malformed saved-build indexes are distinct from an empty library.
  Essential writes refuse to replace an unreadable index. Recovery previews bind
  the exact source records/index, reject changed previews, preserve the original
  index before replacement, and leave unreadable records untouched. Storage read
  failures must not throw through component rendering.
- Bulk build backups use `tarnisheds-arsenal.build-backup` version 1, containing
  existing versioned presets, bounded to 500 builds/10 MiB. Validate the entire
  backup before writing; restore with new IDs and commit the index last. Failed
  cleanup leaves new records recoverable as orphans.
- Reproduction reports project known request/result fields, including nested
  structures, and omit raw storage, logs, and private manifest source paths.
  Report text redaction is an additional precaution; users preview before sharing.
  Captures describe current inputs and displayed results, never imply a failed
  request was captured or a saved result independently recalculated.

## Data snapshots

- Each runtime profile is one immutable manifest snapshot. Every required runtime file must be listed exactly once with its byte length and SHA-256 hash.
- External loading is all-or-nothing. Missing, modified, duplicate, unlisted, mixed-version, or path-traversing entries fail startup; files never fall back individually to embedded data.
- The embedded snapshot is validated against the same manifest contract before parsing.
- The runtime profile registry contains an independently validated snapshot for every shipped profile. Commands select one explicit profile and never combine rows, jobs, lanes, caches, or metadata across profiles.
- Every manifest binds its profile ID, display name, capability flags, mechanics rules, source hashes, and whether each source is bundled. Upgrade caps, upgrade-path shape, Scadutree availability, extended grades, status-scaling semantics, and attack-element fallback behavior are profile data and are enforced in both UI and core. Unsupported model areas use explicit capabilities and schema-only tables, never data copied from another profile.
- Native skill identity remains anchored to the source `native_skill_id` when a
  localized label or motion rows are unavailable. The runtime may expose a null
  label and no damage rows, but it must not erase the ID or invent damage data.
- Convergence ammunition rows remain in the immutable source snapshot but are not
  exposed to catalog/search/export until an arrow/bolt projectile model exists. They
  must never compete using weapon-only or duplicated damage components; current profile
  coverage is listed in the [model reference](../model-reference.md).
- A mod profile with a version-bound availability reference extracts only exact referenced configuration IDs. Offline validation compares every common modeled weapon field and status family; a missing, stale, or mismatched reference fails the release gate.
- Required mounting/affinity permissions, attack-provenance fields
  (`is_bullet_attack`, `is_throw_attack`), and weapon `critical_damage_percent` are
  validated before use. Incompatible schema versions fail at the manifest boundary;
  no per-pair compatibility CSV or runtime pair set is retained. The versioned file
  contract is maintained in [Extracting game data](../../tools/phase1/README.md#snapshot-contract).
- Ash compatibility is exactly mounting permission AND affinity membership AND weapon-type intersection. Native-only skills are handled separately and cannot bypass infusion restrictions.
- A fixed-loadout Paths evaluation pins weapon, affinity, Ash, and upgrade, then clears
  discovery filters before preparing its evaluator. Discovery constraints therefore
  cannot leak into a selected path.
- Schema version describes storage compatibility, dataset version identifies extracted content, and model version identifies calculation semantics. They change independently.
- The UI's “Snapshot loaded” state reports a manifest-bound snapshot that passed
  loading checks and declares capabilities; it does not independently verify every
  formula.

## Change checklist

Any cache, async job, result DTO, preset, or snapshot change must update the closest invariant test and run the frontend, core, Tauri packaged-data, and release metadata gates appropriate to that boundary.

## Fixed-loadout tradeoffs

AR/bleed tradeoffs use the shared cancellable analysis worker. Changing the
request/profile/selected loadout or leaving Compare cancels the subscriber and
prevents a late result from appearing. The returned frontier belongs to one
fixed equipment, upgrade, stat-budget, constraint, and handling context.
Shortlist criteria, sacrifice selection, the all-points table, and the optional
plot read that same completed result without starting more optimizer work.
Inspecting an option preserves its exact allocation; it does not feed the option
through the pinned-loadout reoptimizer. Applying it pins the equipment, upgrade,
and all five combat stats through the existing lock/search actions before saving.
The returned result and a saved/reloaded build must preserve that setup, allocation,
AR, and bleed buildup; a matching weapon name alone does not establish this contract.
This feature creates no persisted format.
